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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRating {
    Again,
    Good,
    Easy,
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
}

