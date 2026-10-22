use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use http_body_util::BodyExt;
use my_english::{build_app, db, state::AppState};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tower::ServiceExt;

async fn test_app_with_pool() -> (axum::Router, SqlitePool, String) {
    let pool = db::init_memory_db().await.expect("init memory db");
    let state = AppState::new(pool.clone(), "my-secret-passcode", "my-cookie-signing-key");
    let app = build_app(state);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("passcode=my-secret-passcode"))
                .unwrap(),
        )
        .await
        .unwrap();
    let set_cookie = res
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .expect("login cookie");
    let cookie = set_cookie.split(';').next().unwrap().trim().to_string();

    (app, pool, cookie)
}

#[tokio::test]
async fn settings_page_generates_pkce_s256_oauth_url_and_persists_verifier() {
    let (app, pool, cookie) = test_app_with_pool().await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);

    // Must contain the Google OAuth v2 endpoint and required parameters
    assert!(html.contains("https://accounts.google.com/o/oauth2/v2/auth"));
    assert!(html.contains("681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com"));
    assert!(html.contains("code_challenge_method=S256"));
    assert!(html.contains("access_type=offline"));
    assert!(html.contains("response_type=code"));
    assert!(
        html.contains("https%3A%2F%2Fcodeassist.google.com%2Fauthcode")
            || html.contains("https://codeassist.google.com/authcode")
    );
    assert!(html.contains("cloud-platform"));
    assert!(html.contains("userinfo.email"));
    assert!(html.contains("userinfo.profile"));

    // pkce_verifier must be stored in app_settings and match the code_challenge in the URL
    let verifier_row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_settings WHERE key = 'pkce_verifier'")
            .fetch_optional(&pool)
            .await
            .expect("query pkce_verifier");
    let verifier = verifier_row.expect("pkce_verifier must be saved in app_settings").0;
    assert!(verifier.len() >= 43, "PKCE verifier must be at least 43 chars");

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let expected_challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());
    assert!(
        html.contains(&format!("code_challenge={expected_challenge}")),
        "HTML must contain S256 code_challenge matching stored pkce_verifier"
    );
}

#[tokio::test]
async fn settings_oauth_code_exchange_and_provider_switch_persist_settings() {
    use axum::{routing::post, Json, Router};
    use std::sync::{Arc, Mutex};

    let captured_token_body = Arc::new(Mutex::new(String::new()));
    let captured_load_auth = Arc::new(Mutex::new(String::new()));

    let token_body_clone = captured_token_body.clone();
    let load_auth_clone = captured_load_auth.clone();

    let mock_google = Router::new()
        .route(
            "/token",
            post(move |body: String| {
                let token_body_clone = token_body_clone.clone();
                async move {
                    *token_body_clone.lock().unwrap() = body;
                    Json(serde_json::json!({
                        "access_token": "mock_access_token_123",
                        "refresh_token": "mock_refresh_token_456",
                        "expires_in": 3600,
                        "token_type": "Bearer"
                    }))
                }
            }),
        )
        .route(
            "/v1internal:loadCodeAssist",
            post(move |headers: axum::http::HeaderMap, _body: String| {
                let load_auth_clone = load_auth_clone.clone();
                async move {
                    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
                        *load_auth_clone.lock().unwrap() = auth.to_string();
                    }
                    Json(serde_json::json!({
                        "cloudaicompanionProject": "companion-proj-999",
                        "currentTier": { "id": "standard-tier" }
                    }))
                }
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, mock_google).await.unwrap();
    });

    let (app, pool, cookie) = test_app_with_pool().await;

    // Configure mock endpoints in app_settings for deterministic local test
    my_english::ai::set_setting(&pool, "oauth_token_url", &format!("http://{addr}/token"))
        .await
        .unwrap();
    my_english::ai::set_setting(&pool, "cloud_code_base_url", &format!("http://{addr}"))
        .await
        .unwrap();

    // 1. Visit GET /settings to generate pkce_verifier
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let verifier = my_english::ai::get_setting(&pool, "pkce_verifier")
        .await
        .unwrap()
        .expect("pkce_verifier");

    // 2. Submit authorization code via POST /settings/oauth/code
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/oauth/code")
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("code=4%2Fmock_auth_code_xyz"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);

    // Verify token endpoint received the PKCE verifier and auth code
    let token_req = captured_token_body.lock().unwrap().clone();
    assert!(token_req.contains("code=4%2Fmock_auth_code_xyz"));
    assert!(token_req.contains(&format!("code_verifier={verifier}")));
    assert!(token_req.contains("grant_type=authorization_code"));
    assert_eq!(
        *captured_load_auth.lock().unwrap(),
        "Bearer mock_access_token_123"
    );

    // Verify credentials and project ID stored in app_settings
    assert_eq!(
        my_english::ai::get_setting(&pool, "oauth_access_token")
            .await
            .unwrap()
            .as_deref(),
        Some("mock_access_token_123")
    );
    assert_eq!(
        my_english::ai::get_setting(&pool, "oauth_refresh_token")
            .await
            .unwrap()
            .as_deref(),
        Some("mock_refresh_token_456")
    );
    assert_eq!(
        my_english::ai::get_setting(&pool, "cloud_code_project_id")
            .await
            .unwrap()
            .as_deref(),
        Some("companion-proj-999")
    );
    assert_eq!(
        my_english::ai::get_setting(&pool, "ai_provider")
            .await
            .unwrap()
            .as_deref(),
        Some("oauth")
    );
    assert!(
        my_english::ai::get_setting(&pool, "oauth_expiry")
            .await
            .unwrap()
            .is_some()
    );

    // 3. Switch provider to agy_cli via POST /settings/provider
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/provider")
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("provider=agy_cli"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        my_english::ai::get_setting(&pool, "ai_provider")
            .await
            .unwrap()
            .as_deref(),
        Some("agy_cli")
    );
}

#[tokio::test]
async fn cloud_code_oauth_provider_refreshes_expired_token_and_calls_generate_content() {
    use axum::{routing::post, Json, Router};
    use my_english::ai::{AiProvider, CloudCodeOAuthProvider};
    use std::sync::{Arc, Mutex};

    let refresh_called = Arc::new(Mutex::new(false));
    let captured_gen_req = Arc::new(Mutex::new(serde_json::Value::Null));
    let captured_gen_auth = Arc::new(Mutex::new(String::new()));

    let refresh_called_clone = refresh_called.clone();
    let gen_req_clone = captured_gen_req.clone();
    let gen_auth_clone = captured_gen_auth.clone();

    let mock_server = Router::new()
        .route(
            "/token",
            post(move |body: String| {
                let refresh_called_clone = refresh_called_clone.clone();
                async move {
                    assert!(body.contains("grant_type=refresh_token"));
                    assert!(body.contains("refresh_token=stored_refresh_token_789"));
                    *refresh_called_clone.lock().unwrap() = true;
                    Json(serde_json::json!({
                        "access_token": "freshly_refreshed_access_token",
                        "expires_in": 3600,
                        "token_type": "Bearer"
                    }))
                }
            }),
        )
        .route(
            "/v1internal:generateContent",
            post(
                move |headers: axum::http::HeaderMap, Json(payload): Json<serde_json::Value>| {
                    let gen_req_clone = gen_req_clone.clone();
                    let gen_auth_clone = gen_auth_clone.clone();
                    async move {
                        if let Some(auth) =
                            headers.get("authorization").and_then(|v| v.to_str().ok())
                        {
                            *gen_auth_clone.lock().unwrap() = auth.to_string();
                        }
                        *gen_req_clone.lock().unwrap() = payload;

                        let inner_cards = serde_json::json!([
                            {
                                "chunk": "get me every time",
                                "surface_chunk": "gets me every time",
                                "context_sentence": "That commercial gets me every time.",
                                "korean_meaning": "볼 때마다 나를 웃기다/울리다",
                                "extra_examples": [
                                    {
                                        "english": "His reaction gets me every time.",
                                        "korean": "그의 반응은 볼 때마다 웃겨."
                                    }
                                ],
                                "nuance_note": "반복적으로 감정을 자극할 때 쓰는 원어민 표현",
                                "tags": ["emotion", "daily"]
                            }
                        ]);
                        Json(serde_json::json!({
                            "traceId": "trace-123",
                            "response": {
                                "candidates": [
                                    {
                                        "content": {
                                            "role": "model",
                                            "parts": [
                                                { "text": inner_cards.to_string() }
                                            ]
                                        }
                                    }
                                ]
                            }
                        }))
                    }
                },
            ),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, mock_server).await.unwrap();
    });

    let pool = db::init_memory_db().await.unwrap();
    my_english::ai::set_setting(&pool, "oauth_token_url", &format!("http://{addr}/token"))
        .await
        .unwrap();
    my_english::ai::set_setting(&pool, "cloud_code_base_url", &format!("http://{addr}"))
        .await
        .unwrap();
    my_english::ai::set_setting(&pool, "oauth_access_token", "expired_access_token")
        .await
        .unwrap();
    my_english::ai::set_setting(&pool, "oauth_refresh_token", "stored_refresh_token_789")
        .await
        .unwrap();
    // Past expiry timestamp
    my_english::ai::set_setting(&pool, "oauth_expiry", "2020-01-01T00:00:00Z")
        .await
        .unwrap();
    my_english::ai::set_setting(&pool, "cloud_code_project_id", "my-cloud-proj")
        .await
        .unwrap();

    let provider = CloudCodeOAuthProvider::new(pool.clone());
    let drafts = provider
        .generate_draft_cards("볼 때마다 웃겨를 영어로 뭐라고 해?")
        .await
        .expect("generate draft cards via CloudCodeOAuthProvider");

    assert!(*refresh_called.lock().unwrap(), "expected token refresh call");
    assert_eq!(
        *captured_gen_auth.lock().unwrap(),
        "Bearer freshly_refreshed_access_token"
    );

    let gen_req = captured_gen_req.lock().unwrap().clone();
    assert_eq!(gen_req["project"], "my-cloud-proj");
    assert!(gen_req["model"].as_str().unwrap_or("").contains("gemini"));
    assert!(gen_req["request"]["contents"].is_array());

    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].chunk, "get me every time");
    assert_eq!(drafts[0].surface_chunk, "gets me every time");
    assert_eq!(
        my_english::ai::get_setting(&pool, "oauth_access_token")
            .await
            .unwrap()
            .as_deref(),
        Some("freshly_refreshed_access_token")
    );
}

#[tokio::test]
async fn agy_cli_provider_invokes_cli_with_json_schema_and_parses_draft_cards() {
    use my_english::ai::{AgyCliProvider, AiProvider};
    use std::os::unix::fs::PermissionsExt;

    let tmp_dir = tempfile::tempdir().unwrap();
    let script_path = tmp_dir.path().join("mock_agy.sh");
    let args_log_path = tmp_dir.path().join("args.log");

    let script = format!(
        r#"#!/bin/sh
echo "$@" > "{}"
cat << 'EOF'
[
  {{
    "chunk": "play it by ear",
    "surface_chunk": "play it by ear",
    "context_sentence": "Let's just play it by ear tonight.",
    "korean_meaning": "상황 봐서 즉흥적으로 하다",
    "extra_examples": [],
    "nuance_note": "계획 없이 상황에 맞춰 행동할 때 사용",
    "tags": ["casual", "plan"]
  }}
]
EOF
"#,
        args_log_path.display()
    );
    std::fs::write(&script_path, script).unwrap();
    let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script_path, perms).unwrap();

    let provider = AgyCliProvider::with_binary(script_path.to_string_lossy().to_string());
    let drafts = provider
        .generate_draft_cards("즉흥적으로 하자 영어로?")
        .await
        .expect("generate draft cards via AgyCliProvider");

    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].chunk, "play it by ear");

    let logged_args = std::fs::read_to_string(&args_log_path).unwrap();
    assert!(logged_args.contains("-p"));
    assert!(logged_args.contains("--disable-slash-commands"));
}

#[derive(Clone)]
struct FakeAiProvider {
    drafts: Vec<my_english::domain::DraftCard>,
}

#[async_trait::async_trait]
impl my_english::ai::AiProvider for FakeAiProvider {
    async fn generate_draft_cards(
        &self,
        _input: &str,
    ) -> anyhow::Result<Vec<my_english::domain::DraftCard>> {
        Ok(self.drafts.clone())
    }

    async fn generate_output_transfer_prompt(
        &self,
        _chunk: &str,
        _context_sentence: &str,
        _korean_meaning: &str,
    ) -> anyhow::Result<my_english::domain::OutputTransferPrompt> {
        anyhow::bail!("not used in this test")
    }

    async fn evaluate_output_transfer(
        &self,
        _chunk: &str,
        _korean_prompt: &str,
        _user_sentence: &str,
    ) -> anyhow::Result<my_english::domain::OutputTransferEvaluation> {
        anyhow::bail!("not used in this test")
    }
}

#[tokio::test]
async fn smart_input_generates_draft_cards_renders_inline_edit_and_saves_with_smart_input_source() {
    use my_english::domain::{DraftCard, ExtraExample};
    use std::sync::Arc;

    let fake_ai = FakeAiProvider {
        drafts: vec![
            DraftCard {
                chunk: "give someone the benefit of the doubt".to_string(),
                surface_chunk: "gave him the benefit of the doubt".to_string(),
                context_sentence: "I wasn't sure if his excuse was true, but I gave him the benefit of the doubt.".to_string(),
                korean_meaning: "(증거가 없어도) 일단 상대방의 말을 믿어주다, 선의로 해석하다".to_string(),
                extra_examples: vec![ExtraExample {
                    english: "Let's give her the benefit of the doubt this time.".to_string(),
                    korean: "이번에는 일단 그녀를 믿어주자.".to_string(),
                }],
                nuance_note: "상대방이 거짓말을 할 수도 있지만 일단 좋은 쪽으로 믿어줄 때 쓰는 필수 청크".to_string(),
                tags: vec!["conversation".to_string(), "trust".to_string()],
            },
            DraftCard {
                chunk: "cut someone some slack".to_string(),
                surface_chunk: "cut me some slack".to_string(),
                context_sentence: "Come on, cut me some slack—it's my first day on the job.".to_string(),
                korean_meaning: "사정을 봐주다, 너무 엄격하게 굴지 않다".to_string(),
                extra_examples: vec![],
                nuance_note: "실수나 부족함에 대해 너그럽게 봐달라고 할 때 자주 쓰임".to_string(),
                tags: vec!["work".to_string(), "daily".to_string()],
            },
        ],
    };

    let pool = db::init_memory_db().await.unwrap();
    let state = AppState::new(pool.clone(), "my-secret-passcode", "my-cookie-signing-key")
        .with_ai_provider(Arc::new(fake_ai));
    let app = build_app(state);

    let login_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("passcode=my-secret-passcode"))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = login_res
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();

    // 1. JSON API request to POST /api/smart-input
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/smart-input")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "input": "일단 상대방 말을 믿어주다를 영어로 어떻게 말해?"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let returned_drafts: Vec<DraftCard> = serde_json::from_slice(&json_bytes).unwrap();
    assert_eq!(returned_drafts.len(), 2);
    assert_eq!(
        returned_drafts[0].chunk,
        "give someone the benefit of the doubt"
    );
    assert_eq!(
        returned_drafts[0].surface_chunk,
        "gave him the benefit of the doubt"
    );
    assert_eq!(returned_drafts[0].extra_examples.len(), 1);
    assert_eq!(returned_drafts[0].tags, vec!["conversation", "trust"]);

    // 2. HTML form submission to POST /smart-input renders Draft Cards with inline-editable fields
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/smart-input")
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "input=I+trusted+his+excuse+even+without+proof.",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let html_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&html_bytes);

    assert!(html.contains("give someone the benefit of the doubt"));
    assert!(html.contains("gave him the benefit of the doubt"));
    assert!(html.contains("cut someone some slack"));
    assert!(html.contains(r#"name="source_type" value="smart_input""#));
    assert!(html.contains("Save as Study Card"));

    // 3. One-click (or inline-edited) Save as Study Card submits to POST /cards with source_type = 'smart_input'
    let save_form = "chunk=give+someone+the+benefit+of+the+doubt&surface_chunk=gave+him+the+benefit+of+the+doubt&context_sentence=I+wasn%27t+sure+if+his+excuse+was+true%2C+but+I+gave+him+the+benefit+of+the+doubt.&korean_meaning=%EC%9D%BC%EB%8B%A8+%EC%83%81%EB%8C%80%EB%B0%A9%EC%9D%98+%EB%A7%90%EC%9D%84+%EB%AF%BF%EC%96%B4%EC%A3%BC%EB%8B%A4&extra_examples=Let%27s+give+her+the+benefit+of+the+doubt+this+time.+%7C+%EC%9D%B4%EB%B2%88%EC%97%90%EB%8A%94+%EC%9D%BC%EB%8B%A8+%EA%B7%B8%EB%85%80%EB%A5%BC+%EB%AF%BF%EC%96%B4%EC%A3%BC%EC%9E%90.&nuance_note=%EC%9D%BC%EB%8B%A8+%EC%A2%8B%EC%9D%80+%EC%AA%BD%EC%9C%BC%EB%A1%9C+%EB%AF%BF%EC%96%B4%EC%A4%84+%EB%95%8C&tags=conversation%2C+trust&source_type=smart_input";
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/cards")
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(save_form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);

    // Verify persisted card in study_cards has source_type == "smart_input"
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/cards")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cards_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let cards: Vec<my_english::domain::StudyCard> = serde_json::from_slice(&cards_bytes).unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].chunk, "give someone the benefit of the doubt");
    assert_eq!(cards[0].surface_chunk, "gave him the benefit of the doubt");
    assert_eq!(cards[0].source_type, "smart_input");
}

#[tokio::test]
async fn gemini_api_key_configuration_and_mock_generation() {
    let (app, pool, cookie) = test_app_with_pool().await;

    // 1. Submit Gemini API Key via /settings/gemini_key
    let form = "gemini_api_key=AIzaSyFakeKey123456&gemini_model=gemini-2.5-flash";
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/gemini_key")
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);

    // Verify persisted in DB
    let saved_key = my_english::ai::get_setting(&pool, "gemini_api_key")
        .await
        .unwrap()
        .expect("gemini_api_key saved");
    assert_eq!(saved_key, "AIzaSyFakeKey123456");

    let saved_provider = my_english::ai::get_setting(&pool, "ai_provider")
        .await
        .unwrap()
        .expect("ai_provider saved");
    assert_eq!(saved_provider, "gemini_api");

    let saved_model = my_english::ai::get_setting(&pool, "gemini_model")
        .await
        .unwrap()
        .expect("gemini_model saved");
    assert_eq!(saved_model, "gemini-2.5-flash");

    // 2. Check that /settings page masks the key
    let res = app
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let html = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(html.contains("AIzaSy...3456"));
    assert!(html.contains("Gemini API"));
}



