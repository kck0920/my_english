use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{FromRequest, Multipart, Request, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, Connection, Row, SqliteConnection, SqlitePool};

use crate::cards::{insert_study_card, list_study_cards};
use crate::domain::{detect_surface_chunk, CreateStudyCardInput, ExtraExample, StudyCard};
use crate::state::AppState;
use crate::ui::{escape_html, APP_CSS};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/data", get(data_page_handler))
        .route("/import/text", post(import_text_handler))
        .route("/api/import/text", post(api_import_text_handler))
        .route("/import/sqlite", post(import_sqlite_handler))
        .route("/api/import/sqlite", post(import_sqlite_handler))
        .route("/export/sqlite", get(export_sqlite_handler))
        .route("/export/json", get(export_json_handler))
        .route("/export/anki.csv", get(export_anki_csv_handler))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSummary {
    pub imported_count: usize,
    pub skipped_count: usize,
    pub cards: Vec<StudyCard>,
}

#[derive(Debug, Default, Deserialize)]
struct ImportTextPayload {
    #[serde(default)]
    text: String,
    #[serde(default)]
    raw_text: String,
}

impl ImportTextPayload {
    fn into_text(self) -> String {
        if !self.text.trim().is_empty() {
            self.text
        } else {
            self.raw_text
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedLegacyNote {
    pub original: String,
    pub correct_translation: String,
    pub used_context: String,
    pub nuance: String,
    pub alternatives: String,
    pub anki_card: String,
    pub natural: String,
    pub analysis: String,
    pub examples: String,
    pub raw_text: String,
}

impl ParsedLegacyNote {
    fn has_any_section(&self) -> bool {
        !self.original.trim().is_empty()
            || !self.correct_translation.trim().is_empty()
            || !self.used_context.trim().is_empty()
            || !self.nuance.trim().is_empty()
            || !self.alternatives.trim().is_empty()
            || !self.anki_card.trim().is_empty()
            || !self.natural.trim().is_empty()
            || !self.analysis.trim().is_empty()
            || !self.examples.trim().is_empty()
    }

    fn has_non_original_section(&self) -> bool {
        !self.correct_translation.trim().is_empty()
            || !self.used_context.trim().is_empty()
            || !self.nuance.trim().is_empty()
            || !self.alternatives.trim().is_empty()
            || !self.anki_card.trim().is_empty()
            || !self.natural.trim().is_empty()
            || !self.analysis.trim().is_empty()
            || !self.examples.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacySection {
    Original,
    CorrectTranslation,
    UsedContext,
    Nuance,
    Alternatives,
    AnkiCard,
    LegacyNatural,
    LegacyAnalysis,
    LegacyExamples,
}

fn match_section_header(line: &str) -> Option<(LegacySection, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Strip leading markdown/list markers: #, *, >, -, digits, dots, spaces
    let stripped = trimmed
        .trim_start_matches(|c: char| {
            matches!(c, '#' | '*' | '>' | '-' | ' ' | '\t') || c.is_ascii_digit() || c == '.'
        })
        .trim();

    let headers: &[(&[&str], LegacySection)] = &[
        (&["[원문]", "원문"], LegacySection::Original),
        (
            &["[올바른 해석]", "[올바른해석]", "올바른 해석", "올바른해석"],
            LegacySection::CorrectTranslation,
        ),
        (
            &[
                "[이 문장이 실제로 사용되는 상황]",
                "이 문장이 실제로 사용되는 상황",
            ],
            LegacySection::UsedContext,
        ),
        (
            &[
                "[미국인이 가장 자연스럽게 쓰는 실제 뉘앙스와 감각]",
                "미국인이 가장 자연스럽게 쓰는 실제 뉘앙스와 감각",
            ],
            LegacySection::Nuance,
        ),
        (
            &[
                "[미국인이 입버릇처럼 쓰는 실전 대체 표현]",
                "미국인이 입버릇처럼 쓰는 실전 대체 표현",
            ],
            LegacySection::Alternatives,
        ),
        (
            &[
                "[30년 차 강사의 실전 Anki 카드 정리]",
                "[30년차 강사의 실전 Anki 카드 정리]",
                "30년 차 강사의 실전 Anki 카드 정리",
                "30년차 강사의 실전 Anki 카드 정리",
            ],
            LegacySection::AnkiCard,
        ),
        (
            &["[자연스러운 표현]", "자연스러운 표현"],
            LegacySection::LegacyNatural,
        ),
        (&["[분석]", "분석"], LegacySection::LegacyAnalysis),
        (&["[예문]", "예문"], LegacySection::LegacyExamples),
    ];

    for (patterns, section) in headers {
        for pat in *patterns {
            if let Some(rest) = stripped.strip_prefix(pat) {
                let after = rest
                    .trim_start_matches('*')
                    .trim_start_matches(':')
                    .trim();
                // For plain word headers without brackets, only match if rest was empty or had `:` / `**`
                let had_brackets = pat.starts_with('[');
                let had_delimiter = rest.is_empty()
                    || rest.starts_with('*')
                    || rest.starts_with(':')
                    || rest.starts_with(']');
                if had_brackets || (had_delimiter && after.is_empty()) {
                    return Some((*section, after.to_string()));
                }
            }
        }
    }
    None
}

fn append_section_line(note: &mut ParsedLegacyNote, section: LegacySection, line: &str) {
    let target = match section {
        LegacySection::Original => &mut note.original,
        LegacySection::CorrectTranslation => &mut note.correct_translation,
        LegacySection::UsedContext => &mut note.used_context,
        LegacySection::Nuance => &mut note.nuance,
        LegacySection::Alternatives => &mut note.alternatives,
        LegacySection::AnkiCard => &mut note.anki_card,
        LegacySection::LegacyNatural => &mut note.natural,
        LegacySection::LegacyAnalysis => &mut note.analysis,
        LegacySection::LegacyExamples => &mut note.examples,
    };
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(line);
}

pub fn parse_legacy_notes(text: &str) -> Vec<ParsedLegacyNote> {
    let mut notes: Vec<ParsedLegacyNote> = Vec::new();
    let mut current = ParsedLegacyNote {
        raw_text: text.trim().to_string(),
        ..Default::default()
    };
    let mut current_section: Option<LegacySection> = None;

    for line in text.lines() {
        if let Some((sec, inline_text)) = match_section_header(line) {
            if sec == LegacySection::Original && current.has_non_original_section() {
                finalize_note_fields(&mut current);
                notes.push(current);
                current = ParsedLegacyNote::default();
            }
            current_section = Some(sec);
            if !inline_text.is_empty() {
                append_section_line(&mut current, sec, &inline_text);
            }
            continue;
        }

        if let Some(sec) = current_section {
            append_section_line(&mut current, sec, line);
        }
    }

    finalize_note_fields(&mut current);
    if current.has_any_section() {
        notes.push(current);
    }
    notes
}

fn finalize_note_fields(note: &mut ParsedLegacyNote) {
    note.original = note.original.trim().to_string();
    note.correct_translation = note.correct_translation.trim().to_string();
    note.used_context = note.used_context.trim().to_string();
    note.nuance = note.nuance.trim().to_string();
    note.alternatives = note.alternatives.trim().to_string();
    note.anki_card = note.anki_card.trim().to_string();
    note.natural = note.natural.trim().to_string();
    note.analysis = note.analysis.trim().to_string();
    note.examples = note.examples.trim().to_string();
}

pub fn legacy_note_to_card_input(note: &ParsedLegacyNote) -> Option<CreateStudyCardInput> {
    let raw_sentence = if !note.original.is_empty() && has_ascii_alpha(&note.original) {
        first_meaningful_line(&note.original)
    } else if !note.natural.is_empty() && has_ascii_alpha(&note.natural) {
        first_meaningful_line(&note.natural)
    } else if !note.original.is_empty() {
        first_meaningful_line(&note.original)
    } else if !note.raw_text.is_empty() {
        first_meaningful_line(&note.raw_text)
    } else {
        String::new()
    };

    let (context_sentence, cloze_in_sentence) = clean_sentence_and_extract_cloze(&raw_sentence);
    if context_sentence.is_empty() {
        return None;
    }

    let chunk = extract_target_chunk(note, &context_sentence, cloze_in_sentence.as_deref());
    let surface_chunk = detect_surface_chunk(&chunk, None, &context_sentence);

    let korean_meaning = extract_korean_meaning(note);
    let nuance_note = build_nuance_note(note, &context_sentence);
    let extra_examples = extract_extra_examples(note, &context_sentence);

    Some(CreateStudyCardInput {
        chunk,
        surface_chunk,
        context_sentence,
        korean_meaning,
        extra_examples,
        nuance_note,
        tags: vec!["legacy".to_string()],
        source_type: Some("legacy_import".to_string()),
    })
}

fn has_ascii_alpha(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_alphabetic())
}

fn first_meaningful_line(s: &str) -> String {
    for line in s.lines() {
        let cleaned = strip_leading_list_and_quotes(line);
        if !cleaned.is_empty() {
            return cleaned;
        }
    }
    String::new()
}

fn strip_leading_list_and_quotes(line: &str) -> String {
    let mut s = line.trim();
    // Strip leading bullet or number like "1. ", "- ", "* ", "> "
    if let Some(rest) = s
        .strip_prefix("- ")
        .or_else(|| s.strip_prefix("* "))
        .or_else(|| s.strip_prefix("> "))
    {
        s = rest.trim();
    } else if let Some(dot_idx) = s.find('.') {
        if dot_idx <= 3 && s[..dot_idx].chars().all(|c| c.is_ascii_digit()) {
            s = s[dot_idx + 1..].trim();
        }
    }
    s = s.trim_matches('*').trim_matches('"').trim_matches('“').trim_matches('”').trim();
    s.to_string()
}

fn clean_sentence_and_extract_cloze(raw: &str) -> (String, Option<String>) {
    // Handle Anki cloze {{c1::chunk::hint}} or {{c1::chunk}}
    if let Some((before, inner, after)) = split_anki_cloze(raw) {
        let joined = format!("{before}{inner}{after}").replace("**", "");
        return (joined.trim().to_string(), Some(inner));
    }
    let cleaned = raw.replace("**", "").trim().to_string();
    (cleaned, None)
}

fn split_anki_cloze(s: &str) -> Option<(String, String, String)> {
    let start = s.find("{{c")?;
    let after_c = &s[start + 3..];
    let colon_pos = after_c.find("::")?;
    let content_start = start + 3 + colon_pos + 2;
    let rest = &s[content_start..];
    let end_rel = rest.find("}}")?;
    let inside = &rest[..end_rel];
    let phrase = inside.split("::").next().unwrap_or(inside).trim().to_string();
    let before = s[..start].to_string();
    let after = rest[end_rel + 2..].to_string();
    if phrase.is_empty() {
        None
    } else {
        Some((before, phrase, after))
    }
}

fn extract_target_chunk(
    note: &ParsedLegacyNote,
    context_sentence: &str,
    cloze_from_sentence: Option<&str>,
) -> String {
    if let Some(c) = cloze_from_sentence {
        if !c.trim().is_empty() {
            return c.trim().to_string();
        }
    }

    // 1. Check Anki card for {{c1::...}}
    if let Some((_, inner, _)) = split_anki_cloze(&note.anki_card) {
        if chunk_matches_in_sentence(&inner, context_sentence) {
            return trim_chunk_punctuation(&inner);
        }
    }

    // 2. Check bold **phrase** or single-quoted 'phrase' across sections
    let candidate_sources = [
        &note.original,
        &note.anki_card,
        &note.nuance,
        &note.analysis,
        &note.correct_translation,
    ];
    for src in candidate_sources {
        for cand in extract_delimited_phrases(src, "**", "**") {
            if is_reasonable_chunk(&cand, context_sentence)
                && chunk_matches_in_sentence(&cand, context_sentence)
            {
                return trim_chunk_punctuation(&cand);
            }
        }
        for cand in extract_single_quoted_phrases(src) {
            if is_reasonable_chunk(&cand, context_sentence)
                && chunk_matches_in_sentence(&cand, context_sentence)
            {
                return trim_chunk_punctuation(&cand);
            }
        }
    }

    // 3. Fallback: derive core verb/idiom chunk from context_sentence
    derive_chunk_from_sentence(context_sentence)
}

fn extract_delimited_phrases(text: &str, open: &str, close: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let after_open = &rest[start + open.len()..];
        if let Some(end) = after_open.find(close) {
            let inner = after_open[..end].trim();
            if !inner.is_empty() && has_ascii_alpha(inner) {
                results.push(inner.to_string());
            }
            rest = &after_open[end + close.len()..];
        } else {
            break;
        }
    }
    results
}

fn extract_single_quoted_phrases(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    for line in text.lines() {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\'' {
                // Skip contraction apostrophes preceded by ASCII alphabetic character
                let preceded_by_alpha = i > 0 && bytes[i - 1].is_ascii_alphabetic();
                if !preceded_by_alpha {
                    if let Some(end_offset) = line[i + 1..].find('\'') {
                        let candidate = &line[i + 1..i + 1 + end_offset];
                        if !candidate.is_empty() && has_ascii_alpha(candidate) {
                            results.push(candidate.trim().to_string());
                            i += 1 + end_offset + 1;
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }
    }
    results
}

fn is_reasonable_chunk(cand: &str, sentence: &str) -> bool {
    let cleaned = trim_chunk_punctuation(cand);
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.is_empty() || words.len() > 7 {
        return false;
    }
    let sentence_clean = trim_chunk_punctuation(sentence);
    if words.len() >= 5 && cleaned.eq_ignore_ascii_case(&sentence_clean) {
        return false;
    }
    true
}

fn chunk_matches_in_sentence(cand: &str, sentence: &str) -> bool {
    let cleaned = trim_chunk_punctuation(cand);
    if cleaned.is_empty() {
        return false;
    }
    let detected = detect_surface_chunk(&cleaned, None, sentence);
    sentence
        .to_lowercase()
        .contains(&detected.to_lowercase())
}

fn trim_chunk_punctuation(s: &str) -> String {
    s.trim()
        .trim_matches(|c: char| matches!(c, '.' | ',' | '!' | '?' | '"' | '\'' | ';' | ':'))
        .trim()
        .to_string()
}

fn derive_chunk_from_sentence(sentence: &str) -> String {
    let cleaned = trim_chunk_punctuation(sentence);
    let prefixes = [
        "I think I'm ",
        "I think I am ",
        "Let's just ",
        "Let's ",
        "I'm ",
        "I am ",
        "I've ",
        "I have ",
        "I'd ",
        "I ",
        "You're ",
        "You are ",
        "You ",
        "We're ",
        "We are ",
        "We ",
        "They're ",
        "They are ",
        "They ",
        "He's ",
        "He is ",
        "She's ",
        "She is ",
        "It's ",
        "It is ",
    ];

    for prefix in prefixes {
        if cleaned.len() > prefix.len()
            && cleaned[..prefix.len()].eq_ignore_ascii_case(prefix)
        {
            let rest = cleaned[prefix.len()..].trim();
            if !rest.is_empty() {
                // Keep "have ..." when prefix was "I "
                return rest.to_string();
            }
        }
    }

    cleaned
}

fn extract_korean_meaning(note: &ParsedLegacyNote) -> String {
    if !note.correct_translation.is_empty() {
        let first = first_meaningful_line(&note.correct_translation);
        if !first.is_empty() {
            return first;
        }
    }
    if !note.natural.is_empty() {
        let first = first_meaningful_line(&note.natural);
        if !first.is_empty() {
            return first;
        }
    }
    if !note.analysis.is_empty() {
        let first = first_meaningful_line(&note.analysis);
        if !first.is_empty() {
            return first;
        }
    }
    if !note.nuance.is_empty() {
        let first = first_meaningful_line(&note.nuance);
        if !first.is_empty() {
            return first;
        }
    }
    "의미 미입력".to_string()
}

fn build_nuance_note(note: &ParsedLegacyNote, context_sentence: &str) -> String {
    let mut parts = Vec::new();
    if !note.used_context.is_empty() {
        parts.push(note.used_context.trim().to_string());
    }
    if !note.nuance.is_empty() {
        parts.push(note.nuance.trim().to_string());
    }
    if !note.analysis.is_empty() {
        parts.push(note.analysis.trim().to_string());
    }
    if !note.natural.is_empty() && !note.natural.trim().eq_ignore_ascii_case(context_sentence) {
        let nat = note.natural.trim();
        if !parts.iter().any(|p| p.contains(nat)) {
            parts.push(format!("자연스러운 표현: {nat}"));
        }
    }
    parts.join("\n\n")
}

fn extract_extra_examples(note: &ParsedLegacyNote, context_sentence: &str) -> Vec<ExtraExample> {
    let mut examples = Vec::new();
    for block in [&note.alternatives, &note.examples] {
        for raw_line in block.lines() {
            let line = strip_leading_list_and_quotes(raw_line);
            if line.is_empty() || !has_ascii_alpha(&line) {
                continue;
            }
            if line.eq_ignore_ascii_case(context_sentence) {
                continue;
            }
            let (en, ko) = if let Some((e, k)) = line.split_once('|') {
                (e.trim().to_string(), k.trim().to_string())
            } else {
                (line, String::new())
            };
            if !en.is_empty() {
                examples.push(ExtraExample {
                    english: en,
                    korean: ko,
                });
            }
        }
    }
    examples
}

pub async fn import_card_inputs_dedup(
    pool: &SqlitePool,
    inputs: Vec<CreateStudyCardInput>,
) -> Result<ImportSummary> {
    let mut imported_cards = Vec::new();
    let mut skipped_count = 0usize;

    for mut input in inputs {
        input.source_type = Some("legacy_import".to_string());
        let chunk = input.chunk.trim();
        let sentence = input.context_sentence.trim();
        if chunk.is_empty() || sentence.is_empty() {
            continue;
        }

        let exists: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM study_cards
            WHERE LOWER(TRIM(chunk)) = LOWER(TRIM(?))
              AND LOWER(TRIM(context_sentence)) = LOWER(TRIM(?))
            "#,
        )
        .bind(chunk)
        .bind(sentence)
        .fetch_one(pool)
        .await?;

        if exists.0 > 0 {
            skipped_count += 1;
            continue;
        }

        let card = insert_study_card(pool, &input).await?;
        imported_cards.push(card);
    }

    Ok(ImportSummary {
        imported_count: imported_cards.len(),
        skipped_count,
        cards: imported_cards,
    })
}

fn extract_raw_text_from_request(headers: &HeaderMap, body: &Bytes) -> Result<String> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("application/json") {
        let payload: ImportTextPayload = serde_json::from_slice(body)?;
        Ok(payload.into_text())
    } else if content_type.contains("application/x-www-form-urlencoded") {
        let payload: ImportTextPayload = serde_urlencoded::from_bytes(body)?;
        Ok(payload.into_text())
    } else {
        Ok(String::from_utf8_lossy(body).to_string())
    }
}

fn wants_html_response(headers: &HeaderMap) -> bool {
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    accept.contains("text/html") && !accept.contains("application/json")
}

async fn import_text_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    handle_import_text(state, headers, body, false).await
}

async fn api_import_text_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    handle_import_text(state, headers, body, true).await
}

async fn handle_import_text(
    state: AppState,
    headers: HeaderMap,
    body: Bytes,
    force_json: bool,
) -> Response {
    let text = match extract_raw_text_from_request(&headers, &body) {
        Ok(t) => t,
        Err(err) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid import payload: {err}"))
                .into_response()
        }
    };

    if text.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "Import text cannot be empty").into_response();
    }

    let parsed_notes = parse_legacy_notes(&text);
    let inputs: Vec<CreateStudyCardInput> = parsed_notes
        .iter()
        .filter_map(legacy_note_to_card_input)
        .collect();

    match import_card_inputs_dedup(&state.pool, inputs).await {
        Ok(summary) => {
            if !force_json && wants_html_response(&headers) {
                let msg = format!(
                    "가져오기 완료: {}개 추가됨 (중복 제외 {}개)",
                    summary.imported_count, summary.skipped_count
                );
                Html(render_data_page(Some(&msg))).into_response()
            } else {
                (StatusCode::OK, Json(summary)).into_response()
            }
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to import text: {err}"),
        )
            .into_response(),
    }
}

fn unique_temp_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let rand_u64: u64 = rand::thread_rng().r#gen();
    let pid = std::process::id();
    std::env::temp_dir().join(format!("{prefix}_{pid}_{nanos}_{rand_u64:x}.{ext}"))
}

async fn extract_sqlite_bytes(state: &AppState, request: Request) -> Result<Vec<u8>> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.starts_with("multipart/form-data") {
        let mut multipart = Multipart::from_request(request, state)
            .await
            .map_err(|e| anyhow::anyhow!("Invalid multipart upload: {e}"))?;
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|e| anyhow::anyhow!("Multipart field error: {e}"))?
        {
            let data = field
                .bytes()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to read field bytes: {e}"))?;
            if !data.is_empty() {
                return Ok(data.to_vec());
            }
        }
        anyhow::bail!("No file found in multipart upload");
    } else {
        let bytes = axum::body::to_bytes(request.into_body(), 50 * 1024 * 1024)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read body bytes: {e}"))?;
        if bytes.is_empty() {
            anyhow::bail!("Empty request body");
        }
        Ok(bytes.to_vec())
    }
}

pub async fn parse_notes_from_sqlite_bytes(sqlite_bytes: &[u8]) -> Result<Vec<CreateStudyCardInput>> {
    if !sqlite_bytes.starts_with(b"SQLite format 3") {
        anyhow::bail!("Invalid SQLite file header");
    }

    let tmp_path = unique_temp_path("my_english_import", "db");
    tokio::fs::write(&tmp_path, sqlite_bytes).await?;

    let result = read_notes_from_sqlite_path(&tmp_path).await;

    let _ = tokio::fs::remove_file(&tmp_path).await;
    let _ = tokio::fs::remove_file(format!("{}-wal", tmp_path.display())).await;
    let _ = tokio::fs::remove_file(format!("{}-shm", tmp_path.display())).await;

    result
}

async fn read_notes_from_sqlite_path(path: &PathBuf) -> Result<Vec<CreateStudyCardInput>> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false);
    let mut conn = SqliteConnection::connect_with(&opts).await?;

    let rows = sqlx::query("SELECT * FROM notes ORDER BY id ASC")
        .fetch_all(&mut conn)
        .await?;

    let mut inputs = Vec::new();
    for row in rows {
        let raw_text: String = row.try_get("raw_text").unwrap_or_default();
        let col_original: String = row.try_get("original").unwrap_or_default();
        let col_translation: String = row.try_get("correct_translation").unwrap_or_default();
        let col_context: String = row.try_get("used_context").unwrap_or_default();
        let col_nuance: String = row.try_get("nuance").unwrap_or_default();
        let col_alternatives: String = row.try_get("alternatives").unwrap_or_default();
        let col_anki: String = row.try_get("anki_card").unwrap_or_default();
        let col_natural: String = row.try_get("natural").unwrap_or_default();
        let col_analysis: String = row.try_get("analysis").unwrap_or_default();
        let col_examples: String = row.try_get("examples").unwrap_or_default();

        let fallback = parse_legacy_notes(&raw_text)
            .into_iter()
            .next()
            .unwrap_or_default();

        let pick = |col: String, fb: String| -> String {
            if !col.trim().is_empty() {
                col.trim().to_string()
            } else {
                fb.trim().to_string()
            }
        };

        let merged = ParsedLegacyNote {
            original: pick(col_original, fallback.original),
            correct_translation: pick(col_translation, fallback.correct_translation),
            used_context: pick(col_context, fallback.used_context),
            nuance: pick(col_nuance, fallback.nuance),
            alternatives: pick(col_alternatives, fallback.alternatives),
            anki_card: pick(col_anki, fallback.anki_card),
            natural: pick(col_natural, fallback.natural),
            analysis: pick(col_analysis, fallback.analysis),
            examples: pick(col_examples, fallback.examples),
            raw_text: raw_text.trim().to_string(),
        };

        if let Some(input) = legacy_note_to_card_input(&merged) {
            inputs.push(input);
        }
    }

    let _ = conn.close().await;
    Ok(inputs)
}

async fn import_sqlite_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let sqlite_bytes = match extract_sqlite_bytes(&state, request).await {
        Ok(b) => b,
        Err(err) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid SQLite upload: {err}"))
                .into_response()
        }
    };

    let inputs = match parse_notes_from_sqlite_bytes(&sqlite_bytes).await {
        Ok(i) => i,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("Failed to read notes table from SQLite file: {err}"),
            )
                .into_response()
        }
    };

    match import_card_inputs_dedup(&state.pool, inputs).await {
        Ok(summary) => {
            if wants_html_response(&headers) {
                let msg = format!(
                    "SQLite 가져오기 완료: {}개 추가됨 (중복 제외 {}개)",
                    summary.imported_count, summary.skipped_count
                );
                Html(render_data_page(Some(&msg))).into_response()
            } else {
                (StatusCode::OK, Json(summary)).into_response()
            }
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to import SQLite cards: {err}"),
        )
            .into_response(),
    }
}

async fn export_json_handler(State(state): State<AppState>) -> Response {
    match list_study_cards(&state.pool).await {
        Ok(cards) => {
            let body = serde_json::to_vec_pretty(&cards).unwrap_or_default();
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/json; charset=utf-8"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=\"my_english_cards.json\"",
                    ),
                ],
                body,
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to export JSON: {err}"),
        )
            .into_response(),
    }
}

async fn export_sqlite_handler(State(state): State<AppState>) -> Response {
    match export_sqlite_bytes(&state.pool).await {
        Ok(bytes) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/vnd.sqlite3"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"my_english_backup.db\"",
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create SQLite backup: {err}"),
        )
            .into_response(),
    }
}

async fn export_sqlite_bytes(pool: &SqlitePool) -> Result<Vec<u8>> {
    let tmp_path = unique_temp_path("my_english_backup", "sqlite");
    let _ = tokio::fs::remove_file(&tmp_path).await;
    let escaped_path = tmp_path.to_string_lossy().replace('\'', "''");
    let vacuum_sql = format!("VACUUM INTO '{escaped_path}'");

    if sqlx::query(&vacuum_sql).execute(pool).await.is_ok() {
        if let Ok(bytes) = tokio::fs::read(&tmp_path).await {
            if bytes.starts_with(b"SQLite format 3\0") {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Ok(bytes);
            }
        }
    }

    // Fallback for environments where in-memory SQLite does not permit VACUUM INTO
    let _ = tokio::fs::remove_file(&tmp_path).await;
    let dst_url = format!("sqlite://{}", tmp_path.display());
    let dst_pool = crate::db::init_db(&dst_url).await?;

    let cards = list_study_cards(pool).await?;
    for card in cards {
        let extra_examples_json = serde_json::to_string(&card.extra_examples)?;
        let tags_json = serde_json::to_string(&card.tags)?;
        sqlx::query(
            r#"
            INSERT INTO study_cards (
                id, chunk, surface_chunk, context_sentence, korean_meaning,
                extra_examples_json, nuance_note, tags_json, srs_stage,
                due_date, last_reviewed_at, review_count, lapse_count,
                source_type, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(card.id)
        .bind(&card.chunk)
        .bind(&card.surface_chunk)
        .bind(&card.context_sentence)
        .bind(&card.korean_meaning)
        .bind(&extra_examples_json)
        .bind(&card.nuance_note)
        .bind(&tags_json)
        .bind(card.srs_stage)
        .bind(&card.due_date)
        .bind(&card.last_reviewed_at)
        .bind(card.review_count)
        .bind(card.lapse_count)
        .bind(&card.source_type)
        .bind(&card.created_at)
        .bind(&card.updated_at)
        .execute(&dst_pool)
        .await?;
    }

    // Ensure WAL is checkpointed into the main file before reading bytes
    let _ = sqlx::raw_sql("PRAGMA wal_checkpoint(TRUNCATE);")
        .execute(&dst_pool)
        .await;
    dst_pool.close().await;

    let bytes = tokio::fs::read(&tmp_path).await?;
    let _ = tokio::fs::remove_file(&tmp_path).await;
    let _ = tokio::fs::remove_file(format!("{}-wal", tmp_path.display())).await;
    let _ = tokio::fs::remove_file(format!("{}-shm", tmp_path.display())).await;
    Ok(bytes)
}

async fn export_anki_csv_handler(State(state): State<AppState>) -> Response {
    match list_study_cards(&state.pool).await {
        Ok(cards) => {
            let csv = build_anki_csv(&cards);
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=\"my_english_anki.csv\"",
                    ),
                ],
                csv.into_bytes(),
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to export Anki CSV: {err}"),
        )
            .into_response(),
    }
}

pub fn build_anki_csv(cards: &[StudyCard]) -> String {
    let mut out = String::from("\u{FEFF}");
    for card in cards {
        let target = if !card.surface_chunk.trim().is_empty() {
            card.surface_chunk.trim()
        } else {
            card.chunk.trim()
        };

        let cloze_sentence = make_anki_cloze_sentence(&card.context_sentence, target);
        let front = format!("{cloze_sentence} — {}", card.korean_meaning);

        let examples_str = card
            .extra_examples
            .iter()
            .map(|e| {
                if e.korean.is_empty() {
                    e.english.clone()
                } else {
                    format!("{} ({})", e.english, e.korean)
                }
            })
            .collect::<Vec<_>>()
            .join(" / ");

        let mut back = format!(
            "Chunk: {} ({}) | Sentence: {} | Meaning: {}",
            card.chunk, target, card.context_sentence, card.korean_meaning
        );
        if !card.nuance_note.is_empty() {
            back.push_str(&format!(" | Nuance: {}", card.nuance_note));
        }
        if !examples_str.is_empty() {
            back.push_str(&format!(" | Examples: {examples_str}"));
        }

        let tags = card.tags.join(" ");

        out.push_str(&csv_escape(&front));
        out.push(',');
        out.push_str(&csv_escape(&back));
        out.push(',');
        out.push_str(&csv_escape(&tags));
        out.push('\n');
    }
    out
}

fn make_anki_cloze_sentence(sentence: &str, target: &str) -> String {
    if target.is_empty() {
        return sentence.to_string();
    }
    let lower_sent = sentence.to_lowercase();
    let lower_target = target.to_lowercase();
    if let Some(idx) = lower_sent.find(&lower_target) {
        let end = idx + target.len();
        if let Some(matched) = sentence.get(idx..end) {
            return format!(
                "{}{{{{c1::{matched}}}}} [ _______________ ]{}",
                &sentence[..idx],
                &sentence[end..]
            );
        }
    }
    format!("{sentence} ({{{{c1::{target}}}}})")
}

fn csv_escape(field: &str) -> String {
    let cleaned = field.replace('\r', " ").replace('\n', " ");
    let escaped = cleaned.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

async fn data_page_handler() -> Response {
    Html(render_data_page(None)).into_response()
}

pub fn render_data_page(notice: Option<&str>) -> String {
    let notice_html = match notice {
        Some(msg) => format!(
            r#"<div class="panel" id="import-notice" role="status"><strong>{}</strong></div>"#,
            escape_html(msg)
        ),
        None => String::new(),
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>데이터 백업 &amp; 가져오기 — My English</title>
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

    {notice_html}

    <section class="panel">
      <h1>Legacy English_note 가져오기 (Import)</h1>
      <form method="POST" action="/import/text">
        <label for="legacy-text">Gemini / English_note 포맷 텍스트 붙여넣기 ([원문], [올바른 해석], [30년 차 강사의 실전 Anki 카드 정리] 등)</label>
        <textarea id="legacy-text" name="text" rows="8" required placeholder="[원문]&#10;I'm under the weather.&#10;&#10;[올바른 해석]&#10;몸 상태가 좀 안 좋아."></textarea>
        <button type="submit" class="btn btn-primary">텍스트에서 Study Card 가져오기</button>
      </form>
    </section>

    <section class="panel">
      <h2>기존 notes.db SQLite 파일 업로드</h2>
      <form method="POST" action="/import/sqlite" enctype="multipart/form-data">
        <label for="sqlite-file">English_note notes.db 파일 선택</label>
        <input type="file" id="sqlite-file" name="file" accept=".db,.sqlite,.sqlite3" required>
        <button type="submit" class="btn btn-primary">SQLite DB에서 Study Card 가져오기</button>
      </form>
    </section>

    <section class="panel">
      <h2>원클릭 백업 및 내보내기 (One-Click Export)</h2>
      <div class="nav-links" style="margin-top: 0.75rem;">
        <a href="/export/sqlite" class="btn btn-primary" download>SQLite DB 백업 다운로드 (/export/sqlite)</a>
        <a href="/export/json" class="btn" download>JSON 내보내기 (/export/json)</a>
        <a href="/export/anki.csv" class="btn" download>Anki CSV 내보내기 (/export/anki.csv)</a>
      </div>
    </section>
  </div>
</body>
</html>"#
    )
}
