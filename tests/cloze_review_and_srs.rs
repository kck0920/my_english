use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use my_english::{build_app, db, state::AppState};
use tower::ServiceExt;

async fn test_app_with_cookie() -> (axum::Router, String) {
    let pool = db::init_memory_db().await.expect("init memory db");
    let state = AppState::new(pool, "my-secret-passcode", "my-cookie-signing-key");
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
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let set_cookie = res
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .expect("set-cookie");
    let cookie = set_cookie.split(';').next().unwrap().trim().to_string();
    (app, cookie)
}

async fn create_card_json(app: &axum::Router, cookie: &str, payload: serde_json::Value) -> i64 {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/cards")
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["id"].as_i64().expect("created card id")
}

#[tokio::test]
async fn cloze_review_session_masks_surface_chunk_supports_direct_reveal_and_checks_answers() {
    let (app, cookie) = test_app_with_cookie().await;

    let card_id = create_card_json(
        &app,
        &cookie,
        serde_json::json!({
            "chunk": "get me every time",
            "surface_chunk": "gets me every time",
            "context_sentence": "That final scene gets me every time.",
            "korean_meaning": "그 마지막 장면은 볼 때마다 나를 울려.",
            "extra_examples": [
                {
                    "english": "His dad jokes get me every time.",
                    "korean": "그의 아재 개그는 들을 때마다 웃겨."
                }
            ],
            "nuance_note": "반복적으로 감정을 자극할 때 쓰는 표현",
            "tags": ["movie", "emotion"]
        }),
    )
    .await;

    // 1. GET /review shows masked Context Sentence, Korean meaning, and word-count hint (4 words)
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/review")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let html = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(html.contains("그 마지막 장면은 볼 때마다 나를 울려."));
    assert!(html.contains("That final scene [ _______________ ]."));
    assert!(html.contains("4"));

    // 2. GET /api/review/next returns structured JSON with cloze prompt
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/review/next")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let next_json: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(next_json["due_count"], 1);
    assert_eq!(next_json["completed"], false);
    assert_eq!(next_json["card"]["id"], card_id);
    assert_eq!(
        next_json["cloze"]["masked_sentence"],
        "That final scene [ _______________ ]."
    );
    assert_eq!(next_json["cloze"]["word_count"], 4);

    // 3. Direct reveal without typing: GET /review?reveal=1 reveals the answer side immediately
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/review?reveal=1")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let reveal_html =
        String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(reveal_html.contains("gets me every time"));
    assert!(reveal_html.contains("That final scene gets me every time."));
    assert!(reveal_html.contains("His dad jokes get me every time."));
    assert!(reveal_html.contains("반복적으로 감정을 자극할 때 쓰는 표현"));

    // 4. POST /review/{id}/check distinguishes ExactSurface, CanonicalBase, and Mismatch
    for (input, expected_match, expected_correct) in [
        ("Gets me every time!", "exact_surface", true),
        ("get me every time", "canonical_base", true),
        ("take my time", "mismatch", false),
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/review/{card_id}/check"))
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "user_input": input }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let check_json: serde_json::Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(check_json["match_result"], expected_match);
        assert_eq!(check_json["correct"], expected_correct);
        assert_eq!(check_json["surface_chunk"], "gets me every time");
    }
}

#[tokio::test]
async fn srs_stage_rating_requeues_again_today_advances_good_and_easy_and_records_review_logs() {
    let (app, cookie) = test_app_with_cookie().await;

    let card1_id = create_card_json(
        &app,
        &cookie,
        serde_json::json!({
            "chunk": "call it a day",
            "context_sentence": "Let's call it a day and head home.",
            "korean_meaning": "오늘 일은 여기까지 하고 마무리하다"
        }),
    )
    .await;

    let card2_id = create_card_json(
        &app,
        &cookie,
        serde_json::json!({
            "chunk": "play it by ear",
            "context_sentence": "Let's just play it by ear tomorrow.",
            "korean_meaning": "상황 봐서 즉흥적으로 정하다"
        }),
    )
    .await;

    // Initially 2 cards are due on dashboard and /api/review/next
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/review/next")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let next_json: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(next_json["due_count"], 2);
    assert_eq!(next_json["card"]["id"], card1_id);

    // 1. Rate card 1 as `Again` -> resets to SRS Stage 1, stays due today (due_count remains 2), increments lapse_count and review_count
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/review/{card1_id}/rate"))
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "rating": "again",
                        "user_input": "call a day"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let rate1: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(rate1["card"]["srs_stage"], 1);
    assert_eq!(rate1["card"]["review_count"], 1);
    assert_eq!(rate1["card"]["lapse_count"], 1);
    assert_eq!(rate1["due_count"], 2);

    // 2. Rate card 2 (Stage 0) as `Easy` -> jumps +2 stages to Stage 2 (3d), leaving only card 1 due today
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/review/{card2_id}/rate"))
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "rating": "easy",
                        "user_input": "play it by ear"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let rate2: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(rate2["card"]["srs_stage"], 2);
    assert_eq!(rate2["card"]["review_count"], 1);
    assert_eq!(rate2["card"]["lapse_count"], 0);
    assert_eq!(rate2["due_count"], 1);

    // 3. Re-review card 1 (currently Stage 1, re-queued today) and rate as `Good` via HTML form -> advances to Stage 2 (3d) and redirects to /review
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/review/{card1_id}/rate"))
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("rating=good&user_input=call+it+a+day"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        res.headers().get("location").and_then(|v| v.to_str().ok()),
        Some("/review")
    );

    // 4. Verify card 1 is now at Stage 2 with review_count=2, lapse_count=1
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&format!("/cards/{card1_id}"))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let card1_after: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(card1_after["srs_stage"], 2);
    assert_eq!(card1_after["review_count"], 2);
    assert_eq!(card1_after["lapse_count"], 1);

    // 5. Due queue is now empty: GET /review shows completion summary and GET / shows 0 due cards
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/review")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let complete_html =
        String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(complete_html.contains("Review Complete"));
    assert!(complete_html.contains("3")); // 3 reviews completed today

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
    let dash_html =
        String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(dash_html.contains(r#"<span id="due-card-count">0</span>"#));

    // 6. Verify 3 review_logs entries were recorded
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/review/logs")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let logs: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let arr = logs.as_array().expect("logs array");
    assert_eq!(arr.len(), 3);
}

#[tokio::test]
async fn cloze_review_handles_inflected_surface_when_card_surface_chunk_was_canonical() {
    let (app, cookie) = test_app_with_cookie().await;

    // Card created with surface_chunk intentionally set to canonical form or empty
    let card_id = create_card_json(
        &app,
        &cookie,
        serde_json::json!({
            "chunk": "feel like",
            "surface_chunk": "feel like", // Even if stored as canonical
            "context_sentence": "It looks like rain, and it even feels like snow because it is so cold.",
            "korean_meaning": "마치 ~인 것 같다, ~처럼 느껴지다",
            "extra_examples": [],
            "nuance_note": ""
        }),
    )
    .await;

    // 1. User inputs correct inflected form "feels like" -> Exact Surface Match!
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/review/{card_id}/check"))
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("user_input=feels+like"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let html =
        String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(
        html.contains("정답입니다! (Exact Surface Match)"),
        "Expected Exact Surface Match banner, but got: {html}"
    );
    assert!(!html.contains("아쉽습니다 (Mismatch)"));

    // 2. User inputs with multiple spaces "feels   like" -> Also Exact Surface Match!
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/review/{card_id}/check"))
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("user_input=feels+++like"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let html2 =
        String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(html2.contains("정답입니다! (Exact Surface Match)"));

    // 3. User inputs canonical chunk "feel like" -> Canonical Base Match!
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/review/{card_id}/check"))
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("user_input=feel+like"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let html3 =
        String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
    assert!(html3.contains("정답 인정! (Canonical Chunk Match)"));
}

