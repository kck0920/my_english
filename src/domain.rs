use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtraExample {
    pub english: String,
    pub korean: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StudyCard {
    pub id: i64,
    pub chunk: String,
    pub surface_chunk: String,
    pub context_sentence: String,
    pub korean_meaning: String,
    pub extra_examples: Vec<ExtraExample>,
    pub nuance_note: String,
    pub tags: Vec<String>,
    pub srs_stage: i64,
    pub due_date: String,
    pub last_reviewed_at: Option<String>,
    pub review_count: i64,
    pub lapse_count: i64,
    pub source_type: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateStudyCardInput {
    pub chunk: String,
    #[serde(default)]
    pub surface_chunk: String,
    pub context_sentence: String,
    pub korean_meaning: String,
    #[serde(default)]
    pub extra_examples: Vec<ExtraExample>,
    #[serde(default)]
    pub nuance_note: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateStudyCardInput {
    pub chunk: String,
    #[serde(default)]
    pub surface_chunk: String,
    pub context_sentence: String,
    pub korean_meaning: String,
    #[serde(default)]
    pub extra_examples: Vec<ExtraExample>,
    #[serde(default)]
    pub nuance_note: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub srs_stage: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CardFilter {
    pub q: Option<String>,
    pub tag: Option<String>,
    pub srs_stage: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DraftCard {
    pub chunk: String,
    pub surface_chunk: String,
    pub context_sentence: String,
    pub korean_meaning: String,
    #[serde(default)]
    pub extra_examples: Vec<ExtraExample>,
    #[serde(default)]
    pub nuance_note: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputTransferPrompt {
    pub korean_prompt: String,
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputTransferEvaluation {
    pub passed: bool,
    pub corrected_sentence: String,
    pub feedback: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnswerMatch {
    ExactSurface,
    CanonicalBase,
    Mismatch,
}

impl AnswerMatch {
    pub fn is_correct(self) -> bool {
        matches!(self, Self::ExactSurface | Self::CanonicalBase)
    }
}

/// Normalizes case, whitespace, punctuation, and contractions, comparing `user_input`
/// against `surface_chunk` (`ExactSurface`) and `chunk` (`CanonicalBase`).
pub fn check_review_answer(user_input: &str, chunk: &str, surface_chunk: &str) -> AnswerMatch {
    let input_norm = normalize_answer_tokens(user_input, false);
    if input_norm.is_empty() {
        return AnswerMatch::Mismatch;
    }
    let input_expanded = normalize_answer_tokens(user_input, true);

    let surface_norm = normalize_answer_tokens(surface_chunk, false);
    if !surface_norm.is_empty() {
        let surface_expanded = normalize_answer_tokens(surface_chunk, true);
        if input_norm == surface_norm || input_expanded == surface_expanded {
            return AnswerMatch::ExactSurface;
        }
    }

    let chunk_norm = normalize_answer_tokens(chunk, false);
    if !chunk_norm.is_empty() {
        let chunk_expanded = normalize_answer_tokens(chunk, true);
        if input_norm == chunk_norm || input_expanded == chunk_expanded {
            return AnswerMatch::CanonicalBase;
        }
    }

    AnswerMatch::Mismatch
}

fn normalize_answer_tokens(raw: &str, expand_contractions: bool) -> String {
    let unified = raw.replace(['’', '‘', '`'], "'");
    let mut cleaned = String::with_capacity(unified.len());
    for ch in unified.chars() {
        if ch.is_alphanumeric() || ch == '\'' || ch.is_whitespace() {
            cleaned.push(ch.to_ascii_lowercase());
        } else {
            cleaned.push(' ');
        }
    }

    let mut tokens: Vec<String> = Vec::new();
    for raw_tok in cleaned.split_whitespace() {
        let tok = raw_tok.trim_matches('\'');
        if tok.is_empty() {
            continue;
        }
        if expand_contractions {
            match tok {
                "i'm" => {
                    tokens.push("i".to_string());
                    tokens.push("am".to_string());
                }
                "you're" => {
                    tokens.push("you".to_string());
                    tokens.push("are".to_string());
                }
                "we're" => {
                    tokens.push("we".to_string());
                    tokens.push("are".to_string());
                }
                "they're" => {
                    tokens.push("they".to_string());
                    tokens.push("are".to_string());
                }
                "he's" => {
                    tokens.push("he".to_string());
                    tokens.push("is".to_string());
                }
                "she's" => {
                    tokens.push("she".to_string());
                    tokens.push("is".to_string());
                }
                "it's" => {
                    tokens.push("it".to_string());
                    tokens.push("is".to_string());
                }
                "let's" => {
                    tokens.push("let".to_string());
                    tokens.push("us".to_string());
                }
                "don't" => {
                    tokens.push("do".to_string());
                    tokens.push("not".to_string());
                }
                "doesn't" => {
                    tokens.push("does".to_string());
                    tokens.push("not".to_string());
                }
                "didn't" => {
                    tokens.push("did".to_string());
                    tokens.push("not".to_string());
                }
                "can't" | "cannot" => {
                    tokens.push("can".to_string());
                    tokens.push("not".to_string());
                }
                "won't" => {
                    tokens.push("will".to_string());
                    tokens.push("not".to_string());
                }
                "isn't" => {
                    tokens.push("is".to_string());
                    tokens.push("not".to_string());
                }
                "aren't" => {
                    tokens.push("are".to_string());
                    tokens.push("not".to_string());
                }
                "wasn't" => {
                    tokens.push("was".to_string());
                    tokens.push("not".to_string());
                }
                "weren't" => {
                    tokens.push("were".to_string());
                    tokens.push("not".to_string());
                }
                "haven't" => {
                    tokens.push("have".to_string());
                    tokens.push("not".to_string());
                }
                "hasn't" => {
                    tokens.push("has".to_string());
                    tokens.push("not".to_string());
                }
                "hadn't" => {
                    tokens.push("had".to_string());
                    tokens.push("not".to_string());
                }
                "wouldn't" => {
                    tokens.push("would".to_string());
                    tokens.push("not".to_string());
                }
                "couldn't" => {
                    tokens.push("could".to_string());
                    tokens.push("not".to_string());
                }
                "shouldn't" => {
                    tokens.push("should".to_string());
                    tokens.push("not".to_string());
                }
                _ => tokens.push(tok.to_string()),
            }
        } else {
            tokens.push(tok.to_string());
        }
    }

    tokens.join(" ")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRating {
    Again,
    Good,
    Easy,
}

impl ReviewRating {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Again => "again",
            Self::Good => "good",
            Self::Easy => "easy",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "again" => Some(Self::Again),
            "good" => Some(Self::Good),
            "easy" => Some(Self::Easy),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SrsTransition {
    pub new_stage: i64,
    pub interval_days: i64,
    pub due_date: String,
    pub last_reviewed_at: String,
    pub review_count: i64,
    pub lapse_count: i64,
}

pub fn stage_interval_days(stage: i64) -> i64 {
    match stage.clamp(0, 5) {
        0 => 0,
        1 => 1,
        2 => 3,
        3 => 7,
        4 => 14,
        _ => 30,
    }
}

pub fn compute_srs_transition(
    current_stage: i64,
    current_review_count: i64,
    current_lapse_count: i64,
    rating: ReviewRating,
    now: chrono::DateTime<chrono::Utc>,
) -> SrsTransition {
    let clamped_stage = current_stage.clamp(0, 5);
    let now_iso = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let review_count = current_review_count.max(0) + 1;

    match rating {
        ReviewRating::Again => SrsTransition {
            new_stage: 1,
            interval_days: 0,
            due_date: now_iso.clone(),
            last_reviewed_at: now_iso,
            review_count,
            lapse_count: current_lapse_count.max(0) + 1,
        },
        ReviewRating::Good => {
            let new_stage = (clamped_stage + 1).min(5);
            let interval_days = stage_interval_days(new_stage);
            let due_date = (now + chrono::Duration::days(interval_days))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
            SrsTransition {
                new_stage,
                interval_days,
                due_date,
                last_reviewed_at: now_iso,
                review_count,
                lapse_count: current_lapse_count.max(0),
            }
        }
        ReviewRating::Easy => {
            let new_stage = (clamped_stage + 2).min(5);
            let interval_days = stage_interval_days(new_stage);
            let due_date = (now + chrono::Duration::days(interval_days))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
            SrsTransition {
                new_stage,
                interval_days,
                due_date,
                last_reviewed_at: now_iso,
                review_count,
                lapse_count: current_lapse_count.max(0),
            }
        }
    }
}

pub const CLOZE_BLANK: &str = "[ _______________ ]";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClozePrompt {
    pub masked_sentence: String,
    pub target_surface: String,
    pub word_count: usize,
}

/// Replaces `surface_chunk` (case-insensitive, or `chunk` fallback) inside `context_sentence`
/// with `[ _______________ ]` and computes the word count hint.
pub fn mask_context_sentence(
    context_sentence: &str,
    surface_chunk: &str,
    chunk: &str,
) -> ClozePrompt {
    let sentence_lower = context_sentence.to_lowercase();
    let surface_trimmed = surface_chunk.trim();

    let (match_span, target_surface) = if !surface_trimmed.is_empty() {
        let surface_lower = surface_trimmed.to_lowercase();
        if let Some(idx) = sentence_lower.find(&surface_lower) {
            let end = idx + surface_lower.len();
            let actual = context_sentence
                .get(idx..end)
                .unwrap_or(surface_trimmed)
                .to_string();
            (Some((idx, end)), actual)
        } else {
            let detected = detect_surface_chunk(chunk, None, context_sentence);
            let detected_lower = detected.to_lowercase();
            if let Some(idx) = sentence_lower.find(&detected_lower) {
                let end = idx + detected_lower.len();
                let actual = context_sentence
                    .get(idx..end)
                    .unwrap_or(&detected)
                    .to_string();
                (Some((idx, end)), actual)
            } else {
                (None, surface_trimmed.to_string())
            }
        }
    } else {
        let detected = detect_surface_chunk(chunk, None, context_sentence);
        let detected_lower = detected.to_lowercase();
        if !detected_lower.is_empty() {
            if let Some(idx) = sentence_lower.find(&detected_lower) {
                let end = idx + detected_lower.len();
                let actual = context_sentence
                    .get(idx..end)
                    .unwrap_or(&detected)
                    .to_string();
                (Some((idx, end)), actual)
            } else {
                (None, detected)
            }
        } else {
            (None, chunk.trim().to_string())
        }
    };

    let masked_sentence = if let Some((start, end)) = match_span {
        let mut out = String::with_capacity(context_sentence.len() + CLOZE_BLANK.len());
        out.push_str(&context_sentence[..start]);
        out.push_str(CLOZE_BLANK);
        out.push_str(&context_sentence[end..]);
        out
    } else {
        format!("{context_sentence} {CLOZE_BLANK}")
    };

    let word_source = if !target_surface.trim().is_empty() {
        target_surface.as_str()
    } else {
        chunk.trim()
    };
    let word_count = word_source.split_whitespace().count();

    ClozePrompt {
        masked_sentence,
        target_surface,
        word_count,
    }
}

/// Automatically detects the inflected or exact surface form of `chunk` inside `context_sentence`
/// when `surface_chunk_input` is omitted or empty.
pub fn detect_surface_chunk(
    chunk: &str,
    surface_chunk_input: Option<&str>,
    context_sentence: &str,
) -> String {
    let chunk_trimmed = chunk.trim();
    if let Some(explicit) = surface_chunk_input {
        let explicit_trimmed = explicit.trim();
        if !explicit_trimmed.is_empty() {
            return explicit_trimmed.to_string();
        }
    }
    if chunk_trimmed.is_empty() {
        return String::new();
    }

    // 1. Exact case-insensitive substring match in context_sentence
    let sentence_lower = context_sentence.to_lowercase();
    let chunk_lower = chunk_trimmed.to_lowercase();
    if let Some(byte_idx) = sentence_lower.find(&chunk_lower) {
        if let Some(slice) = context_sentence.get(byte_idx..byte_idx + chunk_trimmed.len()) {
            return slice.to_string();
        }
    }

    // 2. Token-based inflection / pronoun match (e.g., "get me every time" -> "gets me every time",
    //    "be supposed to" -> "am supposed to", "take one's time" -> "take your time")
    let chunk_tokens: Vec<&str> = chunk_trimmed.split_whitespace().collect();
    if chunk_tokens.len() >= 2 {
        let sentence_words = tokenize_with_spans(context_sentence);
        let n = chunk_tokens.len();
        if sentence_words.len() >= n {
            for window in sentence_words.windows(n) {
                let matches_all = window
                    .iter()
                    .zip(chunk_tokens.iter())
                    .all(|((_, w_clean), &c_tok)| tokens_match_inflected(c_tok, w_clean));
                if matches_all {
                    let start = window.first().unwrap().0.0;
                    let end = window.last().unwrap().0.1;
                    if let Some(slice) = context_sentence.get(start..end) {
                        return slice.to_string();
                    }
                }
            }
        }
    } else if chunk_tokens.len() == 1 {
        let sentence_words = tokenize_with_spans(context_sentence);
        for ((start, end), w_clean) in sentence_words {
            if tokens_match_inflected(chunk_tokens[0], &w_clean) {
                if let Some(slice) = context_sentence.get(start..end) {
                    return slice.to_string();
                }
            }
        }
    }

    chunk_trimmed.to_string()
}

fn tokenize_with_spans(text: &str) -> Vec<((usize, usize), String)> {
    let mut result = Vec::new();
    let mut start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '-' {
            if start.is_none() {
                start = Some(idx);
            }
        } else if let Some(s) = start.take() {
            let slice = &text[s..idx];
            result.push(((s, idx), slice.to_lowercase()));
        }
    }
    if let Some(s) = start {
        let slice = &text[s..];
        result.push(((s, text.len()), slice.to_lowercase()));
    }
    result
}

fn tokens_match_inflected(canonical: &str, surface: &str) -> bool {
    let c = canonical
        .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '\'')
        .to_lowercase();
    let s = surface
        .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '\'')
        .to_lowercase();
    if c == s {
        return true;
    }
    // Be-verb variations
    if c == "be"
        && matches!(
            s.as_str(),
            "am" | "is" | "are" | "was" | "were" | "been" | "being" | "i'm" | "you're" | "he's"
                | "she's" | "it's" | "we're" | "they're"
        )
    {
        return true;
    }
    // Possessive pronoun placeholder variations
    if matches!(c.as_str(), "one's" | "someone's" | "somebody's")
        && matches!(
            s.as_str(),
            "my" | "your" | "his" | "her" | "its" | "our" | "their" | "one's"
        )
    {
        return true;
    }
    // Object pronoun placeholder variations
    if matches!(c.as_str(), "someone" | "somebody")
        && matches!(
            s.as_str(),
            "me" | "you" | "him" | "her" | "us" | "them" | "someone" | "somebody"
        )
    {
        return true;
    }
    // Common irregular verbs
    let irregulars: &[(&str, &[&str])] = &[
        ("get", &["gets", "got", "gotten", "getting"]),
        ("have", &["has", "had", "having"]),
        ("do", &["does", "did", "done", "doing"]),
        ("go", &["goes", "went", "gone", "going"]),
        ("take", &["takes", "took", "taken", "taking"]),
        ("make", &["makes", "made", "making"]),
        ("come", &["comes", "came", "coming"]),
        ("give", &["gives", "gave", "given", "giving"]),
        ("keep", &["keeps", "kept", "keeping"]),
        ("put", &["puts", "putting"]),
        ("run", &["runs", "ran", "running"]),
        ("bring", &["brings", "brought", "bringing"]),
        ("find", &["finds", "found", "finding"]),
        ("catch", &["catches", "caught", "catching"]),
        ("fall", &["falls", "fell", "fallen", "falling"]),
        ("hold", &["holds", "held", "holding"]),
        ("leave", &["leaves", "left", "leaving"]),
        ("feel", &["feels", "felt", "feeling"]),
        ("break", &["breaks", "broke", "broken", "breaking"]),
    ];
    for &(base, forms) in irregulars {
        if c == base && forms.contains(&s.as_str()) {
            return true;
        }
    }
    // Regular verb inflections (s, es, ed, d, ing, ies/ied)
    if c.len() >= 2 {
        if s == format!("{c}s")
            || s == format!("{c}es")
            || s == format!("{c}ed")
            || s == format!("{c}d")
            || s == format!("{c}ing")
        {
            return true;
        }
        if let Some(stem) = c.strip_suffix('e') {
            if s == format!("{stem}ing") {
                return true;
            }
        }
        if let Some(stem) = c.strip_suffix('y') {
            if s == format!("{stem}ies") || s == format!("{stem}ied") {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_surface_chunk_handles_exact_and_inflected_forms() {
        // Explicit override takes precedence
        assert_eq!(
            detect_surface_chunk(
                "get me every time",
                Some("got me every time"),
                "That joke got me every time."
            ),
            "got me every time"
        );

        // Exact case-insensitive substring preserves sentence casing
        assert_eq!(
            detect_surface_chunk(
                "at the end of the day",
                None,
                "At the end of the day, results speak louder."
            ),
            "At the end of the day"
        );

        // Inflected verb + pronoun placeholder
        assert_eq!(
            detect_surface_chunk(
                "take one's time",
                Some(""),
                "Please take your time with the review."
            ),
            "take your time"
        );

        // Be-verb inflection
        assert_eq!(
            detect_surface_chunk(
                "be supposed to",
                None,
                "We are supposed to submit it by Friday."
            ),
            "are supposed to"
        );
    }

    #[test]
    fn mask_context_sentence_replaces_surface_chunk_and_computes_word_count() {
        let prompt = mask_context_sentence(
            "That final scene gets me every time.",
            "gets me every time",
            "get me every time",
        );
        assert_eq!(
            prompt.masked_sentence,
            "That final scene [ _______________ ]."
        );
        assert_eq!(prompt.word_count, 4);
        assert_eq!(prompt.target_surface, "gets me every time");

        // Fallback to chunk detection when surface_chunk is empty or different case
        let fallback_prompt = mask_context_sentence(
            "At the end of the day, consistency wins.",
            "",
            "at the end of the day",
        );
        assert_eq!(
            fallback_prompt.masked_sentence,
            "[ _______________ ], consistency wins."
        );
        assert_eq!(fallback_prompt.word_count, 6);
        assert_eq!(fallback_prompt.target_surface, "At the end of the day");
    }

    #[test]
    fn check_review_answer_distinguishes_exact_surface_canonical_base_and_mismatch() {
        // Exact surface match (with case, extra spaces, and trailing punctuation differences)
        assert_eq!(
            check_review_answer(
                "  Gets   me every time! ",
                "get me every time",
                "gets me every time"
            ),
            AnswerMatch::ExactSurface
        );

        // Canonical base chunk match when surface chunk is inflected
        assert_eq!(
            check_review_answer(
                "get me every time.",
                "get me every time",
                "gets me every time"
            ),
            AnswerMatch::CanonicalBase
        );

        // Contractions and apostrophe variations
        assert_eq!(
            check_review_answer(
                "we're supposed to",
                "be supposed to",
                "we are supposed to"
            ),
            AnswerMatch::ExactSurface
        );

        // Mismatch for wrong expression or empty input
        assert_eq!(
            check_review_answer("get me all the time", "get me every time", "gets me every time"),
            AnswerMatch::Mismatch
        );
        assert_eq!(
            check_review_answer("   ", "get me every time", "gets me every time"),
            AnswerMatch::Mismatch
        );
    }

    #[test]
    fn compute_srs_transition_applies_fixed_stage_schedule_for_again_good_and_easy() {
        use chrono::TimeZone;
        let now = chrono::Utc.with_ymd_and_hms(2026, 10, 21, 9, 0, 0).unwrap();

        // Stage 0 -> Good -> Stage 1 (+1d)
        let t1 = compute_srs_transition(0, 0, 0, ReviewRating::Good, now);
        assert_eq!(t1.new_stage, 1);
        assert_eq!(t1.interval_days, 1);
        assert_eq!(t1.due_date, "2026-10-22T09:00:00Z");
        assert_eq!(t1.review_count, 1);
        assert_eq!(t1.lapse_count, 0);

        // Stage 1 -> Good -> Stage 2 (+3d)
        let t2 = compute_srs_transition(1, 1, 0, ReviewRating::Good, now);
        assert_eq!(t2.new_stage, 2);
        assert_eq!(t2.interval_days, 3);
        assert_eq!(t2.due_date, "2026-10-24T09:00:00Z");

        // Stage 2 -> Easy -> Stage 4 (+14d, jumping 2 stages)
        let t3 = compute_srs_transition(2, 2, 0, ReviewRating::Easy, now);
        assert_eq!(t3.new_stage, 4);
        assert_eq!(t3.interval_days, 14);
        assert_eq!(t3.due_date, "2026-11-04T09:00:00Z");

        // Stage 4 -> Easy -> capped at Stage 5 (+30d)
        let t4 = compute_srs_transition(4, 3, 0, ReviewRating::Easy, now);
        assert_eq!(t4.new_stage, 5);
        assert_eq!(t4.interval_days, 30);
        assert_eq!(t4.due_date, "2026-11-20T09:00:00Z");

        // Stage 4 -> Again -> resets to Stage 1, due immediately (now), increments lapse_count
        let t5 = compute_srs_transition(4, 4, 1, ReviewRating::Again, now);
        assert_eq!(t5.new_stage, 1);
        assert_eq!(t5.interval_days, 0);
        assert_eq!(t5.due_date, "2026-10-21T09:00:00Z");
        assert_eq!(t5.review_count, 5);
        assert_eq!(t5.lapse_count, 2);
    }
}

