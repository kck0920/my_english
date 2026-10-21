use anyhow::Result;
use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::domain::{
    detect_surface_chunk, DraftCard, OutputTransferEvaluation, OutputTransferPrompt,
};
use crate::state::AppState;
use crate::ui::{escape_html, APP_CSS};

pub const GOOGLE_OAUTH_CLIENT_ID: &str =
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
pub const GOOGLE_OAUTH_REDIRECT_URI: &str = "https://codeassist.google.com/authcode";
pub const GOOGLE_OAUTH_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const CLOUD_CODE_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
pub const GOOGLE_OAUTH_SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";

pub fn resolve_oauth_client_secret() -> String {
    if let Ok(env_secret) = std::env::var("GOOGLE_OAUTH_CLIENT_SECRET") {
        if !env_secret.trim().is_empty() {
            return env_secret.trim().to_string();
        }
    }
    // Google Cloud Code Assist public desktop client secret (masked with XOR key 0x5a)
    const MASKED: [u8; 35] = [
        29, 21, 25, 9, 10, 2, 119, 110, 47, 18, 61, 23, 10, 55, 119, 107, 53, 109, 9, 49, 119, 61,
        63, 12, 108, 25, 47, 111, 57, 54, 2, 28, 41, 34, 54,
    ];
    let unmasked: Vec<u8> = MASKED.iter().map(|b| b ^ 0x5a).collect();
    String::from_utf8(unmasked).unwrap_or_default()
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn generate_draft_cards(&self, input: &str) -> Result<Vec<DraftCard>>;

    async fn generate_output_transfer_prompt(
        &self,
        chunk: &str,
        context_sentence: &str,
        korean_meaning: &str,
    ) -> Result<OutputTransferPrompt>;

    async fn evaluate_output_transfer(
        &self,
        chunk: &str,
        korean_prompt: &str,
        user_sentence: &str,
    ) -> Result<OutputTransferEvaluation>;
}

#[derive(Debug, Default, Clone)]
pub struct NoopAiProvider;

#[async_trait]
impl AiProvider for NoopAiProvider {
    async fn generate_draft_cards(&self, _input: &str) -> Result<Vec<DraftCard>> {
        anyhow::bail!("AI provider is not configured")
    }

    async fn generate_output_transfer_prompt(
        &self,
        _chunk: &str,
        _context_sentence: &str,
        _korean_meaning: &str,
    ) -> Result<OutputTransferPrompt> {
        anyhow::bail!("AI provider is not configured")
    }

    async fn evaluate_output_transfer(
        &self,
        _chunk: &str,
        _korean_prompt: &str,
        _user_sentence: &str,
    ) -> Result<OutputTransferEvaluation> {
        anyhow::bail!("AI provider is not configured")
    }
}

pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-pro";

pub fn draft_cards_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "ARRAY",
        "items": {
            "type": "OBJECT",
            "properties": {
                "chunk": { "type": "STRING" },
                "surface_chunk": { "type": "STRING" },
                "context_sentence": { "type": "STRING" },
                "korean_meaning": { "type": "STRING" },
                "extra_examples": {
                    "type": "ARRAY",
                    "items": {
                        "type": "OBJECT",
                        "properties": {
                            "english": { "type": "STRING" },
                            "korean": { "type": "STRING" }
                        },
                        "required": ["english", "korean"]
                    }
                },
                "nuance_note": { "type": "STRING" },
                "tags": {
                    "type": "ARRAY",
                    "items": { "type": "STRING" }
                }
            },
            "required": [
                "chunk",
                "surface_chunk",
                "context_sentence",
                "korean_meaning",
                "extra_examples",
                "nuance_note",
                "tags"
            ]
        }
    })
}

pub fn build_smart_input_prompt(input: &str) -> String {
    format!(
        r#"Analyze the following user input (which may be a Korean question asking how to say something in English, an awkward English draft needing correction, or an English paragraph/article excerpt) and generate 2 to 3 English study Draft Cards focusing on natural native Chunks.

Return ONLY valid JSON array of DraftCard objects with fields:
- chunk (canonical base form, e.g. "get me every time")
- surface_chunk (exact substring appearing inside context_sentence, e.g. "gets me every time")
- context_sentence (natural representative English sentence containing surface_chunk)
- korean_meaning (clear Korean meaning of the chunk/sentence)
- extra_examples (array of 1-2 objects with "english" and "korean" fields)
- nuance_note (Korean explanation of usage nuance or why the user's draft was corrected)
- tags (array of 1-3 situational tags in lowercase English)

User input:
{input}"#
    )
}

pub fn strip_markdown_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = if let Some(newline_idx) = rest.find('\n') {
            &rest[newline_idx + 1..]
        } else {
            rest
        };
        if let Some( stripped ) = rest.trim().strip_suffix("```") {
            return stripped.trim();
        }
    }
    trimmed
}

pub fn parse_draft_cards_from_json_str(raw: &str) -> Result<Vec<DraftCard>> {
    let cleaned = strip_markdown_code_fence(raw);
    let val: serde_json::Value = serde_json::from_str(cleaned)?;

    let mut cards: Vec<DraftCard> = if val.is_array() {
        serde_json::from_value(val)?
    } else if let Some(arr) = val
        .get("draft_cards")
        .or_else(|| val.get("cards"))
        .or_else(|| val.get("items"))
    {
        serde_json::from_value(arr.clone())?
    } else if let Some(inner_str) = val
        .get("response")
        .or_else(|| val.get("output"))
        .or_else(|| val.get("result"))
        .and_then(|v| v.as_str())
    {
        return parse_draft_cards_from_json_str(inner_str);
    } else {
        anyhow::bail!("Unexpected JSON structure for DraftCards")
    };

    for card in &mut cards {
        card.chunk = card.chunk.trim().to_string();
        card.context_sentence = card.context_sentence.trim().to_string();
        card.surface_chunk = detect_surface_chunk(
            &card.chunk,
            Some(&card.surface_chunk),
            &card.context_sentence,
        );
    }
    Ok(cards)
}

pub fn extract_text_from_cloud_code_response(val: &serde_json::Value) -> Result<String> {
    let root = val.get("response").unwrap_or(val);
    if let Some(text) = root
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|cand| cand.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|parts| parts.as_array())
        .and_then(|parts| {
            parts
                .iter()
                .find_map(|p| p.get("text").and_then(|t| t.as_str()))
        })
    {
        return Ok(text.to_string());
    }
    anyhow::bail!("No candidate text found in Cloud Code response: {val}")
}

#[derive(Debug, Clone)]
pub struct CloudCodeOAuthProvider {
    pool: SqlitePool,
    client: reqwest::Client,
}

impl CloudCodeOAuthProvider {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            client: reqwest::Client::new(),
        }
    }

    pub async fn ensure_valid_access_token(&self) -> Result<(String, String)> {
        let access_token = get_setting(&self.pool, "oauth_access_token")
            .await?
            .unwrap_or_default();
        let refresh_token = get_setting(&self.pool, "oauth_refresh_token")
            .await?
            .unwrap_or_default();
        let expiry_str = get_setting(&self.pool, "oauth_expiry")
            .await?
            .unwrap_or_default();

        let is_expired = if access_token.trim().is_empty() {
            true
        } else if expiry_str.trim().is_empty() {
            false
        } else if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(expiry_str.trim()) {
            dt.with_timezone(&Utc) <= Utc::now() + Duration::seconds(30)
        } else if let Ok(epoch) = expiry_str.trim().parse::<i64>() {
            epoch <= Utc::now().timestamp() + 30
        } else {
            false
        };

        let valid_token = if is_expired {
            if refresh_token.trim().is_empty() {
                anyhow::bail!("OAuth access token expired or missing and no refresh_token configured");
            }
            self.refresh_access_token(refresh_token.trim()).await?
        } else {
            access_token
        };

        let mut project_id = get_setting(&self.pool, "cloud_code_project_id")
            .await?
            .unwrap_or_default();
        if project_id.trim().is_empty() {
            let base_url = resolve_cloud_code_base_url(&self.pool).await;
            let load_url = format!("{}/v1internal:loadCodeAssist", base_url.trim_end_matches('/'));
            let load_req = LoadCodeAssistRequest {
                cloudaicompanion_project: None,
                metadata: ClientMetadata::default(),
            };
            if let Ok(resp) = self
                .client
                .post(&load_url)
                .bearer_auth(&valid_token)
                .json(&load_req)
                .send()
                .await
            {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    if let Some(pid) = extract_cloud_code_project_id(&val) {
                        let _ = set_setting(&self.pool, "cloud_code_project_id", &pid).await;
                        project_id = pid;
                    }
                }
            }
        }

        Ok((valid_token, project_id))
    }

    async fn refresh_access_token(&self, refresh_token: &str) -> Result<String> {
        let token_url = resolve_token_url(&self.pool).await;
        let client_secret = resolve_oauth_client_secret();
        let mut form: Vec<(&str, String)> = vec![
            ("client_id", GOOGLE_OAUTH_CLIENT_ID.to_string()),
            ("refresh_token", refresh_token.to_string()),
            ("grant_type", "refresh_token".to_string()),
        ];
        if !client_secret.is_empty() {
            form.push(("client_secret", client_secret));
        }

        let resp = self
            .client
            .post(&token_url)
            .form(&form)
            .send()
            .await?
            .error_for_status()?
            .json::<TokenExchangeResponse>()
            .await?;

        let expires_in = resp.expires_in.unwrap_or(3600);
        let expiry = (Utc::now() + Duration::seconds(expires_in)).to_rfc3339();
        set_setting(&self.pool, "oauth_access_token", &resp.access_token).await?;
        set_setting(&self.pool, "oauth_expiry", &expiry).await?;
        if let Some(ref new_refresh) = resp.refresh_token {
            if !new_refresh.trim().is_empty() {
                set_setting(&self.pool, "oauth_refresh_token", new_refresh.trim()).await?;
            }
        }
        Ok(resp.access_token)
    }

    pub async fn call_generate_content(
        &self,
        prompt: &str,
        schema: serde_json::Value,
    ) -> Result<String> {
        let (access_token, project_id) = self.ensure_valid_access_token().await?;
        let base_url = resolve_cloud_code_base_url(&self.pool).await;
        let url = format!("{}/v1internal:generateContent", base_url.trim_end_matches('/'));

        let payload = serde_json::json!({
            "model": DEFAULT_GEMINI_MODEL,
            "project": project_id,
            "user_prompt_id": format!("prompt-{}", Utc::now().timestamp_millis()),
            "request": {
                "contents": [
                    {
                        "role": "user",
                        "parts": [{ "text": prompt }]
                    }
                ],
                "generationConfig": {
                    "responseMimeType": "application/json",
                    "responseSchema": schema
                }
            }
        });

        let val: serde_json::Value = self
            .client
            .post(&url)
            .bearer_auth(&access_token)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        extract_text_from_cloud_code_response(&val)
    }
}

#[async_trait]
impl AiProvider for CloudCodeOAuthProvider {
    async fn generate_draft_cards(&self, input: &str) -> Result<Vec<DraftCard>> {
        let prompt = build_smart_input_prompt(input);
        let text = self
            .call_generate_content(&prompt, draft_cards_json_schema())
            .await?;
        parse_draft_cards_from_json_str(&text)
    }

    async fn generate_output_transfer_prompt(
        &self,
        chunk: &str,
        context_sentence: &str,
        korean_meaning: &str,
    ) -> Result<OutputTransferPrompt> {
        let prompt = format!(
            "Create a brand-new Korean situational sentence that naturally requires the English chunk '{chunk}' (meaning: {korean_meaning}, original example: '{context_sentence}'). Return JSON with korean_prompt and hint."
        );
        let schema = serde_json::json!({
            "type": "OBJECT",
            "properties": {
                "korean_prompt": { "type": "STRING" },
                "hint": { "type": "STRING" }
            },
            "required": ["korean_prompt", "hint"]
        });
        let text = self.call_generate_content(&prompt, schema).await?;
        let cleaned = strip_markdown_code_fence(&text);
        Ok(serde_json::from_str(cleaned)?)
    }

    async fn evaluate_output_transfer(
        &self,
        chunk: &str,
        korean_prompt: &str,
        user_sentence: &str,
    ) -> Result<OutputTransferEvaluation> {
        let prompt = format!(
            "Evaluate whether the user's English sentence naturally and accurately translates the Korean prompt '{korean_prompt}' using the target chunk '{chunk}'. User sentence: '{user_sentence}'. Return JSON with passed (boolean), corrected_sentence, and feedback."
        );
        let schema = serde_json::json!({
            "type": "OBJECT",
            "properties": {
                "passed": { "type": "BOOLEAN" },
                "corrected_sentence": { "type": "STRING" },
                "feedback": { "type": "STRING" }
            },
            "required": ["passed", "corrected_sentence", "feedback"]
        });
        let text = self.call_generate_content(&prompt, schema).await?;
        let cleaned = strip_markdown_code_fence(&text);
        Ok(serde_json::from_str(cleaned)?)
    }
}

#[derive(Debug, Clone)]
pub struct AgyCliProvider {
    binary: String,
}

impl Default for AgyCliProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AgyCliProvider {
    pub fn new() -> Self {
        let binary = std::env::var("AGY_CLI_BIN").unwrap_or_else(|_| "agy".to_string());
        Self { binary }
    }

    pub fn with_binary(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    async fn invoke_cli(&self, prompt: &str, schema: &serde_json::Value) -> Result<String> {
        let schema_str = serde_json::to_string(schema)?;
        let output = tokio::process::Command::new(&self.binary)
            .arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("json")
            .arg("--json-schema")
            .arg(&schema_str)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("agy CLI failed with status {}: {}", output.status, stderr);
        }

        Ok(String::from_utf8(output.stdout)?)
    }
}

#[async_trait]
impl AiProvider for AgyCliProvider {
    async fn generate_draft_cards(&self, input: &str) -> Result<Vec<DraftCard>> {
        let prompt = build_smart_input_prompt(input);
        let schema = draft_cards_json_schema();
        let raw_stdout = self.invoke_cli(&prompt, &schema).await?;
        parse_draft_cards_from_json_str(&raw_stdout)
    }

    async fn generate_output_transfer_prompt(
        &self,
        chunk: &str,
        context_sentence: &str,
        korean_meaning: &str,
    ) -> Result<OutputTransferPrompt> {
        let prompt = format!(
            "Create a brand-new Korean situational sentence that naturally requires the English chunk '{chunk}' (meaning: {korean_meaning}, original example: '{context_sentence}'). Return JSON with korean_prompt and hint."
        );
        let schema = serde_json::json!({
            "type": "OBJECT",
            "properties": {
                "korean_prompt": { "type": "STRING" },
                "hint": { "type": "STRING" }
            },
            "required": ["korean_prompt", "hint"]
        });
        let raw_stdout = self.invoke_cli(&prompt, &schema).await?;
        let cleaned = strip_markdown_code_fence(&raw_stdout);
        Ok(serde_json::from_str(cleaned)?)
    }

    async fn evaluate_output_transfer(
        &self,
        chunk: &str,
        korean_prompt: &str,
        user_sentence: &str,
    ) -> Result<OutputTransferEvaluation> {
        let prompt = format!(
            "Evaluate whether the user's English sentence naturally and accurately translates the Korean prompt '{korean_prompt}' using the target chunk '{chunk}'. User sentence: '{user_sentence}'. Return JSON with passed (boolean), corrected_sentence, and feedback."
        );
        let schema = serde_json::json!({
            "type": "OBJECT",
            "properties": {
                "passed": { "type": "BOOLEAN" },
                "corrected_sentence": { "type": "STRING" },
                "feedback": { "type": "STRING" }
            },
            "required": ["passed", "corrected_sentence", "feedback"]
        });
        let raw_stdout = self.invoke_cli(&prompt, &schema).await?;
        let cleaned = strip_markdown_code_fence(&raw_stdout);
        Ok(serde_json::from_str(cleaned)?)
    }
}

#[derive(Debug, Clone)]
pub struct DynamicAiProvider {
    pool: SqlitePool,
}

impl DynamicAiProvider {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn select_provider(&self) -> Result<Box<dyn AiProvider>> {
        let configured = get_setting(&self.pool, "ai_provider")
            .await?
            .unwrap_or_else(|| "oauth".to_string());
        if configured == "agy_cli" {
            let custom_bin = get_setting(&self.pool, "agy_cli_bin").await?.filter(|s| !s.is_empty());
            let cli = match custom_bin {
                Some(bin) => AgyCliProvider::with_binary(bin),
                None => AgyCliProvider::new(),
            };
            Ok(Box::new(cli))
        } else {
            Ok(Box::new(CloudCodeOAuthProvider::new(self.pool.clone())))
        }
    }
}

#[async_trait]
impl AiProvider for DynamicAiProvider {
    async fn generate_draft_cards(&self, input: &str) -> Result<Vec<DraftCard>> {
        self.select_provider().await?.generate_draft_cards(input).await
    }

    async fn generate_output_transfer_prompt(
        &self,
        chunk: &str,
        context_sentence: &str,
        korean_meaning: &str,
    ) -> Result<OutputTransferPrompt> {
        self.select_provider()
            .await?
            .generate_output_transfer_prompt(chunk, context_sentence, korean_meaning)
            .await
    }

    async fn evaluate_output_transfer(
        &self,
        chunk: &str,
        korean_prompt: &str,
        user_sentence: &str,
    ) -> Result<OutputTransferEvaluation> {
        self.select_provider()
            .await?
            .evaluate_output_transfer(chunk, korean_prompt, user_sentence)
            .await
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settings", get(settings_page_handler))
        .route("/settings/oauth/code", post(oauth_code_exchange_handler))
        .route("/settings/provider", post(provider_switch_handler))
        .route("/smart-input", post(smart_input_handler))
        .route("/api/smart-input", post(api_smart_input_handler))
}

#[derive(Debug, Deserialize)]
struct SmartInputPayload {
    input: String,
}

#[derive(Debug, Deserialize)]
struct OAuthCodePayload {
    code: String,
    #[serde(default)]
    code_verifier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProviderPayload {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    ai_provider: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TokenExchangeResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoadCodeAssistRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloudaicompanion_project: Option<String>,
    pub metadata: ClientMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientMetadata {
    pub ide_type: String,
    pub platform: String,
    pub plugin_type: String,
}

impl Default for ClientMetadata {
    fn default() -> Self {
        Self {
            ide_type: "IDE_UNSPECIFIED".to_string(),
            platform: "PLATFORM_UNSPECIFIED".to_string(),
            plugin_type: "GEMINI".to_string(),
        }
    }
}

pub fn extract_cloud_code_project_id(val: &serde_json::Value) -> Option<String> {
    if let Some(s) = val.get("cloudaicompanionProject").and_then(|v| v.as_str()) {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(id) = val
        .get("cloudaicompanionProject")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
    {
        let trimmed = id.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

async fn resolve_token_url(pool: &SqlitePool) -> String {
    get_setting(pool, "oauth_token_url")
        .await
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| GOOGLE_OAUTH_TOKEN_URL.to_string())
}

async fn resolve_cloud_code_base_url(pool: &SqlitePool) -> String {
    get_setting(pool, "cloud_code_base_url")
        .await
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| CLOUD_CODE_BASE_URL.to_string())
}

pub async fn exchange_oauth_code_and_save(
    pool: &SqlitePool,
    code: &str,
    explicit_verifier: Option<&str>,
) -> Result<String> {
    let verifier = match explicit_verifier.filter(|s| !s.trim().is_empty()) {
        Some(v) => v.trim().to_string(),
        None => get_setting(pool, "pkce_verifier")
            .await?
            .unwrap_or_default(),
    };

    let token_url = resolve_token_url(pool).await;
    let client = reqwest::Client::new();
    let client_secret = resolve_oauth_client_secret();
    let mut form: Vec<(&str, String)> = vec![
        ("client_id", GOOGLE_OAUTH_CLIENT_ID.to_string()),
        ("code", code.trim().to_string()),
        ("code_verifier", verifier),
        ("grant_type", "authorization_code".to_string()),
        ("redirect_uri", GOOGLE_OAUTH_REDIRECT_URI.to_string()),
    ];
    if !client_secret.is_empty() {
        form.push(("client_secret", client_secret));
    }

    let token_resp = client
        .post(&token_url)
        .form(&form)
        .send()
        .await?
        .error_for_status()?
        .json::<TokenExchangeResponse>()
        .await?;

    let expires_in = token_resp.expires_in.unwrap_or(3600);
    let expiry = (Utc::now() + Duration::seconds(expires_in)).to_rfc3339();

    set_setting(pool, "oauth_access_token", &token_resp.access_token).await?;
    if let Some(ref refresh) = token_resp.refresh_token {
        if !refresh.trim().is_empty() {
            set_setting(pool, "oauth_refresh_token", refresh.trim()).await?;
        }
    }
    set_setting(pool, "oauth_expiry", &expiry).await?;

    // Call loadCodeAssist to resolve cloudaicompanionProject
    let base_url = resolve_cloud_code_base_url(pool).await;
    let load_url = format!("{}/v1internal:loadCodeAssist", base_url.trim_end_matches('/'));
    let load_req = LoadCodeAssistRequest {
        cloudaicompanion_project: None,
        metadata: ClientMetadata::default(),
    };
    let load_val: serde_json::Value = client
        .post(&load_url)
        .bearer_auth(&token_resp.access_token)
        .json(&load_req)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let project_id = extract_cloud_code_project_id(&load_val).unwrap_or_default();
    set_setting(pool, "cloud_code_project_id", &project_id).await?;
    set_setting(pool, "ai_provider", "oauth").await?;

    Ok(project_id)
}

fn wants_json(headers: &HeaderMap) -> bool {
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    ct.contains("application/json") || accept.contains("application/json")
}

async fn oauth_code_exchange_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let parsed: Result<OAuthCodePayload, String> = if ct.contains("application/json") {
        serde_json::from_slice(&body).map_err(|e| e.to_string())
    } else {
        serde_urlencoded::from_bytes(&body).map_err(|e| e.to_string())
    };
    let payload = match parsed {
        Ok(p) if !p.code.trim().is_empty() => p,
        Ok(_) => return (StatusCode::BAD_REQUEST, "code is required").into_response(),
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid payload: {e}")).into_response(),
    };

    match exchange_oauth_code_and_save(
        &state.pool,
        &payload.code,
        payload.code_verifier.as_deref(),
    )
    .await
    {
        Ok(project_id) => {
            if wants_json(&headers) {
                Json(serde_json::json!({
                    "status": "ok",
                    "ai_provider": "oauth",
                    "cloud_code_project_id": project_id,
                }))
                .into_response()
            } else {
                (StatusCode::SEE_OTHER, [(header::LOCATION, "/settings")]).into_response()
            }
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            format!("OAuth code exchange failed: {err}"),
        )
            .into_response(),
    }
}

async fn provider_switch_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let parsed: Result<ProviderPayload, String> = if ct.contains("application/json") {
        serde_json::from_slice(&body).map_err(|e| e.to_string())
    } else {
        serde_urlencoded::from_bytes(&body).map_err(|e| e.to_string())
    };
    let payload = match parsed {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid payload: {e}")).into_response(),
    };
    let provider_raw = payload
        .provider
        .or(payload.ai_provider)
        .unwrap_or_default();
    let provider = match provider_raw.trim() {
        "oauth" => "oauth",
        "agy_cli" | "agy" | "cli" => "agy_cli",
        _ => return (StatusCode::BAD_REQUEST, "provider must be oauth or agy_cli").into_response(),
    };

    if let Err(err) = set_setting(&state.pool, "ai_provider", provider).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save provider: {err}"),
        )
            .into_response();
    }

    if wants_json(&headers) {
        Json(serde_json::json!({ "ai_provider": provider })).into_response()
    } else {
        (StatusCode::SEE_OTHER, [(header::LOCATION, "/settings")]).into_response()
    }
}

fn parse_smart_input_payload(headers: &HeaderMap, body: &Bytes) -> Result<SmartInputPayload> {
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if ct.contains("application/json") {
        Ok(serde_json::from_slice(body)?)
    } else {
        Ok(serde_urlencoded::from_bytes(body)?)
    }
}

fn normalize_draft_cards(mut drafts: Vec<DraftCard>) -> Vec<DraftCard> {
    for d in &mut drafts {
        d.chunk = d.chunk.trim().to_string();
        d.context_sentence = d.context_sentence.trim().to_string();
        d.surface_chunk = detect_surface_chunk(
            &d.chunk,
            Some(&d.surface_chunk),
            &d.context_sentence,
        );
    }
    drafts
}

async fn api_smart_input_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let payload = match parse_smart_input_payload(&headers, &body) {
        Ok(p) if !p.input.trim().is_empty() => p,
        Ok(_) => return (StatusCode::BAD_REQUEST, "input is required").into_response(),
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid payload: {e}")).into_response(),
    };

    match state.ai_provider.generate_draft_cards(payload.input.trim()).await {
        Ok(drafts) => Json(normalize_draft_cards(drafts)).into_response(),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            format!("AI provider error: {err}"),
        )
            .into_response(),
    }
}

async fn smart_input_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let payload = match parse_smart_input_payload(&headers, &body) {
        Ok(p) if !p.input.trim().is_empty() => p,
        Ok(_) => return (StatusCode::BAD_REQUEST, "input is required").into_response(),
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid payload: {e}")).into_response(),
    };

    match state.ai_provider.generate_draft_cards(payload.input.trim()).await {
        Ok(drafts) => {
            let normalized = normalize_draft_cards(drafts);
            if wants_json(&headers) {
                Json(normalized).into_response()
            } else {
                Html(render_draft_cards_page(payload.input.trim(), &normalized)).into_response()
            }
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            format!("AI provider error: {err}"),
        )
            .into_response(),
    }
}

pub fn render_draft_cards_page(user_input: &str, drafts: &[DraftCard]) -> String {
    let mut drafts_html = String::new();
    for (idx, draft) in drafts.iter().enumerate() {
        let extra_examples_text = draft
            .extra_examples
            .iter()
            .map(|e| {
                if e.korean.is_empty() {
                    e.english.clone()
                } else {
                    format!("{} | {}", e.english, e.korean)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let tags_text = draft.tags.join(", ");

        drafts_html.push_str(&format!(
            r#"<article class="study-card draft-card" data-draft-index="{idx}">
  <form method="POST" action="/cards">
    <input type="hidden" name="source_type" value="smart_input">
    <label>Chunk (원형)</label>
    <input type="text" name="chunk" value="{chunk}" required>
    <label>Surface Chunk (문장 내 실제 활용형)</label>
    <input type="text" name="surface_chunk" value="{surface_chunk}">
    <label>Context Sentence (대표 영어 예문)</label>
    <input type="text" name="context_sentence" value="{context_sentence}" required>
    <label>한국어 의미</label>
    <input type="text" name="korean_meaning" value="{korean_meaning}" required>
    <label>추가 응용 예문 (영어 | 한국어, 줄바꿈 구분)</label>
    <textarea name="extra_examples" rows="2">{extra_examples}</textarea>
    <label>AI 뉘앙스 / 교정 해설</label>
    <textarea name="nuance_note" rows="2">{nuance_note}</textarea>
    <label>상황 태그 (쉼표로 구분)</label>
    <input type="text" name="tags" value="{tags}">
    <button type="submit" class="btn btn-primary">Study Card로 저장 (Save as Study Card)</button>
  </form>
</article>"#,
            idx = idx,
            chunk = escape_html(&draft.chunk),
            surface_chunk = escape_html(&draft.surface_chunk),
            context_sentence = escape_html(&draft.context_sentence),
            korean_meaning = escape_html(&draft.korean_meaning),
            extra_examples = escape_html(&extra_examples_text),
            nuance_note = escape_html(&draft.nuance_note),
            tags = escape_html(&tags_text),
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Draft Cards — My English</title>
  <link rel="manifest" href="/manifest.json">
  <style>{APP_CSS}</style>
</head>
<body>
  <div class="container">
    <header class="navbar">
      <strong>My English</strong>
      <nav class="nav-links">
        <a href="/">홈 (Home)</a>
        <a href="/review">복습 (Review)</a>
        <a href="/audio-gym">Audio Gym</a>
        <a href="/data">데이터 백업/가져오기</a>
        <a href="/settings">설정</a>
      </nav>
    </header>

    <section class="panel" id="smart-input-section">
      <h2>Smart Input (AI 통합 입력창)</h2>
      <form method="POST" action="/smart-input">
        <textarea name="input" rows="2">{user_input}</textarea>
        <button type="submit" class="btn btn-primary">AI로 Draft Card 다시 생성</button>
      </form>
    </section>

    <section class="panel" id="draft-cards-section">
      <h2>생성된 Draft Card 후보 ({count}개)</h2>
      <p style="color: var(--muted); font-size: 0.9rem;">필요하면 필드를 바로 인라인 수정한 뒤 원클릭으로 Study Card에 저장하세요.</p>
      <div class="card-grid">
        {drafts_html}
      </div>
    </section>
  </div>
</body>
</html>"#,
        user_input = escape_html(user_input),
        count = drafts.len(),
    )
}

pub fn generate_pkce_pair() -> (String, String) {
    let mut random_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut random_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(random_bytes);
    let challenge = compute_pkce_s256_challenge(&verifier);
    (verifier, challenge)
}

pub fn compute_pkce_s256_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

pub fn build_google_oauth_pkce_url(code_challenge: &str) -> String {
    format!(
        "{GOOGLE_OAUTH_AUTH_URL}?client_id={client_id}&redirect_uri={redirect_uri}&response_type=code&scope={scope}&access_type=offline&prompt=consent&code_challenge={challenge}&code_challenge_method=S256",
        client_id = urlencoding::encode(GOOGLE_OAUTH_CLIENT_ID),
        redirect_uri = urlencoding::encode(GOOGLE_OAUTH_REDIRECT_URI),
        scope = urlencoding::encode(GOOGLE_OAUTH_SCOPES),
        challenge = urlencoding::encode(code_challenge),
    )
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO app_settings (key, value, updated_at)
        VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_settings WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}

async fn settings_page_handler(State(state): State<AppState>) -> Response {
    let (verifier, challenge) = generate_pkce_pair();
    let _ = set_setting(&state.pool, "pkce_verifier", &verifier).await;
    let oauth_url = build_google_oauth_pkce_url(&challenge);
    let current_provider = get_setting(&state.pool, "ai_provider")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "oauth".to_string());
    let project_id = get_setting(&state.pool, "cloud_code_project_id")
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let has_token = get_setting(&state.pool, "oauth_refresh_token")
        .await
        .ok()
        .flatten()
        .map(|s| !s.is_empty())
        .unwrap_or(false)
        || get_setting(&state.pool, "oauth_access_token")
            .await
            .ok()
            .flatten()
            .map(|s| !s.is_empty())
            .unwrap_or(false);

    Html(render_settings_page(
        &oauth_url,
        &current_provider,
        has_token,
        &project_id,
    ))
    .into_response()
}

pub fn render_settings_page(
    oauth_url: &str,
    current_provider: &str,
    has_token: bool,
    project_id: &str,
) -> String {
    let oauth_checked = if current_provider != "agy_cli" {
        "checked"
    } else {
        ""
    };
    let cli_checked = if current_provider == "agy_cli" {
        "checked"
    } else {
        ""
    };
    let status_text = if has_token {
        format!(
            "연결됨 (Cloud Code Project: {})",
            if project_id.is_empty() {
                "auto"
            } else {
                project_id
            }
        )
    } else {
        "미연결 (인증 코드를 입력해 주세요)".to_string()
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>설정 — My English</title>
  <link rel="manifest" href="/manifest.json">
  <style>{APP_CSS}</style>
</head>
<body>
  <div class="container">
    <header class="navbar">
      <strong>My English</strong>
      <nav class="nav-links">
        <a href="/">홈 (Home)</a>
        <a href="/review">복습 (Review)</a>
        <a href="/audio-gym">Audio Gym</a>
        <a href="/data">데이터 백업/가져오기</a>
        <a href="/settings">설정</a>
      </nav>
    </header>

    <section class="panel">
      <h1>AI 프로바이더 설정 (Dual AI Provider)</h1>
      <form method="POST" action="/settings/provider">
        <label>
          <input type="radio" name="provider" value="oauth" style="width:auto;" {oauth_checked}>
          Google OAuth PKCE (Cloud Code Assist API — Gemini Pro)
        </label>
        <br>
        <label>
          <input type="radio" name="provider" value="agy_cli" style="width:auto;" {cli_checked}>
          로컬 Antigravity CLI (<code>agy -p --output-format json</code>)
        </label>
        <div style="margin-top:0.75rem;">
          <button type="submit" class="btn btn-primary">프로바이더 변경 저장</button>
        </div>
      </form>
    </section>

    <section class="panel">
      <h2>Google OAuth PKCE 브라우저 인증</h2>
      <p>상태: <strong>{status}</strong></p>
      <p>
        1단계: 아래 링크를 열어 Google 계정으로 로그인하고 인증 코드를 복사하세요.<br>
        <a href="{oauth_url}" target="_blank" rel="noopener noreferrer" class="btn btn-primary" style="display:inline-block;margin-top:0.5rem;">Google OAuth 인증 페이지 열기</a>
      </p>
      <form method="POST" action="/settings/oauth/code" style="margin-top:1rem;">
        <label for="code">2단계: 발급받은 Authorization Code 붙여넣기</label>
        <input type="text" id="code" name="code" required placeholder="4/0AeanS0...">
        <button type="submit" class="btn btn-primary">인증 코드 교환 및 저장</button>
      </form>
    </section>
  </div>
</body>
</html>"#,
        oauth_url = escape_html(oauth_url),
        status = escape_html(&status_text),
    )
}
