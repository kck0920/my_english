use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::cards::{count_due_cards, get_study_card_by_id};
use crate::domain::{
    check_review_answer, compute_srs_transition, mask_context_sentence, AnswerMatch, ClozePrompt,
    ExtraExample, ReviewRating, SrsTransition, StudyCard,
};
use crate::state::AppState;
use crate::ui::{escape_html, APP_CSS};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/review", get(review_page_handler))
        .route("/api/review/next", get(api_review_next_handler))
        .route("/review/{id}/check", post(check_answer_handler))
        .route("/api/review/{id}/check", post(api_check_answer_handler))
        .route("/review/{id}/rate", post(rate_card_handler))
        .route("/api/review/{id}/rate", post(api_rate_card_handler))
        .route("/review/{id}/transfer-prompt", post(transfer_prompt_handler))
        .route("/api/review/{id}/transfer-prompt", post(api_transfer_prompt_handler))
        .route("/review/{id}/transfer-evaluate", post(transfer_evaluate_handler))
        .route("/api/review/{id}/transfer-evaluate", post(api_transfer_evaluate_handler))
        .route("/api/review/logs", get(api_review_logs_handler))
}

#[derive(Debug, Default, Deserialize)]
pub struct ReviewPageQuery {
    #[serde(default)]
    pub reveal: Option<String>,
    #[serde(default)]
    pub card_id: Option<i64>,
    #[serde(default)]
    pub mode: Option<String>,
}

impl ReviewPageQuery {
    pub fn is_revealed(&self) -> bool {
        matches!(
            self.reveal.as_deref().map(str::trim),
            Some("1") | Some("true") | Some("yes")
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextReviewResponse {
    pub due_count: i64,
    pub completed: bool,
    pub card: Option<StudyCard>,
    pub cloze: Option<ClozePrompt>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CheckAnswerInput {
    #[serde(default)]
    pub user_input: Option<String>,
    #[serde(default)]
    pub answer: Option<String>,
}

impl CheckAnswerInput {
    pub fn text(&self) -> &str {
        self.user_input
            .as_deref()
            .or(self.answer.as_deref())
            .unwrap_or("")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckAnswerResponse {
    pub card_id: i64,
    pub user_input: String,
    pub match_result: AnswerMatch,
    pub correct: bool,
    pub chunk: String,
    pub surface_chunk: String,
    pub context_sentence: String,
    pub korean_meaning: String,
    pub extra_examples: Vec<ExtraExample>,
    pub nuance_note: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct RateCardInput {
    pub rating: String,
    #[serde(default)]
    pub user_input: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub ai_feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateCardResponse {
    pub card: StudyCard,
    pub transition: SrsTransition,
    pub due_count: i64,
    pub review_log_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewLogEntry {
    pub id: i64,
    pub card_id: i64,
    pub rating: String,
    pub mode: String,
    pub user_input: String,
    pub ai_feedback: String,
    pub reviewed_at: String,
}

async fn review_page_handler(
    State(state): State<AppState>,
    Query(query): Query<ReviewPageQuery>,
) -> Response {
    let due_count = count_due_cards(&state.pool).await.unwrap_or(0);
    let card_opt = if let Some(cid) = query.card_id {
        get_study_card_by_id(&state.pool, cid).await.unwrap_or(None)
    } else {
        fetch_next_due_card(&state.pool).await.unwrap_or(None)
    };

    match card_opt {
        Some(card) => {
            let is_standard_mode = query.mode.as_deref().map(str::trim) == Some("standard");
            if card.srs_stage >= 3 && !is_standard_mode && !query.is_revealed() {
                // Output Transfer mode for Stage 3+ cards
                match state.ai_provider.generate_output_transfer_prompt(
                    &card.chunk,
                    &card.context_sentence,
                    &card.korean_meaning,
                ).await {
                    Ok(prompt) => {
                        return Html(render_output_transfer_prompt_page(
                            &card,
                            &prompt,
                            due_count,
                            None,
                        ))
                        .into_response();
                    }
                    Err(err) => {
                        tracing::warn!("Failed to generate output transfer prompt, falling back to cloze: {:#}", err);
                    }
                }
            }

            let cloze =
                mask_context_sentence(&card.context_sentence, &card.surface_chunk, &card.chunk);
            Html(render_review_card_page(
                &card,
                &cloze,
                due_count,
                query.is_revealed(),
                None,
                None,
            ))
            .into_response()
        }
        None => {
            let reviewed_today = count_reviews_today(&state.pool).await.unwrap_or(0);
            Html(render_review_complete_page(reviewed_today)).into_response()
        }
    }
}

async fn api_review_next_handler(State(state): State<AppState>) -> Response {
    let due_count = count_due_cards(&state.pool).await.unwrap_or(0);
    let card_opt = fetch_next_due_card(&state.pool).await.unwrap_or(None);
    let cloze = card_opt
        .as_ref()
        .map(|c| mask_context_sentence(&c.context_sentence, &c.surface_chunk, &c.chunk));
    Json(NextReviewResponse {
        due_count,
        completed: card_opt.is_none(),
        card: card_opt,
        cloze,
    })
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct TransferEvaluateInput {
    pub korean_prompt: String,
    pub user_sentence: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransferEvaluateResponse {
    pub card_id: i64,
    pub korean_prompt: String,
    pub user_sentence: String,
    pub passed: bool,
    pub corrected_sentence: String,
    pub feedback: String,
}

pub async fn transfer_prompt_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    api_transfer_prompt_handler(State(state), Path(id)).await
}

pub async fn api_transfer_prompt_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    let Some(card) = (match get_study_card_by_id(&state.pool, id).await {
        Ok(c) => c,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {err}")).into_response(),
    }) else {
        return (StatusCode::NOT_FOUND, "Card not found").into_response();
    };

    match state.ai_provider.generate_output_transfer_prompt(
        &card.chunk,
        &card.context_sentence,
        &card.korean_meaning,
    ).await {
        Ok(prompt) => Json(prompt).into_response(),
        Err(err) => (StatusCode::BAD_GATEWAY, format!("AI error: {err}")).into_response(),
    }
}

pub async fn api_transfer_evaluate_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<TransferEvaluateInput>,
) -> Response {
    transfer_evaluate_impl(state, id, input.korean_prompt, input.user_sentence, true).await
}

pub async fn transfer_evaluate_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let wants_json = headers.get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("application/json"))
        .unwrap_or(false);

    let (korean_prompt, user_sentence) = if let Ok(json_input) = serde_json::from_slice::<TransferEvaluateInput>(&body) {
        (json_input.korean_prompt, json_input.user_sentence)
    } else if let Ok(form) = serde_urlencoded::from_bytes::<std::collections::HashMap<String, String>>(&body) {
        (
            form.get("korean_prompt").cloned().unwrap_or_default(),
            form.get("user_sentence").cloned().unwrap_or_default(),
        )
    } else {
        return (StatusCode::BAD_REQUEST, "Invalid evaluate payload").into_response();
    };

    transfer_evaluate_impl(state, id, korean_prompt, user_sentence, wants_json).await
}

async fn transfer_evaluate_impl(
    state: AppState,
    id: i64,
    korean_prompt: String,
    user_sentence: String,
    wants_json: bool,
) -> Response {
    let Some(card) = (match get_study_card_by_id(&state.pool, id).await {
        Ok(c) => c,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {err}")).into_response(),
    }) else {
        return (StatusCode::NOT_FOUND, "Card not found").into_response();
    };

    let evaluation = match state.ai_provider.evaluate_output_transfer(
        &card.chunk,
        &korean_prompt,
        &user_sentence,
    ).await {
        Ok(ev) => ev,
        Err(err) => return (StatusCode::BAD_GATEWAY, format!("AI evaluation failed: {err}")).into_response(),
    };

    if wants_json {
        Json(TransferEvaluateResponse {
            card_id: card.id,
            korean_prompt,
            user_sentence,
            passed: evaluation.passed,
            corrected_sentence: evaluation.corrected_sentence,
            feedback: evaluation.feedback,
        }).into_response()
    } else {
        let due_count = count_due_cards(&state.pool).await.unwrap_or(0);
        let prompt = crate::domain::OutputTransferPrompt {
            korean_prompt: korean_prompt.clone(),
            hint: String::new(),
        };
        Html(render_output_transfer_prompt_page(
            &card,
            &prompt,
            due_count,
            Some((&user_sentence, &evaluation)),
        )).into_response()
    }
}

pub fn render_output_transfer_prompt_page(
    card: &StudyCard,
    prompt: &crate::domain::OutputTransferPrompt,
    due_count: i64,
    evaluated: Option<(&str, &crate::domain::OutputTransferEvaluation)>,
) -> String {
    let eval_section = if let Some((user_sent, eval)) = evaluated {
        let border_color = if eval.passed { "var(--accent)" } else { "var(--danger)" };
        let status_badge = if eval.passed {
            "<span class=\"stage-badge\" style=\"background: var(--accent); color: white;\">통과 (Pass)</span>"
        } else {
            "<span class=\"stage-badge\" style=\"background: var(--danger); color: white;\">보완 필요</span>"
        };
        format!(
            r#"<section class="panel" style="border-left: 4px solid {border_color}; margin-top: 1rem;">
  <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;">
    <strong>AI 첨삭 및 피드백</strong>
    {status_badge}
  </div>
  <p style="margin: 0.25rem 0;"><strong>내 영작:</strong> {user_sent}</p>
  <p style="margin: 0.25rem 0; color: var(--primary);"><strong>원어민 추천 문장:</strong> {corrected}</p>
  <p style="margin: 0.5rem 0 0 0; font-size: 0.95rem; color: var(--text-muted);">{feedback}</p>

  <div style="margin-top: 1.25rem; display: flex; gap: 0.75rem; flex-wrap: wrap;">
    <form method="POST" action="/review/{id}/rate" style="display:inline;">
      <input type="hidden" name="user_input" value="{user_sent}">
      <input type="hidden" name="ai_feedback" value="{feedback}">
      <input type="hidden" name="mode" value="output_transfer">
      <input type="hidden" name="rating" value="again">
      <button type="submit" class="btn btn-danger">Again (당일 재복습 · Stage 1)</button>
    </form>
    <form method="POST" action="/review/{id}/rate" style="display:inline;">
      <input type="hidden" name="user_input" value="{user_sent}">
      <input type="hidden" name="ai_feedback" value="{feedback}">
      <input type="hidden" name="mode" value="output_transfer">
      <input type="hidden" name="rating" value="good">
      <button type="submit" class="btn btn-primary">Good (+1 Stage)</button>
    </form>
    <form method="POST" action="/review/{id}/rate" style="display:inline;">
      <input type="hidden" name="user_input" value="{user_sent}">
      <input type="hidden" name="ai_feedback" value="{feedback}">
      <input type="hidden" name="mode" value="output_transfer">
      <input type="hidden" name="rating" value="easy">
      <button type="submit" class="btn">Easy (+2 Stages)</button>
    </form>
  </div>
</section>"#,
            border_color = border_color,
            status_badge = status_badge,
            user_sent = escape_html(user_sent),
            corrected = escape_html(&eval.corrected_sentence),
            feedback = escape_html(&eval.feedback),
            id = card.id,
        )
    } else {
        format!(
            r#"<section class="panel" style="margin-top: 1rem;">
  <form method="POST" action="/review/{id}/transfer-evaluate">
    <input type="hidden" name="korean_prompt" value="{korean_prompt}">
    <label for="user_sentence">위 상황을 영어로 작문해 보세요 (Target Chunk: <strong>{chunk}</strong> 활용)</label>
    <textarea id="user_sentence" name="user_sentence" rows="3" style="width: 100%; margin: 0.5rem 0;" autofocus placeholder="영작을 입력하세요..."></textarea>
    <div style="display: flex; gap: 0.75rem; flex-wrap: wrap;">
      <button type="submit" class="btn btn-primary">AI 첨삭 및 제출</button>
      <a href="/review?mode=standard&amp;card_id={id}" class="btn">기존 빈칸 복습으로 전환</a>
    </div>
  </form>
</section>"#,
            id = card.id,
            korean_prompt = escape_html(&prompt.korean_prompt),
            chunk = escape_html(&card.chunk),
        )
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Output Transfer Challenge - My English</title>
  <link rel="manifest" href="/manifest.json">
  <style>{css}</style>
</head>
<body>
  <div class="container">
    <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:1rem;">
      <a href="/" style="text-decoration:none; color:var(--primary); font-size:0.9rem;">← 홈으로</a>
      <span style="font-size:0.9rem; color:var(--text-muted);">남은 복습: <strong>{due_count}</strong></span>
    </div>

    <section class="panel" style="border-left: 4px solid var(--primary);">
      <div class="chunk-header">
        <span class="stage-badge">SRS Stage {stage} · Output Transfer</span>
        <span class="surface-badge">Chunk: {chunk}</span>
      </div>
      <h2 style="font-size: 1.15rem; margin: 0.75rem 0 0.5rem 0; color: var(--text-dark);">
        상황 제시: "{korean_prompt}"
      </h2>
      <p style="font-size: 0.9rem; color: var(--text-muted); margin: 0;">
        (원래 예문: {orig_context})
      </p>
    </section>

    {eval_section}
  </div>
</body>
</html>"#,
        css = APP_CSS,
        due_count = due_count,
        stage = card.srs_stage,
        chunk = escape_html(&card.chunk),
        korean_prompt = escape_html(&prompt.korean_prompt),
        orig_context = escape_html(&card.context_sentence),
        eval_section = eval_section,
    )
}

fn parse_check_payload(headers: &HeaderMap, body: &Bytes) -> Result<(CheckAnswerInput, bool)> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let wants_json = content_type.contains("application/json")
        || headers
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.contains("application/json"))
            .unwrap_or(false);

    if content_type.contains("application/json") {
        let input: CheckAnswerInput = serde_json::from_slice(body)?;
        Ok((input, true))
    } else {
        let input: CheckAnswerInput = serde_urlencoded::from_bytes(body)?;
        Ok((input, wants_json))
    }
}

async fn api_check_answer_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    check_answer_impl(state, id, headers, body, true).await
}

async fn check_answer_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    check_answer_impl(state, id, headers, body, false).await
}

async fn check_answer_impl(
    state: AppState,
    id: i64,
    headers: HeaderMap,
    body: Bytes,
    force_json: bool,
) -> Response {
    let (input, parsed_wants_json) = match parse_check_payload(&headers, &body) {
        Ok(v) => v,
        Err(err) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid payload: {err}")).into_response()
        }
    };
    let wants_json = force_json || parsed_wants_json;

    let Some(card) = (match get_study_card_by_id(&state.pool, id).await {
        Ok(c) => c,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load card: {err}"),
            )
                .into_response()
        }
    }) else {
        return (StatusCode::NOT_FOUND, "Card not found").into_response();
    };

    let cloze = mask_context_sentence(&card.context_sentence, &card.surface_chunk, &card.chunk);
    let effective_surface = if card.surface_chunk.trim().is_empty() {
        cloze.target_surface.as_str()
    } else {
        card.surface_chunk.as_str()
    };
    let user_text = input.text().trim().to_string();
    let match_result = check_review_answer(&user_text, &card.chunk, effective_surface);

    if wants_json {
        let resp = CheckAnswerResponse {
            card_id: card.id,
            user_input: user_text,
            match_result,
            correct: match_result.is_correct(),
            chunk: card.chunk,
            surface_chunk: effective_surface.to_string(),
            context_sentence: card.context_sentence,
            korean_meaning: card.korean_meaning,
            extra_examples: card.extra_examples,
            nuance_note: card.nuance_note,
        };
        (StatusCode::OK, Json(resp)).into_response()
    } else {
        let due_count = count_due_cards(&state.pool).await.unwrap_or(0);
        Html(render_review_card_page(
            &card,
            &cloze,
            due_count,
            true,
            Some(&user_text),
            Some(match_result),
        ))
        .into_response()
    }
}

fn parse_rate_payload(headers: &HeaderMap, body: &Bytes) -> Result<(RateCardInput, bool)> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let wants_json = content_type.contains("application/json")
        || headers
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.contains("application/json"))
            .unwrap_or(false);

    if content_type.contains("application/json") {
        let input: RateCardInput = serde_json::from_slice(body)?;
        Ok((input, true))
    } else {
        let input: RateCardInput = serde_urlencoded::from_bytes(body)?;
        Ok((input, wants_json))
    }
}

async fn api_rate_card_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    rate_card_impl(state, id, headers, body, true).await
}

async fn rate_card_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    rate_card_impl(state, id, headers, body, false).await
}

async fn rate_card_impl(
    state: AppState,
    id: i64,
    headers: HeaderMap,
    body: Bytes,
    force_json: bool,
) -> Response {
    let (input, parsed_wants_json) = match parse_rate_payload(&headers, &body) {
        Ok(v) => v,
        Err(err) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid payload: {err}")).into_response()
        }
    };
    let wants_json = force_json || parsed_wants_json;

    let Some(rating) = ReviewRating::parse(&input.rating) else {
        return (
            StatusCode::BAD_REQUEST,
            "rating must be one of: again, good, easy",
        )
            .into_response();
    };

    let Some(card) = (match get_study_card_by_id(&state.pool, id).await {
        Ok(c) => c,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load card: {err}"),
            )
                .into_response()
        }
    }) else {
        return (StatusCode::NOT_FOUND, "Card not found").into_response();
    };

    let now = chrono::Utc::now();
    let transition = compute_srs_transition(
        card.srs_stage,
        card.review_count,
        card.lapse_count,
        rating,
        now,
    );
    let mode = input
        .mode
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("standard");
    let user_input = input.user_input.as_deref().unwrap_or("").trim();
    let ai_feedback = input.ai_feedback.as_deref().unwrap_or("").trim();

    let (updated_card, review_log_id) = match apply_review_rating(
        &state.pool,
        id,
        rating,
        &transition,
        mode,
        user_input,
        ai_feedback,
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to record review rating: {err}"),
            )
                .into_response()
        }
    };

    if wants_json {
        let due_count = count_due_cards(&state.pool).await.unwrap_or(0);
        let resp = RateCardResponse {
            card: updated_card,
            transition,
            due_count,
            review_log_id,
        };
        (StatusCode::OK, Json(resp)).into_response()
    } else {
        (StatusCode::SEE_OTHER, [(header::LOCATION, "/review")]).into_response()
    }
}

async fn api_review_logs_handler(State(state): State<AppState>) -> Response {
    match list_review_logs(&state.pool).await {
        Ok(logs) => (StatusCode::OK, Json(logs)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list review logs: {err}"),
        )
            .into_response(),
    }
}

pub async fn apply_review_rating(
    pool: &SqlitePool,
    card_id: i64,
    rating: ReviewRating,
    transition: &SrsTransition,
    mode: &str,
    user_input: &str,
    ai_feedback: &str,
) -> Result<(StudyCard, i64)> {
    sqlx::query(
        r#"
        UPDATE study_cards
        SET srs_stage = ?,
            due_date = ?,
            last_reviewed_at = ?,
            review_count = ?,
            lapse_count = ?,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(transition.new_stage)
    .bind(&transition.due_date)
    .bind(&transition.last_reviewed_at)
    .bind(transition.review_count)
    .bind(transition.lapse_count)
    .bind(&transition.last_reviewed_at)
    .bind(card_id)
    .execute(pool)
    .await?;

    let log_res = sqlx::query(
        r#"
        INSERT INTO review_logs (card_id, rating, mode, user_input, ai_feedback, reviewed_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(card_id)
    .bind(rating.as_str())
    .bind(mode)
    .bind(user_input)
    .bind(ai_feedback)
    .bind(&transition.last_reviewed_at)
    .execute(pool)
    .await?;

    let updated = get_study_card_by_id(pool, card_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Updated card not found"))?;
    Ok((updated, log_res.last_insert_rowid()))
}

pub async fn list_review_logs(pool: &SqlitePool) -> Result<Vec<ReviewLogEntry>> {
    let rows = sqlx::query(
        r#"
        SELECT id, card_id, rating, mode, user_input, ai_feedback, reviewed_at
        FROM review_logs
        ORDER BY id DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ReviewLogEntry {
            id: r.get("id"),
            card_id: r.get("card_id"),
            rating: r.get("rating"),
            mode: r.get("mode"),
            user_input: r.get("user_input"),
            ai_feedback: r.get("ai_feedback"),
            reviewed_at: r.get("reviewed_at"),
        })
        .collect())
}

pub async fn fetch_next_due_card(pool: &SqlitePool) -> Result<Option<StudyCard>> {
    let row = sqlx::query(
        r#"
        SELECT id
        FROM study_cards
        WHERE due_date <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        ORDER BY due_date ASC, COALESCE(last_reviewed_at, '') ASC, id ASC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    if let Some(r) = row {
        let id: i64 = r.get("id");
        get_study_card_by_id(pool, id).await
    } else {
        Ok(None)
    }
}

pub async fn count_reviews_today(pool: &SqlitePool) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM review_logs WHERE date(reviewed_at) = date('now')",
    )
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub fn render_review_card_page(
    card: &StudyCard,
    cloze: &ClozePrompt,
    due_count: i64,
    revealed: bool,
    user_input: Option<&str>,
    match_result: Option<AnswerMatch>,
) -> String {
    let feedback_banner = match match_result {
        Some(AnswerMatch::ExactSurface) => format!(
            r#"<div class="panel" style="border-left: 4px solid var(--accent);">
  <strong>정답입니다! (Exact Surface Match)</strong> — 문장 내 활용형 <code>{}</code>과(와) 정확히 일치합니다.
</div>"#,
            escape_html(&cloze.target_surface)
        ),
        Some(AnswerMatch::CanonicalBase) => format!(
            r#"<div class="panel" style="border-left: 4px solid var(--accent);">
  <strong>정답 인정! (Canonical Chunk Match)</strong> — 원형 Chunk <code>{}</code>(으)로 맞히셨습니다. 문장 내 실제 활용형은 <strong>{}</strong>입니다.
</div>"#,
            escape_html(&card.chunk),
            escape_html(&cloze.target_surface)
        ),
        Some(AnswerMatch::Mismatch) => format!(
            r#"<div class="panel" style="border-left: 4px solid var(--danger);">
  <strong>아쉽습니다 (Mismatch)</strong> — 입력값: <code>{}</code> / 정답: <strong>{}</strong> (원형: <code>{}</code>)
</div>"#,
            escape_html(user_input.unwrap_or("")),
            escape_html(&cloze.target_surface),
            escape_html(&card.chunk)
        ),
        None => String::new(),
    };

    let mut examples_html = String::new();
    if !card.extra_examples.is_empty() {
        examples_html.push_str("<ul class=\"extra-examples\">");
        for ex in &card.extra_examples {
            examples_html.push_str(&format!(
                "<li><strong>{}</strong> — {}</li>",
                escape_html(&ex.english),
                escape_html(&ex.korean)
            ));
        }
        examples_html.push_str("</ul>");
    }

    let nuance_html = if card.nuance_note.is_empty() {
        String::new()
    } else {
        format!(
            r#"<p class="nuance-note">{}</p>"#,
            escape_html(&card.nuance_note)
        )
    };

    let body_section = if revealed {
        let user_input_val = escape_html(user_input.unwrap_or(""));
        format!(
            r#"{feedback_banner}
<section class="panel" id="review-answer-side">
  <div class="chunk-header">
    <div>
      <span class="chunk-title">{surface_chunk}</span>
      <span class="surface-badge">원형 Chunk: {chunk}</span>
    </div>
    <span class="stage-badge">SRS Stage {srs_stage}</span>
  </div>
  <div style="display: flex; align-items: center; gap: 0.5rem;">
    <p class="context-sentence" style="font-size: 1.15rem; font-weight: 600; margin: 0;">{context_sentence}</p>
    <button type="button" class="btn" style="padding: 0.25rem 0.5rem; font-size: 0.85rem;" onclick="playReviewAudio(this, decodeURIComponent('{context_sentence_enc}'))">🔊</button>
  </div>
  <p class="korean-meaning">{korean_meaning}</p>
  {examples_html}
  {nuance_html}
  <div style="margin-top: 1rem; display: flex; gap: 0.75rem; flex-wrap: wrap;">
    <form method="POST" action="/review/{id}/rate" style="display:inline;">
      <input type="hidden" name="user_input" value="{user_input_val}">
      <input type="hidden" name="rating" value="again">
      <button type="submit" class="btn btn-danger">Again (당일 재복습 · Stage 1)</button>
    </form>
    <form method="POST" action="/review/{id}/rate" style="display:inline;">
      <input type="hidden" name="user_input" value="{user_input_val}">
      <input type="hidden" name="rating" value="good">
      <button type="submit" class="btn btn-primary">Good (+1 Stage)</button>
    </form>
    <form method="POST" action="/review/{id}/rate" style="display:inline;">
      <input type="hidden" name="user_input" value="{user_input_val}">
      <input type="hidden" name="rating" value="easy">
      <button type="submit" class="btn">Easy (+2 Stages)</button>
    </form>
  </div>
</section>"#,
            id = card.id,
            chunk = escape_html(&card.chunk),
            surface_chunk = escape_html(&cloze.target_surface),
            srs_stage = card.srs_stage,
            context_sentence = escape_html(&card.context_sentence),
            context_sentence_enc = urlencoding::encode(&card.context_sentence),
            korean_meaning = escape_html(&card.korean_meaning),
        )
    } else {
        format!(
            r#"<section class="panel" id="review-prompt-side">
  <div class="chunk-header">
    <span class="stage-badge">SRS Stage {srs_stage}</span>
    <span class="surface-badge" id="word-count-hint">힌트: {word_count} words ({word_count}단어)</span>
  </div>
  <p class="korean-meaning" style="font-size: 1.15rem; font-weight: 600; margin-top: 0.75rem;">{korean_meaning}</p>
  <p class="context-sentence" style="font-size: 1.15rem; margin: 1rem 0;">{masked_sentence}</p>
  <form method="POST" action="/review/{id}/check">
    <label for="user_input">빈칸에 들어갈 영어 Chunk 입력</label>
    <input type="text" id="user_input" name="user_input" autofocus autocomplete="off" placeholder="영어 Chunk를 입력하세요...">
    <div style="display: flex; gap: 0.75rem; flex-wrap: wrap;">
      <button type="submit" class="btn btn-primary">정답 확인 (Check Answer)</button>
      <a href="/review?reveal=1&amp;card_id={id}" class="btn">정답 바로 보기 (Show Answer)</a>
    </div>
  </form>
</section>"#,
            id = card.id,
            srs_stage = card.srs_stage,
            word_count = cloze.word_count,
            korean_meaning = escape_html(&card.korean_meaning),
            masked_sentence = escape_html(&cloze.masked_sentence),
        )
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Review — My English</title>
  <link rel="manifest" href="/manifest.json">
  <style>{APP_CSS}</style>
</head>
<body>
  <div class="container">
    <header class="navbar">
      <strong>My English — Review</strong>
      <nav class="nav-links">
        <a href="/">홈 (Home)</a>
        <span>남은 복습 카드: <strong id="remaining-due-count">{due_count}</strong>장</span>
      </nav>
    </header>
    {body_section}
  </div>
  <script>
    function playReviewAudio(btn, text) {{
      if (!text) return;
      const orig = btn ? btn.innerText : '🔊';
      if (btn) btn.innerText = '⏳';
      let done = false;
      const finish = () => {{
        if (!done) {{
          done = true;
          if (btn) btn.innerText = orig;
        }}
      }};
      try {{
        const audio = new Audio('/api/tts?text=' + encodeURIComponent(text) + '&lang=en');
        audio.onended = finish;
        audio.onerror = () => {{
          if ('speechSynthesis' in window) {{
            try {{
              const u = new SpeechSynthesisUtterance(text);
              u.lang = 'en-US';
              u.onend = finish;
              u.onerror = finish;
              window.speechSynthesis.speak(u);
            }} catch (e) {{ finish(); }}
          }} else {{
            finish();
          }}
        }};
        const p = audio.play();
        if (p !== undefined) {{
          p.catch(() => {{
            audio.onerror();
          }});
        }}
      }} catch (err) {{
        if ('speechSynthesis' in window) {{
          try {{
            const u = new SpeechSynthesisUtterance(text);
            u.lang = 'en-US';
            u.onend = finish;
            u.onerror = finish;
            window.speechSynthesis.speak(u);
          }} catch (e) {{ finish(); }}
        }} else {{
          finish();
        }}
      }}
      setTimeout(finish, 6000);
    }}
  </script>
</body>
</html>"#
    )
}

pub fn render_review_complete_page(reviewed_today: i64) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Review 완료 — My English</title>
  <link rel="manifest" href="/manifest.json">
  <style>{APP_CSS}</style>
</head>
<body>
  <div class="container">
    <header class="navbar">
      <strong>My English — Review</strong>
      <nav class="nav-links">
        <a href="/">홈 (Home)</a>
      </nav>
    </header>
    <section class="panel" id="review-complete-summary">
      <h2>오늘의 Review를 모두 완료했습니다! (Review Complete)</h2>
      <p>현재 대기 중인 복습 카드(Due Study Cards): <strong id="due-card-count">0</strong>장</p>
      <p>오늘 완료한 복습 횟수: <strong>{reviewed_today}</strong>회</p>
      <a href="/" class="btn btn-primary">홈 대시보드로 돌아가기</a>
    </section>
  </div>
</body>
</html>"#
    )
}
