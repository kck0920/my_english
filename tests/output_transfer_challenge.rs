use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use my_english::{
    ai::AiProvider,
    build_app, cards, db,
    domain::{CreateStudyCardInput, DraftCard, OutputTransferEvaluation, OutputTransferPrompt},
    state::AppState,
};
use tower::ServiceExt;

#[derive(Debug, Default)]
struct FakeOutputTransferAiProvider {
    prompt_korean: String,
    eval_passed: bool,
    eval_corrected: String,
    eval_feedback: String,
}

#[async_trait]
impl AiProvider for FakeOutputTransferAiProvider {
    async fn generate_draft_cards(&self, _input: &str) -> Result<Vec<DraftCard>> {
        Ok(vec![])
    }

    async fn generate_output_transfer_prompt(
        &self,
        _chunk: &str,
        _context_sentence: &str,
        _korean_meaning: &str,
    ) -> Result<OutputTransferPrompt> {
        Ok(OutputTransferPrompt {
            korean_prompt: self.prompt_korean.clone(),
            hint: "상황에 맞춰 Chunk를 활용하세요.".to_string(),
        })
    }

    async fn evaluate_output_transfer(
        &self,
        _chunk: &str,
        _korean_prompt: &str,
        _user_sentence: &str,
    ) -> Result<OutputTransferEvaluation> {
        Ok(OutputTransferEvaluation {
            passed: self.eval_passed,
            corrected_sentence: self.eval_corrected.clone(),
            feedback: self.eval_feedback.clone(),
        })
    }
}

async fn test_app_with_ai(
    ai: Arc<FakeOutputTransferAiProvider>,
) -> (axum::Router, String, sqlx::SqlitePool) {
    let pool = db::init_memory_db().await.expect("init memory db");
    let state = AppState::new(pool.clone(), "my-secret-passcode", "my-cookie-signing-key")
        .with_ai_provider(ai);
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
    (app, cookie, pool)
}

#[tokio::test]
async fn review_stage_3_card_triggers_output_transfer_challenge_and_records_evaluation() {
    let fake_ai = Arc::new(FakeOutputTransferAiProvider {
        prompt_korean: "그 사람 농담은 들을 때마다 빵 터져.".to_string(),
        eval_passed: true,
        eval_corrected: "His jokes get me every time.".to_string(),
        eval_feedback: "Chunk 'get me every time'을 완벽하게 활용했습니다!".to_string(),
    });

    let (app, cookie, pool) = test_app_with_ai(fake_ai).await;

    // 1. Create a card and bump it to SRS Stage 3
    let card = cards::insert_study_card(
        &pool,
        &CreateStudyCardInput {
            chunk: "get me every time".into(),
            surface_chunk: "gets me every time".into(),
            context_sentence: "That scene gets me every time.".into(),
            korean_meaning: "볼 때마다 웃겨/울컥해".into(),
            extra_examples: vec![],
            nuance_note: "".into(),
            tags: vec![],
            source_type: Some("manual".into()),
        },
    )
    .await
    .unwrap();

    sqlx::query("UPDATE study_cards SET srs_stage = 3 WHERE id = ?")
        .bind(card.id)
        .execute(&pool)
        .await
        .unwrap();

    // 2. GET /review defaults to Output Transfer prompt for Stage 3 card
    let res_review = app
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
    assert_eq!(res_review.status(), StatusCode::OK);
    let html = String::from_utf8(res_review.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(html.contains("Output Transfer"));
    assert!(html.contains("그 사람 농담은 들을 때마다 빵 터져."));

    // 3. POST /review/{id}/transfer-evaluate evaluates the user's sentence
    let eval_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/review/{}/transfer-evaluate", card.id))
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "korean_prompt=그 사람 농담은 들을 때마다 빵 터져.&user_sentence=His joke gets me every time.",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(eval_res.status(), StatusCode::OK);
    let eval_html = String::from_utf8(eval_res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(eval_html.contains("통과 (Pass)"));
    assert!(eval_html.contains("His jokes get me every time."));
    assert!(eval_html.contains("Good (+1 Stage)"));

    // 4. Rate with mode=output_transfer
    let rate_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/review/{}/rate", card.id))
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "rating=good&mode=output_transfer&user_input=His joke gets me every time.&ai_feedback=Excellent",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rate_res.status(), StatusCode::SEE_OTHER);

    // Verify review_logs recorded output_transfer mode
    let logs = my_english::review::list_review_logs(&pool).await.unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].mode, "output_transfer");
    assert_eq!(logs[0].rating, "good");

    // 5. Switching to standard cloze with ?mode=standard works seamlessly
    let standard_res = app
        .oneshot(
            Request::builder()
                .uri(format!("/review?mode=standard&card_id={}", card.id))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(standard_res.status(), StatusCode::OK);
    let standard_html = String::from_utf8(standard_res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(standard_html.contains("빈칸에 들어갈 영어 Chunk 입력"));
}
