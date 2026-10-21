use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};

use crate::domain::{
    detect_surface_chunk, CardFilter, CreateStudyCardInput, ExtraExample, StudyCard,
    UpdateStudyCardInput,
};
use crate::state::AppState;
use crate::ui;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(dashboard_handler))
        .route("/cards", post(create_card_handler))
        .route("/cards/{id}", get(get_card_handler).put(update_card_handler).delete(delete_card_handler))
        .route("/cards/{id}/edit", post(update_card_handler))
        .route("/cards/{id}/delete", post(delete_card_handler))
        .route("/api/cards", get(api_list_cards_handler))
}

#[derive(Debug, Default, Deserialize)]
pub struct DashboardQuery {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub srs_stage: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
}

impl DashboardQuery {
    pub fn into_filter(self) -> CardFilter {
        let q = self.q.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let tag = self
            .tag
            .map(|s| s.trim().trim_start_matches('#').trim().to_string())
            .filter(|s| !s.is_empty());
        let srs_stage = self
            .srs_stage
            .or(self.stage)
            .and_then(|s| s.trim().parse::<i64>().ok());
        CardFilter { q, tag, srs_stage }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawCardForm {
    #[serde(default)]
    chunk: String,
    #[serde(default)]
    surface_chunk: String,
    #[serde(default)]
    context_sentence: String,
    #[serde(default)]
    korean_meaning: String,
    #[serde(default)]
    extra_examples: String,
    #[serde(default)]
    nuance_note: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    source_type: Option<String>,
    #[serde(default)]
    srs_stage: Option<String>,
}

pub fn parse_extra_examples_str(raw: &str) -> Vec<ExtraExample> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.starts_with('[') {
        if let Ok(parsed) = serde_json::from_str::<Vec<ExtraExample>>(trimmed) {
            return parsed;
        }
    }
    trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            if let Some((en, ko)) = line.split_once('|') {
                ExtraExample {
                    english: en.trim().to_string(),
                    korean: ko.trim().to_string(),
                }
            } else {
                ExtraExample {
                    english: line.to_string(),
                    korean: String::new(),
                }
            }
        })
        .collect()
}

pub fn parse_tags_str(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.starts_with('[') {
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(trimmed) {
            return parsed
                .into_iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
        }
    }
    trimmed
        .split(',')
        .map(|s| s.trim().trim_start_matches('#').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_create_payload(headers: &HeaderMap, body: &Bytes) -> Result<(CreateStudyCardInput, bool)> {
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
        let input: CreateStudyCardInput = serde_json::from_slice(body)?;
        Ok((input, true))
    } else {
        let raw: RawCardForm = serde_urlencoded::from_bytes(body)?;
        let input = CreateStudyCardInput {
            chunk: raw.chunk,
            surface_chunk: raw.surface_chunk,
            context_sentence: raw.context_sentence,
            korean_meaning: raw.korean_meaning,
            extra_examples: parse_extra_examples_str(&raw.extra_examples),
            nuance_note: raw.nuance_note,
            tags: parse_tags_str(&raw.tags),
            source_type: raw.source_type,
        };
        Ok((input, wants_json))
    }
}

async fn dashboard_handler(
    State(state): State<AppState>,
    Query(query): Query<DashboardQuery>,
) -> Response {
    let filter = query.into_filter();
    let all_cards = list_study_cards(&state.pool).await.unwrap_or_default();
    let all_tags = collect_unique_tags(&all_cards);
    let filtered = filter_cards(all_cards, &filter);
    let due_count = count_due_cards(&state.pool).await.unwrap_or(0);
    Html(ui::render_dashboard(
        &filtered,
        due_count,
        &all_tags,
        filter.q.as_deref(),
        filter.tag.as_deref(),
        filter.srs_stage,
    ))
    .into_response()
}

async fn api_list_cards_handler(
    State(state): State<AppState>,
    Query(query): Query<DashboardQuery>,
) -> Response {
    let filter = query.into_filter();
    let all_cards = list_study_cards(&state.pool).await.unwrap_or_default();
    let filtered = filter_cards(all_cards, &filter);
    Json(filtered).into_response()
}

pub fn filter_cards(cards: Vec<StudyCard>, filter: &CardFilter) -> Vec<StudyCard> {
    cards
        .into_iter()
        .filter(|card| {
            if let Some(ref q) = filter.q {
                let q_lower = q.to_lowercase();
                let matches_q = card.chunk.to_lowercase().contains(&q_lower)
                    || card.surface_chunk.to_lowercase().contains(&q_lower)
                    || card.context_sentence.to_lowercase().contains(&q_lower)
                    || card.korean_meaning.to_lowercase().contains(&q_lower);
                if !matches_q {
                    return false;
                }
            }
            if let Some(ref tag) = filter.tag {
                let tag_lower = tag.to_lowercase();
                if !card.tags.iter().any(|t| t.to_lowercase() == tag_lower) {
                    return false;
                }
            }
            if let Some(stage) = filter.srs_stage {
                if card.srs_stage != stage {
                    return false;
                }
            }
            true
        })
        .collect()
}

async fn create_card_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (input, wants_json) = match parse_create_payload(&headers, &body) {
        Ok(v) => v,
        Err(err) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid payload: {err}")).into_response()
        }
    };
    if input.chunk.trim().is_empty() || input.context_sentence.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "chunk and context_sentence are required",
        )
            .into_response();
    }

    match insert_study_card(&state.pool, &input).await {
        Ok(card) => {
            if wants_json {
                (StatusCode::CREATED, Json(card)).into_response()
            } else {
                (StatusCode::SEE_OTHER, [(header::LOCATION, "/")]).into_response()
            }
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create card: {err}"),
        )
            .into_response(),
    }
}

fn parse_update_payload(headers: &HeaderMap, body: &Bytes) -> Result<(UpdateStudyCardInput, bool)> {
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
        let input: UpdateStudyCardInput = serde_json::from_slice(body)?;
        Ok((input, true))
    } else {
        let raw: RawCardForm = serde_urlencoded::from_bytes(body)?;
        let srs_stage = raw
            .srs_stage
            .and_then(|s| s.trim().parse::<i64>().ok())
            .map(|st| st.clamp(0, 5));
        let input = UpdateStudyCardInput {
            chunk: raw.chunk,
            surface_chunk: raw.surface_chunk,
            context_sentence: raw.context_sentence,
            korean_meaning: raw.korean_meaning,
            extra_examples: parse_extra_examples_str(&raw.extra_examples),
            nuance_note: raw.nuance_note,
            tags: parse_tags_str(&raw.tags),
            srs_stage,
        };
        Ok((input, wants_json))
    }
}

async fn get_card_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    match get_study_card_by_id(&state.pool, id).await {
        Ok(Some(card)) => Json(card).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Card not found").into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to fetch card: {err}"),
        )
            .into_response(),
    }
}

async fn update_card_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (input, wants_json) = match parse_update_payload(&headers, &body) {
        Ok(v) => v,
        Err(err) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid payload: {err}")).into_response()
        }
    };
    if input.chunk.trim().is_empty() || input.context_sentence.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "chunk and context_sentence are required",
        )
            .into_response();
    }

    match update_study_card(&state.pool, id, &input).await {
        Ok(Some(card)) => {
            if wants_json {
                (StatusCode::OK, Json(card)).into_response()
            } else {
                (StatusCode::SEE_OTHER, [(header::LOCATION, "/")]).into_response()
            }
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Card not found").into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update card: {err}"),
        )
            .into_response(),
    }
}

async fn delete_card_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    let wants_json = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("application/json"))
        .unwrap_or(false);

    match delete_study_card(&state.pool, id).await {
        Ok(true) => {
            if wants_json {
                StatusCode::NO_CONTENT.into_response()
            } else {
                (StatusCode::SEE_OTHER, [(header::LOCATION, "/")]).into_response()
            }
        }
        Ok(false) => (StatusCode::NOT_FOUND, "Card not found").into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete card: {err}"),
        )
            .into_response(),
    }
}

pub async fn update_study_card(
    pool: &SqlitePool,
    id: i64,
    input: &UpdateStudyCardInput,
) -> Result<Option<StudyCard>> {
    let Some(existing) = get_study_card_by_id(pool, id).await? else {
        return Ok(None);
    };

    let chunk = input.chunk.trim();
    let context_sentence = input.context_sentence.trim();
    let surface_chunk = detect_surface_chunk(
        chunk,
        Some(input.surface_chunk.as_str()),
        context_sentence,
    );
    let korean_meaning = input.korean_meaning.trim();
    let extra_examples_json = serde_json::to_string(&input.extra_examples)?;
    let nuance_note = input.nuance_note.trim();
    let tags_json = serde_json::to_string(&input.tags)?;
    let srs_stage = input.srs_stage.unwrap_or(existing.srs_stage).clamp(0, 5);

    sqlx::query(
        r#"
        UPDATE study_cards
        SET chunk = ?,
            surface_chunk = ?,
            context_sentence = ?,
            korean_meaning = ?,
            extra_examples_json = ?,
            nuance_note = ?,
            tags_json = ?,
            srs_stage = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE id = ?
        "#,
    )
    .bind(chunk)
    .bind(&surface_chunk)
    .bind(context_sentence)
    .bind(korean_meaning)
    .bind(&extra_examples_json)
    .bind(nuance_note)
    .bind(&tags_json)
    .bind(srs_stage)
    .bind(id)
    .execute(pool)
    .await?;

    get_study_card_by_id(pool, id).await
}

pub async fn delete_study_card(pool: &SqlitePool, id: i64) -> Result<bool> {
    let res = sqlx::query("DELETE FROM study_cards WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn insert_study_card(
    pool: &SqlitePool,
    input: &CreateStudyCardInput,
) -> Result<StudyCard> {
    let chunk = input.chunk.trim();
    let context_sentence = input.context_sentence.trim();
    let surface_chunk = detect_surface_chunk(
        chunk,
        Some(input.surface_chunk.as_str()),
        context_sentence,
    );
    let korean_meaning = input.korean_meaning.trim();
    let extra_examples_json = serde_json::to_string(&input.extra_examples)?;
    let nuance_note = input.nuance_note.trim();
    let tags_json = serde_json::to_string(&input.tags)?;
    let source_type = input
        .source_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("manual");

    let result = sqlx::query(
        r#"
        INSERT INTO study_cards (
            chunk, surface_chunk, context_sentence, korean_meaning,
            extra_examples_json, nuance_note, tags_json, source_type
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(chunk)
    .bind(&surface_chunk)
    .bind(context_sentence)
    .bind(korean_meaning)
    .bind(&extra_examples_json)
    .bind(nuance_note)
    .bind(&tags_json)
    .bind(source_type)
    .execute(pool)
    .await?;

    let id = result.last_insert_rowid();
    get_study_card_by_id(pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Inserted card not found"))
}

pub async fn get_study_card_by_id(pool: &SqlitePool, id: i64) -> Result<Option<StudyCard>> {
    let row = sqlx::query(
        r#"
        SELECT id, chunk, surface_chunk, context_sentence, korean_meaning,
               extra_examples_json, nuance_note, tags_json, srs_stage,
               due_date, last_reviewed_at, review_count, lapse_count,
               source_type, created_at, updated_at
        FROM study_cards
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| row_to_study_card(&r)))
}

pub async fn list_study_cards(pool: &SqlitePool) -> Result<Vec<StudyCard>> {
    let rows = sqlx::query(
        r#"
        SELECT id, chunk, surface_chunk, context_sentence, korean_meaning,
               extra_examples_json, nuance_note, tags_json, srs_stage,
               due_date, last_reviewed_at, review_count, lapse_count,
               source_type, created_at, updated_at
        FROM study_cards
        ORDER BY id DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_study_card).collect())
}

pub async fn count_due_cards(pool: &SqlitePool) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM study_cards WHERE due_date <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub fn collect_unique_tags(cards: &[StudyCard]) -> Vec<String> {
    let mut tags = Vec::new();
    for card in cards {
        for tag in &card.tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
    }
    tags.sort();
    tags
}

fn row_to_study_card(r: &sqlx::sqlite::SqliteRow) -> StudyCard {
    let extra_examples_json: String = r.get("extra_examples_json");
    let tags_json: String = r.get("tags_json");
    let extra_examples: Vec<ExtraExample> =
        serde_json::from_str(&extra_examples_json).unwrap_or_default();
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    StudyCard {
        id: r.get("id"),
        chunk: r.get("chunk"),
        surface_chunk: r.get("surface_chunk"),
        context_sentence: r.get("context_sentence"),
        korean_meaning: r.get("korean_meaning"),
        extra_examples,
        nuance_note: r.get("nuance_note"),
        tags,
        srs_stage: r.get("srs_stage"),
        due_date: r.get("due_date"),
        last_reviewed_at: r.get("last_reviewed_at"),
        review_count: r.get("review_count"),
        lapse_count: r.get("lapse_count"),
        source_type: r.get("source_type"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}
