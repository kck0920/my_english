use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use my_english::{build_app, db, state::AppState};
use tower::ServiceExt;

async fn test_app() -> axum::Router {
    let pool = db::init_memory_db().await.expect("init memory db");
    let state = AppState::new(pool, "my-secret-passcode", "my-cookie-signing-key");
    build_app(state)
}

#[tokio::test]
async fn sqlite_initializes_wal_mode_and_required_tables() {
    let tmp = tempfile::NamedTempFile::new().expect("temp file");
    let db_url = format!("sqlite://{}", tmp.path().display());
    let pool = db::init_db(&db_url).await.expect("init file db");

    let mode: (String,) = sqlx::query_as("PRAGMA journal_mode;")
        .fetch_one(&pool)
        .await
        .expect("query pragma journal_mode");
    assert_eq!(mode.0.to_lowercase(), "wal");

    for table in ["study_cards", "review_logs", "app_settings"] {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(table)
        .fetch_optional(&pool)
        .await
        .expect("query sqlite_master");
        assert!(row.is_some(), "expected table `{table}` to exist");
    }
}

#[tokio::test]
async fn unauthenticated_requests_redirect_to_login_while_health_is_public() {
    let app = test_app().await;

    // /health returns 200 OK without auth
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Unauthenticated GET / redirects to /login
    let res = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(
        res.status().is_redirection(),
        "expected redirect status, got {}",
        res.status()
    );
    assert_eq!(
        res.headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );

    // Unauthenticated POST /cards redirects to /login
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/cards")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("chunk=get+me+every+time"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_redirection());
    assert_eq!(
        res.headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/login")
    );

    // GET /login is accessible without auth
    let res = app
        .oneshot(
            Request::builder()
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("passcode"));
}

async fn login_and_get_cookie(app: &axum::Router, passcode: &str) -> (StatusCode, Option<String>) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("passcode={passcode}")))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let set_cookie = res
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    (status, set_cookie)
}

fn extract_cookie_pair(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().trim().to_string()
}

#[tokio::test]
async fn passcode_login_issues_hmac_cookie_and_logout_clears_session() {
    let app = test_app().await;

    // 1. Wrong passcode fails with 401
    let (status, cookie) = login_and_get_cookie(&app, "wrong-passcode").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(cookie.is_none());

    // 2. Correct passcode succeeds with redirect and HttpOnly HMAC-SHA256 cookie
    let (status, set_cookie) = login_and_get_cookie(&app, "my-secret-passcode").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let set_cookie_str = set_cookie.expect("set-cookie header must be present");
    assert!(set_cookie_str.contains("HttpOnly"));
    assert!(set_cookie_str.contains("Max-Age=15552000"));
    let cookie_pair = extract_cookie_pair(&set_cookie_str);

    // 3. Protected route GET / succeeds with valid cookie
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", &cookie_pair)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 4. Tampered HMAC signature is rejected
    let tampered = format!("{cookie_pair}00");
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", tampered)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_redirection());

    // 5. POST /logout clears the session cookie
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/logout")
                .header("cookie", &cookie_pair)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let logout_cookie = res
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .expect("logout set-cookie");
    assert!(logout_cookie.contains("Max-Age=0"));
}

#[tokio::test]
async fn create_study_card_manually_with_automatic_surface_chunk_detection() {
    let app = test_app().await;
    let (_, set_cookie) = login_and_get_cookie(&app, "my-secret-passcode").await;
    let cookie = extract_cookie_pair(&set_cookie.unwrap());

    // Create card without explicit surface_chunk; chunk "get me every time" is inflected as "gets me every time" in Context Sentence
    let form_body = "chunk=get+me+every+time&surface_chunk=&context_sentence=That+final+scene+gets+me+every+time.&korean_meaning=%EA%B7%B8+%EB%A7%88%EC%A7%80%EB%A7%89+%EC%9E%A5%EB%A9%B4%EC%9D%80+%EB%B3%BC+%EB%95%8C%EB%A7%88%EB%8B%A4+%EB%82%98%EB%A5%BC+%EC%9A%B8%EB%A0%A4.&extra_examples=His+dad+jokes+get+me+every+time.+%7C+%EA%B7%B8%EC%9D%98+%EC%95%84%EC%9E%AC+%EA%B0%9C%EA%B7%B8%EB%8A%94+%EB%93%A4%EC%9D%84+%EB%95%8C%EB%A7%88%EB%8B%A4+%EC%9B%83%EA%B2%A8.&nuance_note=Used+when+something+consistently+triggers+an+emotional+reaction.&tags=movie%2C+emotion";
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/cards")
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(form_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);

    // Verify it appears on the home dashboard with detected surface_chunk "gets me every time"
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("get me every time"));
    assert!(html.contains("gets me every time"));
    assert!(html.contains("That final scene gets me every time."));
    assert!(html.contains("그 마지막 장면은 볼 때마다 나를 울려."));
    assert!(html.contains("His dad jokes get me every time."));
    assert!(html.contains("Used when something consistently triggers an emotional reaction."));
    assert!(html.contains("movie"));
    assert!(html.contains("emotion"));
}

#[tokio::test]
async fn search_and_filter_study_cards_by_keyword_tag_and_srs_stage() {
    let app = test_app().await;
    let (_, set_cookie) = login_and_get_cookie(&app, "my-secret-passcode").await;
    let cookie = extract_cookie_pair(&set_cookie.unwrap());

    // Create 2 distinct cards
    let card1 = serde_json::json!({
        "chunk": "call it a day",
        "context_sentence": "Let's call it a day and go home.",
        "korean_meaning": "오늘 일은 여기까지 하고 마무리하다",
        "tags": ["work", "office"]
    });
    let card2 = serde_json::json!({
        "chunk": "play it by ear",
        "context_sentence": "We don't have a fixed plan, so let's just play it by ear.",
        "korean_meaning": "상황 봐서 즉흥적으로 정하다",
        "tags": ["travel", "casual"]
    });

    for payload in [card1, card2] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cards")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    // 1. Search by Chunk keyword ("call it")
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/?q=call+it")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(html.contains("call it a day"));
    assert!(!html.contains("play it by ear"));

    // 2. Search by Context Sentence keyword ("fixed plan")
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/?q=fixed+plan")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(!html.contains("call it a day"));
    assert!(html.contains("play it by ear"));

    // 3. Search by Korean meaning ("마무리하다")
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/?q=%EB%A7%88%EB%AC%B4%EB%A6%AC%ED%95%98%EB%8B%A4")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(html.contains("call it a day"));
    assert!(!html.contains("play it by ear"));

    // 4. Filter by tag ("travel")
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/?tag=travel")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(!html.contains("call it a day"));
    assert!(html.contains("play it by ear"));

    // 5. Filter by SRS Stage (both are stage 0; stage 3 should show neither)
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/?srs_stage=3")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(!html.contains("call it a day"));
    assert!(!html.contains("play it by ear"));
}

#[tokio::test]
async fn edit_and_delete_existing_study_card() {
    let app = test_app().await;
    let (_, set_cookie) = login_and_get_cookie(&app, "my-secret-passcode").await;
    let cookie = extract_cookie_pair(&set_cookie.unwrap());

    // Create a Study Card
    let create_payload = serde_json::json!({
        "chunk": "on the same page",
        "context_sentence": "Let's make sure we are on the same page.",
        "korean_meaning": "생각이 같다, 이해가 일치하다",
        "tags": ["meeting"]
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/cards")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(create_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let created_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&created_bytes).unwrap();
    let id = created["id"].as_i64().expect("card id");

    // 1. Edit via POST /cards/{id}/edit (form submission)
    let edit_form = "chunk=be+on+the+same+page&surface_chunk=are+on+the+same+page&context_sentence=Let%27s+make+sure+we+are+on+the+same+page+before+launching.&korean_meaning=%EC%B6%9C%EC%8B%9C+%EC%A0%84%EC%97%90+%EC%9A%B0%EB%A6%AC+%EC%9D%B4%ED%95%B4%EA%B0%80+%EC%9D%BC%EC%B9%98%ED%95%98%EB%8A%94%EC%A7%80+%ED%99%95%EC%9D%B8%ED%95%98%EC%9E%90.&extra_examples=&nuance_note=Very+common+in+business+meetings.&tags=business%2C+alignment&srs_stage=3";
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/cards/{id}/edit"))
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(edit_form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);

    // Verify updated fields and SRS Stage filter (stage 3) on dashboard
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/?srs_stage=3")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(html.contains("be on the same page"));
    assert!(html.contains("are on the same page"));
    assert!(html.contains("before launching."));
    assert!(html.contains("Very common in business meetings."));
    assert!(html.contains("alignment"));

    // 2. Also test PUT /cards/{id} with JSON
    let put_payload = serde_json::json!({
        "chunk": "be on the same page",
        "surface_chunk": "are on the same page",
        "context_sentence": "We are on the same page now.",
        "korean_meaning": "이제 우리 의견이 일치한다.",
        "extra_examples": [],
        "nuance_note": "Updated via PUT",
        "tags": ["sync"],
        "srs_stage": 4
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/cards/{id}"))
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(put_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. Delete via POST /cards/{id}/delete
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/cards/{id}/delete"))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);

    // Verify card no longer appears on dashboard
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(!html.contains("be on the same page"));

    // 4. PWA manifest is publicly accessible
    let res = app
        .oneshot(
            Request::builder()
                .uri("/manifest.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let manifest_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["short_name"], "MyEnglish");
}




