use anyhow::Result;
use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Form, Json, Router,
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
use crate::ui::{escape_html, render_navbar, APP_CSS, HEAD_META, ICON_SPARKLE};

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

pub fn to_standard_lowercase_json_schema(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                if k == "type" {
                    if let Some(s) = v.as_str() {
                        new_map.insert(k.clone(), serde_json::Value::String(s.to_lowercase()));
                    } else {
                        new_map.insert(k.clone(), to_standard_lowercase_json_schema(v));
                    }
                } else {
                    new_map.insert(k.clone(), to_standard_lowercase_json_schema(v));
                }
            }
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(to_standard_lowercase_json_schema).collect())
        }
        _ => val.clone(),
    }
}

pub fn draft_cards_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "OBJECT",
        "properties": {
            "cards": {
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
            }
        },
        "required": ["cards"]
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

pub fn extract_json_slice(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find("```json") {
        let rest = &trimmed[start + 7..];
        if let Some(end) = rest.find("```") {
            return rest[..end].trim();
        }
    }
    if let Some(start) = trimmed.find("```") {
        let rest = &trimmed[start + 3..];
        let rest = if let Some(nl) = rest.find('\n') { &rest[nl + 1..] } else { rest };
        if let Some(end) = rest.find("```") {
            return rest[..end].trim();
        }
    }

    let start_obj = trimmed.find('{');
    let start_arr = trimmed.find('[');
    let start = match (start_obj, start_arr) {
        (Some(o), Some(a)) => Some(o.min(a)),
        (Some(o), None) => Some(o),
        (None, Some(a)) => Some(a),
        (None, None) => None,
    };

    if let Some(s) = start {
        return trimmed[s..].trim();
    }

    trimmed
}

pub fn parse_draft_cards_from_json_str(raw: &str) -> Result<Vec<DraftCard>> {
    let json_text = extract_json_slice(raw);
    if json_text.trim().is_empty() {
        anyhow::bail!("AI 응답에서 유효한 JSON을 찾을 수 없습니다. (원문: {})", raw.chars().take(200).collect::<String>());
    }

    // Deserializer::into_iter()를 사용하여 trailing characters(뒤에 붙은 텍스트/안내문구)가 있어도
    // 첫 번째 온전한 JSON 엔티티만 안전하게 파싱합니다.
    let de = serde_json::Deserializer::from_str(json_text);
    let val: serde_json::Value = match de.into_iter::<serde_json::Value>().next() {
        Some(Ok(v)) => v,
        Some(Err(e)) => {
            anyhow::bail!("AI 응답 JSON 파싱 실패 ({e}): {}", json_text.chars().take(200).collect::<String>());
        }
        None => {
            anyhow::bail!("AI 응답이 비어 있습니다.");
        }
    };

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
        anyhow::bail!("Unexpected JSON structure for DraftCards: {json_text}")
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

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&access_token)
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::FORBIDDEN {
                anyhow::bail!(
                    "Google Cloud Code Assist 권한 에러 (403 Forbidden): 개인 계정은 Cloud Code Assist 무료 라이선스가 종료되었습니다. 상단 [설정] 메뉴에서 무료 'Gemini API 키(Google AI Studio)'를 등록하여 이용해 주세요. (상세: {err_text})"
                );
            }
            anyhow::bail!("Cloud Code API 에러 ({status}): {err_text}");
        }

        let val: serde_json::Value = resp.json().await?;
        extract_text_from_cloud_code_response(&val)
    }
}

#[derive(Debug, Clone)]
pub struct GeminiApiKeyProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl GeminiApiKeyProvider {
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "gemini-2.5-flash".to_string());
        Self {
            api_key: api_key.into(),
            model,
            client: reqwest::Client::new(),
        }
    }

    pub async fn call_generate_content(
        &self,
        prompt: &str,
        schema: serde_json::Value,
    ) -> Result<String> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let payload = serde_json::json!({
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
        });

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gemini API 에러 ({status}): {err_text}");
        }

        let val: serde_json::Value = resp.json().await?;
        extract_text_from_cloud_code_response(&val)
    }
}

#[async_trait]
impl AiProvider for GeminiApiKeyProvider {
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
    model: Option<String>,
}

impl Default for AgyCliProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AgyCliProvider {
    pub fn new() -> Self {
        let binary = std::env::var("AGY_CLI_BIN").unwrap_or_else(|_| "agy".to_string());
        let model = std::env::var("AGY_CLI_MODEL")
            .ok()
            .or_else(|| Some("gemini-3.8-flash-low".to_string()));
        Self { binary, model }
    }

    pub fn with_binary(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            model: Some("gemini-3.8-flash-low".to_string()),
        }
    }

    pub fn with_binary_and_model(binary: impl Into<String>, model: Option<String>) -> Self {
        Self {
            binary: binary.into(),
            model,
        }
    }

    async fn invoke_cli(&self, prompt: &str, _schema: &serde_json::Value) -> Result<String> {
        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.arg("-p").arg(prompt);
        cmd.arg("--disable-slash-commands");

        if let Some(ref m) = self.model {
            let trimmed = m.trim();
            if !trimmed.is_empty() {
                cmd.arg("--model").arg(trimmed);
            }
        }

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("agy CLI 실행 실패 (exit code: {}): {}", output.status, stderr);
        }

        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        if raw.trim().is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("agy CLI가 빈 결과를 반환했습니다. (stderr: {stderr})");
        }
        Ok(raw)
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
            "Create a brand-new Korean situational sentence that naturally requires the English chunk '{chunk}' (meaning: {korean_meaning}, original example: '{context_sentence}'). Return ONLY valid JSON with keys korean_prompt and hint."
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
        let cleaned = extract_json_slice(&raw_stdout);
        let val: OutputTransferPrompt = serde_json::Deserializer::from_str(cleaned)
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Empty JSON for OutputTransferPrompt"))??;
        Ok(val)
    }

    async fn evaluate_output_transfer(
        &self,
        chunk: &str,
        korean_prompt: &str,
        user_sentence: &str,
    ) -> Result<OutputTransferEvaluation> {
        let prompt = format!(
            "Evaluate whether the user's English sentence naturally and accurately translates the Korean prompt '{korean_prompt}' using the target chunk '{chunk}'. User sentence: '{user_sentence}'. Return ONLY valid JSON with keys passed (boolean), corrected_sentence, and feedback."
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
        let cleaned = extract_json_slice(&raw_stdout);
        let val: OutputTransferEvaluation = serde_json::Deserializer::from_str(cleaned)
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Empty JSON for OutputTransferEvaluation"))??;
        Ok(val)
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
            .unwrap_or_else(|| "gemini_api".to_string());

        let api_key = get_setting(&self.pool, "gemini_api_key")
            .await?
            .or_else(|| std::env::var("GEMINI_API_KEY").ok())
            .unwrap_or_default();

        if configured == "agy_cli" {
            let custom_bin = get_setting(&self.pool, "agy_cli_bin").await?.filter(|s| !s.is_empty());
            let selected_model = get_setting(&self.pool, "ai_model")
                .await?
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "gemini-3.8-flash-low".to_string());

            let bin = custom_bin.unwrap_or_else(|| "agy".to_string());
            let cli = AgyCliProvider::with_binary_and_model(bin, Some(selected_model));
            Ok(Box::new(cli))
        } else if configured == "gemini_api" || (!api_key.trim().is_empty() && configured != "oauth") {
            if api_key.trim().is_empty() {
                anyhow::bail!(
                    "Gemini API 키가 등록되지 않았습니다. 상단 [설정] 메뉴에서 무료 'Google AI Studio Gemini API 키'를 등록해 주세요."
                );
            }
            let gemini_model = get_setting(&self.pool, "gemini_model").await?;
            let selected_model = match gemini_model {
                Some(m) if !m.trim().is_empty() => m,
                _ => get_setting(&self.pool, "ai_model")
                    .await?
                    .unwrap_or_else(|| "gemini-2.5-flash".to_string()),
            };
            Ok(Box::new(GeminiApiKeyProvider::new(api_key.trim(), Some(selected_model))))
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
        .route("/settings/gemini_key", post(gemini_key_handler))
        .route("/settings/oauth/code", post(oauth_code_exchange_handler))
        .route("/settings/provider", post(provider_switch_handler))
        .route("/settings/passcode", post(passcode_change_handler))
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
    #[serde(default)]
    ai_model: Option<String>,
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
        "gemini_api" | "gemini" | "api_key" => "gemini_api",
        "oauth" => "oauth",
        "agy_cli" | "agy" | "cli" => "agy_cli",
        _ => return (StatusCode::BAD_REQUEST, "provider must be gemini_api, oauth, or agy_cli").into_response(),
    };

    if let Err(err) = set_setting(&state.pool, "ai_provider", provider).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save provider: {err}"),
        )
            .into_response();
    }

    let saved_model = if let Some(m) = payload.ai_model.as_deref() {
        let trimmed = m.trim();
        if !trimmed.is_empty() {
            let _ = set_setting(&state.pool, "ai_model", trimmed).await;
            trimmed.to_string()
        } else {
            "gemini-3.8-flash-low".to_string()
        }
    } else {
        get_setting(&state.pool, "ai_model")
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "gemini-3.8-flash-low".to_string())
    };

    if wants_json(&headers) {
        Json(serde_json::json!({
            "ai_provider": provider,
            "ai_model": saved_model,
        })).into_response()
    } else {
        (StatusCode::SEE_OTHER, [(header::LOCATION, "/settings?provider_saved=1")]).into_response()
    }
}

#[derive(Deserialize)]
struct GeminiKeyPayload {
    gemini_api_key: String,
    #[serde(default)]
    gemini_model: Option<String>,
}

async fn gemini_key_handler(
    State(state): State<AppState>,
    Form(form): Form<GeminiKeyPayload>,
) -> Response {
    let key_trimmed = form.gemini_api_key.trim();
    if !key_trimmed.is_empty() {
        let _ = set_setting(&state.pool, "gemini_api_key", key_trimmed).await;
        let _ = set_setting(&state.pool, "ai_provider", "gemini_api").await;
    }
    if let Some(model) = form.gemini_model.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let _ = set_setting(&state.pool, "gemini_model", model).await;
        let _ = set_setting(&state.pool, "ai_model", model).await;
    }
    (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, "/settings?gemini_saved=1")],
    )
        .into_response()
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
        let card_num = idx + 1;
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
            r#"<article class="study-card draft-card" data-draft-index="{idx}" id="draft-card-{idx}">
  <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom: 0.75rem;">
    <span class="badge" style="font-size:0.8rem; background:#334155; color:#94a3b8; font-weight:600; padding:2px 8px; border-radius:4px;">후보 #{card_num}</span>
    <span class="save-status-badge" style="display:none; font-size:0.85rem; font-weight:700; color:#10b981;">✅ 저장 완료</span>
  </div>
  <form method="POST" action="/cards" onsubmit="handleDraftCardSubmit(event, this)">
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
    <button type="submit" class="btn btn-primary submit-btn" style="width:100%; margin-top:0.5rem;">Study Card로 저장 (Save as Study Card)</button>
  </form>
</article>"#,
            idx = idx,
            card_num = card_num,
            chunk = escape_html(&draft.chunk),
            surface_chunk = escape_html(&draft.surface_chunk),
            context_sentence = escape_html(&draft.context_sentence),
            korean_meaning = escape_html(&draft.korean_meaning),
            extra_examples = escape_html(&extra_examples_text),
            nuance_note = escape_html(&draft.nuance_note),
            tags = escape_html(&tags_text),
        ));
    }

    let navbar = render_navbar("home", None);

    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
  {HEAD_META}
  <title>Draft Cards — My English</title>
  <style>{APP_CSS}</style>
</head>
<body>
  <div id="toastNotification" style="position: fixed; top: 1.5rem; right: 1.5rem; z-index: 9999; padding: 0.85rem 1.25rem; background: #0f172a; color: #fff; border: 1px solid #10b981; border-radius: 8px; box-shadow: 0 8px 24px rgba(0,0,0,0.4); display: none; font-weight: 600; align-items: center; gap: 0.5rem; transition: opacity 0.2s ease;"></div>
  {navbar}

  <div class="container">
    <section class="panel" id="smart-input-section" style="border-color: rgba(56, 189, 248, 0.25);">
      <h2 style="display:flex; align-items:center; gap:0.5rem; font-size:1.15rem; margin-top:0;">{ICON_SPARKLE} <span>Smart Input (AI 통합 입력창)</span></h2>
      <form method="POST" action="/smart-input">
        <textarea name="input" rows="2">{user_input}</textarea>
        <button type="submit" class="btn btn-primary">{ICON_SPARKLE} <span>AI로 Draft Card 다시 생성</span></button>
      </form>
    </section>

    <section class="panel" id="draft-cards-section">
      <div style="display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 0.75rem; margin-bottom: 0.5rem;">
        <h2 style="margin: 0;">생성된 Draft Card 후보 ({count}개)</h2>
        <div style="display: flex; gap: 0.5rem; flex-wrap: wrap;">
          <button type="button" id="saveAllDraftsBtn" class="btn btn-primary" onclick="saveAllDrafts()" style="background: #2563eb;">⚡ 모든 Draft Card 일괄 저장</button>
          <a href="/" class="btn" style="text-decoration: none; display: inline-flex; align-items: center;">홈 (Home)으로 이동</a>
        </div>
      </div>
      <p style="color: var(--text-muted); font-size: 0.9rem;">필요하면 필드를 바로 인라인 수정한 뒤 원클릭으로 Study Card에 저장하세요. 저장 후에도 페이지를 벗어나지 않고 다른 후보들을 계속 추가할 수 있습니다.</p>
      <div id="saveAllStatusBanner" style="display:none; padding: 0.85rem 1.15rem; border-radius: 8px; margin-bottom: 1.25rem; background: rgba(16, 185, 129, 0.15); border: 1px solid #10b981; color: #10b981; font-weight: 600;"></div>
      <div class="card-grid">
        {drafts_html}
      </div>
    </section>
  </div>

  <script>
    function showToast(msg) {{
      const toast = document.getElementById('toastNotification');
      if (!toast) return;
      toast.innerText = msg;
      toast.style.display = 'flex';
      toast.style.opacity = '1';
      clearTimeout(window._toastTimer);
      window._toastTimer = setTimeout(() => {{
        toast.style.opacity = '0';
        setTimeout(() => {{ toast.style.display = 'none'; }}, 200);
      }}, 2500);
    }}

    async function handleDraftCardSubmit(e, form) {{
      if (e && e.preventDefault) e.preventDefault();
      const card = form.closest('.draft-card');
      const btn = form.querySelector('.submit-btn');
      const statusBadge = card ? card.querySelector('.save-status-badge') : null;
      const chunkInput = form.querySelector('input[name="chunk"]');
      const chunkVal = chunkInput ? chunkInput.value : '';

      if (btn && btn.disabled) return;
      const originalText = btn ? btn.innerHTML : '';
      if (btn) {{
        btn.disabled = true;
        btn.innerText = '저장 중...';
      }}

      try {{
        const formData = new FormData(form);
        const body = new URLSearchParams(formData);

        const res = await fetch(form.action, {{
          method: 'POST',
          headers: {{
            'Accept': 'application/json',
            'Content-Type': 'application/x-www-form-urlencoded'
          }},
          body: body.toString()
        }});

        if (res.ok) {{
          if (btn) {{
            btn.innerText = '✅ 저장 완료 (Saved)';
            btn.style.background = '#10b981';
            btn.style.borderColor = '#059669';
            btn.style.color = '#ffffff';
          }}
          if (statusBadge) statusBadge.style.display = 'inline';
          if (card) {{
            card.style.border = '2px solid #10b981';
            card.style.background = 'rgba(16, 185, 129, 0.05)';
            card.setAttribute('data-saved', 'true');
          }}
          showToast(`'${{chunkVal}}' Study Card가 저장되었습니다! 🎉`);
          checkAllSaved();
        }} else {{
          const err = await res.text();
          alert('저장 실패: ' + err);
          if (btn) {{
            btn.disabled = false;
            btn.innerHTML = originalText;
          }}
        }}
      }} catch (err) {{
        alert('네트워크 오류: ' + err.message);
        if (btn) {{
          btn.disabled = false;
          btn.innerHTML = originalText;
        }}
      }}
    }}

    async function saveAllDrafts() {{
      const forms = Array.from(document.querySelectorAll('.draft-card:not([data-saved="true"]) form'));
      if (forms.length === 0) {{
        showToast('이미 모든 Draft Card가 저장되었습니다! 😊');
        return;
      }}

      const saveAllBtn = document.getElementById('saveAllDraftsBtn');
      if (saveAllBtn) {{
        saveAllBtn.disabled = true;
        saveAllBtn.innerText = '모두 저장 중...';
      }}

      for (const form of forms) {{
        const card = form.closest('.draft-card');
        if (card && card.getAttribute('data-saved') === 'true') continue;
        await handleDraftCardSubmit(null, form);
      }}

      if (saveAllBtn) {{
        saveAllBtn.innerText = '✅ 모든 카드 저장 완료';
        saveAllBtn.style.background = '#10b981';
      }}
    }}

    function checkAllSaved() {{
      const unsaved = document.querySelectorAll('.draft-card:not([data-saved="true"])');
      if (unsaved.length === 0) {{
        const banner = document.getElementById('saveAllStatusBanner');
        if (banner) {{
          banner.innerHTML = '🎉 모든 Draft Card가 Study Card에 성공적으로 저장되었습니다! <a href="/" style="color:#10b981; text-decoration:underline; margin-left: 0.5rem; font-weight:700;">홈 (Home)으로 이동하기 →</a>';
          banner.style.display = 'block';
        }}
        const saveAllBtn = document.getElementById('saveAllDraftsBtn');
        if (saveAllBtn) {{
          saveAllBtn.innerText = '✅ 모든 카드 저장 완료';
          saveAllBtn.style.background = '#10b981';
          saveAllBtn.disabled = true;
        }}
      }}
    }}
  </script>
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
        .unwrap_or_else(|| "agy_cli".to_string());
    let current_model = get_setting(&state.pool, "ai_model")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "gemini-3.8-flash-low".to_string());
    let project_id = get_setting(&state.pool, "cloud_code_project_id")
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let gemini_key = get_setting(&state.pool, "gemini_api_key")
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let masked_gemini_key = if gemini_key.len() > 8 {
        format!("{}...{}", &gemini_key[..6], &gemini_key[gemini_key.len() - 4..])
    } else if !gemini_key.is_empty() {
        "********".to_string()
    } else {
        "".to_string()
    };
    let gemini_model = get_setting(&state.pool, "gemini_model")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "gemini-2.5-flash".to_string());
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
        &current_model,
        &masked_gemini_key,
        &gemini_model,
    ))
    .into_response()
}

#[derive(Deserialize)]
struct PasscodeChangePayload {
    new_passcode: String,
}

async fn passcode_change_handler(
    State(state): State<AppState>,
    Form(form): Form<PasscodeChangePayload>,
) -> Response {
    let trimmed = form.new_passcode.trim();
    if !trimmed.is_empty() {
        let _ = set_setting(&state.pool, "custom_passcode", trimmed).await;
    }
    (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, "/settings?passcode_saved=1")],
    )
        .into_response()
}

pub fn render_settings_page(
    oauth_url: &str,
    current_provider: &str,
    has_token: bool,
    project_id: &str,
    current_model: &str,
    masked_gemini_key: &str,
    gemini_model: &str,
) -> String {
    let gemini_checked = if current_provider == "gemini_api" || (current_provider != "agy_cli" && current_provider != "oauth") {
        "checked"
    } else {
        ""
    };
    let oauth_checked = if current_provider == "oauth" {
        "checked"
    } else {
        ""
    };
    let cli_checked = if current_provider == "agy_cli" {
        "checked"
    } else {
        ""
    };

    let flash_low_sel = if current_model == "gemini-3.8-flash-low" { "selected" } else { "" };
    let flash_med_sel = if current_model == "gemini-3.8-flash-medium" { "selected" } else { "" };
    let argon_med_sel = if current_model == "gemini-4-argon-medium" { "selected" } else { "" };
    let claude_sel = if current_model.contains("claude") { "selected" } else { "" };

    let g25_flash_sel = if gemini_model == "gemini-2.5-flash" || gemini_model.is_empty() { "selected" } else { "" };
    let g25_pro_sel = if gemini_model == "gemini-2.5-pro" { "selected" } else { "" };
    let g15_flash_sel = if gemini_model == "gemini-1.5-flash" { "selected" } else { "" };

    let gemini_status_badge = if !masked_gemini_key.is_empty() {
        format!(r#"<span style="display:inline-block;padding:2px 8px;border-radius:999px;font-size:0.8rem;background:#064e3b;color:#34d399;font-weight:700;">🟢 등록됨 ({})</span>"#, escape_html(masked_gemini_key))
    } else {
        r#"<span style="display:inline-block;padding:2px 8px;border-radius:999px;font-size:0.8rem;background:#334155;color:#94a3b8;font-weight:700;">⚪ 미등록</span>"#.to_string()
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

    let navbar = render_navbar("settings", None);

    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
  {HEAD_META}
  <title>설정 — My English</title>
  <style>{APP_CSS}</style>
</head>
<body>
  {navbar}
  <div class="container">
    <section class="panel">
      <h2>🎨 비주얼 테마 설정 (Visual Themes)</h2>
      <p style="color:var(--text-muted);font-size:0.9rem;margin-top:0.25rem;margin-bottom:1.15rem;">
        취향과 학습 환경에 맞춰 5가지 고유 테마를 선택할 수 있습니다. 상단 네비게이션 바의 테마 선택기에서도 언제든 즉시 변경됩니다.
      </p>
      <div class="theme-cards-grid">
        <button type="button" class="theme-card-option" data-theme-val="obsidian" onclick="window.switchTheme('obsidian')">
          <div class="theme-card-preview" style="background:#090d16;border:1px solid #1e293b;">
            <div class="theme-card-dot" style="background:#2563eb;"></div>
            <div class="theme-card-dot" style="background:#38bdf8;"></div>
            <div class="theme-card-dot" style="background:#10b981;"></div>
          </div>
          <div style="font-weight:700;font-size:0.95rem;color:var(--text);">🌌 옵시디언 블루 (Obsidian)</div>
          <div style="font-size:0.8rem;color:var(--text-muted);line-height:1.4;">세련된 딥 네이비 사이버 다크와 일렉트릭 블루 액센트</div>
        </button>

        <button type="button" class="theme-card-option" data-theme-val="nordic" onclick="window.switchTheme('nordic')">
          <div class="theme-card-preview" style="background:#09130e;border:1px solid #1e3b2c;">
            <div class="theme-card-dot" style="background:#059669;"></div>
            <div class="theme-card-dot" style="background:#34d399;"></div>
            <div class="theme-card-dot" style="background:#eab308;"></div>
          </div>
          <div style="font-weight:700;font-size:0.95rem;color:var(--text);">🌲 포레스트 노르딕 (Nordic)</div>
          <div style="font-size:0.8rem;color:var(--text-muted);line-height:1.4;">눈이 편안한 깊은 침엽수림 에버그린과 세이지 그린</div>
        </button>

        <button type="button" class="theme-card-option" data-theme-val="paper" onclick="window.switchTheme('paper')">
          <div class="theme-card-preview" style="background:#f8f6f0;border:1px solid #dfdbce;">
            <div class="theme-card-dot" style="background:#2563eb;"></div>
            <div class="theme-card-dot" style="background:#d97706;"></div>
            <div class="theme-card-dot" style="background:#1a1714;"></div>
          </div>
          <div style="font-weight:700;font-size:0.95rem;color:var(--text);">📜 페이퍼 미니멀 (Paper Light)</div>
          <div style="font-size:0.8rem;color:var(--text-muted);line-height:1.4;">따뜻한 양장본 서적 느낌의 고대비 종이 질감 라이트 모드</div>
        </button>

        <button type="button" class="theme-card-option" data-theme-val="sunset" onclick="window.switchTheme('sunset')">
          <div class="theme-card-preview" style="background:#13100f;border:1px solid #382d26;">
            <div class="theme-card-dot" style="background:#d97706;"></div>
            <div class="theme-card-dot" style="background:#fbbf24;"></div>
            <div class="theme-card-dot" style="background:#10b981;"></div>
          </div>
          <div style="font-weight:700;font-size:0.95rem;color:var(--text);">☕ 선셋 앰버 (Sunset & Coffee)</div>
          <div style="font-size:0.8rem;color:var(--text-muted);line-height:1.4;">아늑한 에스프레소 카페와 저녁 촛불 노을빛 감성</div>
        </button>

        <button type="button" class="theme-card-option" data-theme-val="terminal" onclick="window.switchTheme('terminal')">
          <div class="theme-card-preview" style="background:#050805;border:1px solid #16301f;">
            <div class="theme-card-dot" style="background:#22c55e;"></div>
            <div class="theme-card-dot" style="background:#4ade80;"></div>
            <div class="theme-card-dot" style="background:#06b6d4;"></div>
          </div>
          <div style="font-weight:700;font-size:0.95rem;color:var(--text);">📟 사이버 터미널 (Retro Terminal)</div>
          <div style="font-size:0.8rem;color:var(--text-muted);line-height:1.4;">피치 블랙 배경과 발광하는 형광 그린 매트릭스 콘솔</div>
        </button>
      </div>
    </section>

    <section class="panel">
      <h2>🔑 로그인 패스코드(비밀번호) 변경</h2>
      <p style="color:var(--text-muted);font-size:0.9rem;margin-top:0.25rem;margin-bottom:1rem;">
        앱 접속 시 입력하는 비밀번호를 변경할 수 있습니다. 저장 즉시 다음 로그인부터 적용됩니다.
      </p>
      <form method="POST" action="/settings/passcode" style="max-width:360px;">
        <label for="new_passcode">새 비밀번호 (New Passcode)</label>
        <input type="password" id="new_passcode" name="new_passcode" required placeholder="새 비밀번호 입력" autocomplete="new-password">
        <button type="submit" class="btn btn-primary">비밀번호 변경 및 저장</button>
      </form>
    </section>

    <section class="panel" style="border-left:4px solid var(--primary);">
      <h2>⚡ Gemini API 설정 (권장 — 무료 & 가장 안정적)</h2>
      <p style="color:var(--text-muted);font-size:0.9rem;margin-top:0.25rem;margin-bottom:1rem;line-height:1.5;">
        Google AI Studio에서 무료로 발급받은 Gemini API 키를 입력하시면 클라우드 서버에서도 오류 없이 즉시 스마트 인풋을 이용하실 수 있습니다.
        <br>
        👉 <a href="https://aistudio.google.com/app/apikey" target="_blank" rel="noopener noreferrer" style="color:var(--primary);font-weight:700;text-decoration:underline;">Google AI Studio에서 무료 API 키 발급받기 (클릭)</a>
      </p>
      <form method="POST" action="/settings/gemini_key" style="max-width:540px;">
        <div style="margin-bottom:0.75rem;">
          <label for="gemini_api_key" style="font-weight:600;">Gemini API Key</label>
          <div style="font-size:0.85rem;color:var(--text-muted);margin-bottom:0.25rem;">
            등록 상태: {gemini_status_badge}
          </div>
          <input type="password" id="gemini_api_key" name="gemini_api_key" placeholder="AIzaSy... (새 키 입력 시 기존 키를 덮어씁니다)" autocomplete="off">
        </div>

        <div style="margin-bottom:1rem;">
          <label for="gemini_model" style="font-weight:600;">사용할 Gemini 모델</label>
          <select id="gemini_model" name="gemini_model" style="margin-top:0.25rem;">
            <option value="gemini-2.5-flash" {g25_flash_sel}>⚡ Gemini 2.5 Flash (권장 — 초고속 / 고품질 한국어 맥락 청크 분석)</option>
            <option value="gemini-2.5-pro" {g25_pro_sel}>🧠 Gemini 2.5 Pro (최상위 분석 성능)</option>
            <option value="gemini-1.5-flash" {g15_flash_sel}>⚡ Gemini 1.5 Flash (경량 고속 모델)</option>
          </select>
        </div>

        <button type="submit" class="btn btn-primary">Gemini API 키 저장 및 활성화</button>
      </form>
    </section>

    <section class="panel">
      <h1>AI 엔진 및 모델 설정 (AI Engine & Model)</h1>
      <form method="POST" action="/settings/provider">
        <label style="display:flex;align-items:center;gap:0.5rem;margin-bottom:0.5rem;cursor:pointer;">
          <input type="radio" name="provider" value="gemini_api" style="width:auto;" {gemini_checked}>
          <strong>⚡ Google AI Studio Gemini API 키 (권장 — 개인 계정 무료 / 가장 안정적)</strong>
        </label>
        <label style="display:flex;align-items:center;gap:0.5rem;margin-bottom:0.5rem;cursor:pointer;">
          <input type="radio" name="provider" value="agy_cli" style="width:auto;" {cli_checked}>
          <span>💻 로컬 Antigravity CLI</span>
        </label>
        <label style="display:flex;align-items:center;gap:0.5rem;margin-bottom:0.5rem;cursor:pointer;">
          <input type="radio" name="provider" value="oauth" style="width:auto;" {oauth_checked}>
          <span>🏢 Google OAuth PKCE (Cloud Code Assist API — 기업/조직 라이선스 계정 전용)</span>
        </label>

        <label for="ai_model" style="display:block;margin-top:1.25rem;font-weight:600;">Antigravity CLI 실행 모델:</label>
        <select id="ai_model" name="ai_model" style="margin-top:0.25rem;">
          <option value="gemini-3.8-flash-low" {flash_low_sel}>⚡ Gemini 3.8 Flash (Low) — 초고속 / 권장 (2~3초)</option>
          <option value="gemini-3.8-flash-medium" {flash_med_sel}>⚡ Gemini 3.8 Flash (Medium) — 균형</option>
          <option value="gemini-4-argon-medium" {argon_med_sel}>🧠 Gemini 4 Argon (Medium) — 심층 분석</option>
          <option value="claude-sonnet-5-5-medium" {claude_sel}>🧠 Claude Sonnet 5.5 (Medium)</option>
        </select>

        <div style="margin-top:1rem;">
          <button type="submit" class="btn btn-primary">선택 엔진 저장 (Save Provider)</button>
        </div>
      </form>
    </section>

    <section class="panel">
      <h2>Google OAuth PKCE 브라우저 인증 (기업/조직용)</h2>
      <p style="color:var(--text-muted);font-size:0.85rem;">
        ※ 구글 정책 변경으로 개인 계정은 Cloud Code Assist 무료 라이선스가 종료(403 Forbidden 발생)되었으므로, 위의 무료 <strong>Gemini API 키</strong> 사용을 권장합니다.
      </p>
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
