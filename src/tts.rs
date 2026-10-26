use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::cards::list_study_cards;
use crate::domain::StudyCard;
use crate::state::AppState;
use crate::ui::{escape_html, render_navbar, APP_CSS, HEAD_META, ICON_AUDIO};

/// Global in-memory bounded cache for synthesized TTS MP3 audio bytes.
/// Key format: "{lang}:{gender}:{trimmed_text}"
static TTS_BYTE_CACHE: LazyLock<RwLock<HashMap<String, Vec<u8>>>> = LazyLock::new(|| {
    RwLock::new(HashMap::with_capacity(100))
});

#[async_trait]
pub trait TtsProvider: Send + Sync {
    async fn synthesize(&self, text: &str, lang: &str, gender: &str) -> Result<Vec<u8>>;
}

#[derive(Debug, Default, Clone)]
pub struct NoopTtsProvider;

#[async_trait]
impl TtsProvider for NoopTtsProvider {
    async fn synthesize(&self, _text: &str, _lang: &str, _gender: &str) -> Result<Vec<u8>> {
        Ok(vec![])
    }
}

/// Production HTTP TTS provider with Amazon Polly Joanna/Matthew + Google Translate fallback.
#[derive(Debug, Clone)]
pub struct HttpTtsProvider {
    client: reqwest::Client,
}

impl Default for HttpTtsProvider {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(12))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl HttpTtsProvider {
    pub fn new() -> Self {
        Self::default()
    }

    async fn fetch_amazon_polly(&self, text: &str, gender: &str) -> Result<Vec<u8>> {
        let voice = if gender.eq_ignore_ascii_case("male") {
            "Matthew"
        } else {
            "Joanna"
        };
        let res = self
            .client
            .post("https://ttsmp3.com/makemp3_new.php")
            .header("Referer", "https://ttsmp3.com/")
            .header("Origin", "https://ttsmp3.com")
            .form(&[
                ("msg", text),
                ("lang", voice),
                ("source", "ttsmp3"),
            ])
            .send()
            .await?;

        if !res.status().is_success() {
            return Err(anyhow!("ttsmp3 HTTP status: {}", res.status()));
        }

        #[derive(Deserialize)]
        struct TtsMp3Response {
            #[serde(rename = "URL")]
            url: Option<String>,
            #[serde(rename = "Error")]
            error: Option<serde_json::Value>,
        }

        let body: TtsMp3Response = res.json().await?;
        if let Some(err) = body.error {
            if !err.is_null() && err != 0 {
                return Err(anyhow!("ttsmp3 returned error: {:?}", err));
            }
        }

        let url = body.url.ok_or_else(|| anyhow!("missing MP3 URL in ttsmp3 response"))?;
        let mp3_res = self.client.get(&url).send().await?;
        if !mp3_res.status().is_success() {
            return Err(anyhow!("failed to fetch MP3 from ttsmp3 URL"));
        }
        let bytes = mp3_res.bytes().await?;
        Ok(bytes.to_vec())
    }

    async fn fetch_google_tts_chunk(&self, chunk: &str, lang: &str) -> Result<Vec<u8>> {
        let url = format!(
            "https://translate.google.com/translate_tts?ie=UTF-8&q={}&tl={}&client=tw-ob",
            urlencoding::encode(chunk),
            lang
        );
        let res = self
            .client
            .get(&url)
            .header("Referer", "https://translate.google.com/")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(anyhow!("google tts status: {}", res.status()));
        }
        let bytes = res.bytes().await?;
        Ok(bytes.to_vec())
    }

    async fn fetch_google_tts(&self, text: &str, lang: &str) -> Result<Vec<u8>> {
        let chunks = split_text_for_tts(text, 150);
        let mut combined = Vec::new();
        for chunk in chunks {
            let chunk_bytes = self.fetch_google_tts_chunk(&chunk, lang).await?;
            combined.extend_from_slice(&chunk_bytes);
        }
        Ok(combined)
    }
}

#[async_trait]
impl TtsProvider for HttpTtsProvider {
    async fn synthesize(&self, text: &str, lang: &str, gender: &str) -> Result<Vec<u8>> {
        if lang.eq_ignore_ascii_case("en") {
            // 1st choice: Amazon Polly (최대 3초 대기, 지연 시 즉시 Google TTS로 폴백)
            let polly_result = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                self.fetch_amazon_polly(text, gender),
            )
            .await;

            match polly_result {
                Ok(Ok(bytes)) if !bytes.is_empty() => return Ok(bytes),
                Ok(Err(err)) => {
                    tracing::warn!("Amazon Polly failed, falling back to Google TTS: {:#}", err);
                }
                Err(_) => {
                    tracing::warn!("Amazon Polly timed out (3s), falling back to Google TTS immediately");
                }
                _ => {}
            }
            // 2nd choice: Google Translate TTS fallback (초고속 안정성)
            self.fetch_google_tts(text, "en").await
        } else {
            // Korean and other languages: Google Translate TTS
            self.fetch_google_tts(text, lang).await
        }
    }
}

fn split_text_for_tts(text: &str, max_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.len() + 1 + word.len() > max_len {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() && !text.trim().is_empty() {
        chunks.push(text.trim().to_string());
    }
    chunks
}

#[derive(Debug, Deserialize)]
pub struct TtsQuery {
    pub text: Option<String>,
    #[serde(default = "default_en")]
    pub lang: String,
    #[serde(default = "default_female")]
    pub gender: String,
}

fn default_en() -> String {
    "en".to_string()
}

fn default_female() -> String {
    "female".to_string()
}

pub async fn tts_proxy_handler(
    State(state): State<AppState>,
    Query(query): Query<TtsQuery>,
) -> Response {
    let text = match query.text {
        Some(ref t) if !t.trim().is_empty() => {
            let clean = t.trim();
            if clean.chars().count() > 500 {
                return (
                    StatusCode::BAD_REQUEST,
                    "text parameter exceeds 500 characters limit",
                )
                    .into_response();
            }
            clean
        }
        _ => return (StatusCode::BAD_REQUEST, "text parameter is required").into_response(),
    };

    let lang = query.lang.trim().to_lowercase();
    let gender = query.gender.trim().to_lowercase();
    let cache_key = format!("{}:{}:{}", lang, gender, text);

    if let Ok(guard) = TTS_BYTE_CACHE.read() {
        if let Some(cached_bytes) = guard.get(&cache_key) {
            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "audio/mpeg"),
                    (header::CACHE_CONTROL, "public, max-age=86400"),
                ],
                Bytes::from(cached_bytes.clone()),
            )
                .into_response();
        }
    }

    match state.tts_provider.synthesize(text, &lang, &gender).await {
        Ok(bytes) => {
            if !bytes.is_empty() {
                if let Ok(mut guard) = TTS_BYTE_CACHE.write() {
                    if guard.len() >= 2000 {
                        guard.clear();
                    }
                    guard.insert(cache_key, bytes.clone());
                }
            }
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "audio/mpeg"),
                    (header::ACCEPT_RANGES, "bytes"),
                    (header::CACHE_CONTROL, "public, max-age=86400"),
                ],
                Bytes::from(bytes),
            )
                .into_response()
        }
        Err(err) => {
            tracing::error!("TTS synthesis failed: {:#}", err);
            (
                StatusCode::BAD_GATEWAY,
                format!("TTS synthesis failed: {}", err),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AudioGymQuery {
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default = "default_gap")]
    pub gap: u32,
    #[serde(default = "default_female")]
    pub gender: String,
}

fn default_gap() -> u32 {
    3
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AudioGymItem {
    pub id: i64,
    pub chunk: String,
    pub context_sentence: String,
    pub korean_meaning: String,
    pub tags: Vec<String>,
    pub srs_stage: i64,
}

pub async fn audio_gym_playlist_api(
    State(state): State<AppState>,
    Query(query): Query<AudioGymQuery>,
) -> Response {
    let filter = query.filter.as_deref().unwrap_or("due");
    let tag = query.tag.as_deref().unwrap_or("");

    let all_cards = match list_study_cards(&state.pool).await {
        Ok(c) => c,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB query failed: {}", err),
            )
                .into_response();
        }
    };

    let now_str = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let filtered_cards: Vec<StudyCard> = match filter {
        "all" => all_cards,
        "lapsed" => all_cards
            .into_iter()
            .filter(|c| c.lapse_count > 0 || c.srs_stage <= 1)
            .collect(),
        _ => all_cards
            .into_iter()
            .filter(|c| c.due_date.as_str() <= now_str.as_str() || c.review_count == 0)
            .collect(),
    };

    let items: Vec<AudioGymItem> = filtered_cards
        .into_iter()
        .filter(|c| {
            if tag.is_empty() {
                true
            } else {
                c.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
            }
        })
        .map(|c| AudioGymItem {
            id: c.id,
            chunk: c.chunk,
            context_sentence: c.context_sentence,
            korean_meaning: c.korean_meaning,
            tags: c.tags,
            srs_stage: c.srs_stage,
        })
        .collect();

    Json(items).into_response()
}

pub async fn audio_gym_page(
    State(state): State<AppState>,
    Query(query): Query<AudioGymQuery>,
) -> Response {
    let all_cards = list_study_cards(&state.pool).await.unwrap_or_default();
    let all_count = all_cards.len();
    let now_str = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let due_count = all_cards
        .iter()
        .filter(|c| c.due_date.as_str() <= now_str.as_str() || c.review_count == 0)
        .count();
    let lapsed_count = all_cards
        .iter()
        .filter(|c| c.lapse_count > 0 || c.srs_stage <= 1)
        .count();

    let default_filter = if due_count > 0 { "due" } else { "all" };
    let filter = query.filter.as_deref().unwrap_or(default_filter);
    let tag = query.tag.as_deref().unwrap_or("");
    let gap = query.gap.clamp(1, 15);
    let gender = if query.gender.eq_ignore_ascii_case("male") {
        "male"
    } else {
        "female"
    };

    let navbar = render_navbar("gym", Some(due_count as i64));

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
    {HEAD_META}
    <title>Audio Gym - My English</title>
    <style>
        {css}
        .gym-header {{ display: flex; justify-content: space-between; align-items: center; margin-bottom: 1.5rem; }}
        .gym-stage {{ background: var(--surface-card); border-radius: var(--radius); padding: 2.25rem 1.5rem; text-align: center; box-shadow: var(--shadow-lg); margin-bottom: 1.5rem; border: 1px solid var(--border); transition: border-color 0.2s; }}
        .gym-step-indicator {{ font-size: 0.875rem; font-weight: 700; color: var(--primary-light); text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.75rem; display: flex; align-items: center; justify-content: center; gap: 0.4rem; }}
        .gym-korean {{ font-size: 1.25rem; font-weight: 600; color: var(--text); margin-bottom: 1rem; min-height: 2rem; }}
        .gym-chunk {{ font-size: 1.6rem; font-weight: 700; color: var(--primary-light); margin-bottom: 0.5rem; min-height: 2.25rem; letter-spacing: -0.01em; }}
        .gym-sentence {{ font-size: 1.15rem; color: #cbd5e1; line-height: 1.6; min-height: 3rem; }}
        .gym-timer {{ font-size: 3.75rem; font-weight: 800; color: var(--accent); margin: 1rem 0; font-variant-numeric: tabular-nums; text-shadow: 0 0 20px rgba(16, 185, 129, 0.4); }}
        .gym-controls {{ display: flex; gap: 0.5rem; justify-content: center; align-items: center; margin-bottom: 1.5rem; width: 100%; max-width: 440px; margin-left: auto; margin-right: auto; }}
        .gym-btn {{ white-space: nowrap; border-radius: var(--radius-sm); font-size: 0.95rem; font-weight: 700; cursor: pointer; border: 1px solid var(--border); transition: all 0.18s; text-align: center; box-sizing: border-box; }}
        .gym-btn-primary {{ background: linear-gradient(135deg, #2563eb, #1d4ed8); border-color: #3b82f6; color: white; box-shadow: 0 4px 14px rgba(37, 99, 235, 0.4); flex: 1.35; padding: 0.75rem 0.6rem; }}
        .gym-btn-primary:hover {{ background: linear-gradient(135deg, #3b82f6, #2563eb); transform: translateY(-1px); }}
        .gym-btn-secondary {{ background: var(--surface-card); color: var(--text); flex: 1; padding: 0.75rem 0.4rem; }}
        .gym-btn-secondary:hover {{ background: var(--surface-hover); transform: translateY(-1px); }}
        .gym-config {{ display: flex; gap: 0.75rem 1rem; justify-content: center; align-items: center; flex-wrap: wrap; margin-bottom: 1.5rem; font-size: 0.875rem; }}
        .gym-config select, .gym-config input {{ background: var(--surface-card); color: var(--text); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 0.3rem 0.5rem; }}
        .sound-wave {{ display: inline-flex; align-items: center; gap: 2px; height: 14px; vertical-align: middle; }}
        .sound-wave span {{ display: block; width: 3px; height: 100%; background: currentColor; border-radius: 2px; animation: gym-wave 0.9s infinite ease-in-out; }}
        .sound-wave span:nth-child(1) {{ animation-delay: -0.4s; }}
        .sound-wave span:nth-child(2) {{ animation-delay: -0.2s; }}
        .sound-wave span:nth-child(3) {{ animation-delay: 0s; }}
        @keyframes gym-wave {{ 0%, 40%, 100% {{ transform: scaleY(0.25); }} 20% {{ transform: scaleY(1); }} }}
        @media (max-width: 600px) {{
            .gym-stage {{ padding: 1.5rem 1rem; margin-bottom: 1rem; }}
            .gym-korean {{ font-size: 1.1rem; min-height: 1.6rem; margin-bottom: 0.75rem; }}
            .gym-chunk {{ font-size: 1.35rem; min-height: 1.8rem; word-break: break-word; }}
            .gym-sentence {{ font-size: 1rem; min-height: 2.4rem; word-break: break-word; }}
            .gym-timer {{ font-size: 2.8rem; margin: 0.6rem 0; }}
            .gym-config {{ gap: 0.5rem 0.6rem; font-size: 0.825rem; margin-bottom: 1rem; }}
            .gym-config select, .gym-config input {{ padding: 0.25rem 0.4rem; font-size: 0.825rem; }}
            .gym-btn {{ font-size: 0.875rem; }}
        }}
    </style>
</head>
<body>
    {navbar}
    <div class="container">
        <div class="gym-header">
            <div>
                <h1 style="margin: 0; font-size: 1.5rem; display: flex; align-items: center; gap: 0.5rem;">{ICON_AUDIO} <span>Audio Gym</span></h1>
            </div>
            <div style="font-size: 0.875rem; color: var(--text-muted);">
                오늘 복습 대상: <strong>{due_count}</strong>개 (전체: {all_count}개)
            </div>
        </div>

        <div class="gym-config">
            <div>
                <label>재생 필터: </label>
                <select id="filterSelect" onchange="applyFilters()">
                    <option value="due" {sel_due}>오늘 복습 카드 ({due_count})</option>
                    <option value="lapsed" {sel_lapsed}>자주 틀린 카드 ({lapsed_count})</option>
                    <option value="all" {sel_all}>전체 카드 ({all_count})</option>
                </select>
            </div>
            <div>
                <label>말하기 대기(초): </label>
                <input type="number" id="gapInput" value="{gap}" min="1" max="15" style="width: 50px;" onchange="applyFilters()">
            </div>
            <div>
                <label>목소리: </label>
                <select id="genderSelect" onchange="applyFilters()">
                    <option value="female" {sel_female}>Joanna (여성)</option>
                    <option value="male" {sel_male}>Matthew (남성)</option>
                </select>
            </div>
        </div>

        <div class="gym-stage">
            <div id="stepIndicator" class="gym-step-indicator">대기 중</div>
            <div id="koreanText" class="gym-korean">재생 버튼을 누르면 시작합니다</div>
            <div id="timerDisplay" class="gym-timer" style="display: none;">3</div>
            <div id="chunkText" class="gym-chunk" style="display: none;"></div>
            <div id="sentenceText" class="gym-sentence" style="display: none;"></div>
        </div>

        <div class="gym-controls">
            <button class="gym-btn gym-btn-secondary" onclick="prevCard()">⏮ 이전</button>
            <button id="playBtn" class="gym-btn gym-btn-primary" onclick="togglePlay()">▶ 시작하기</button>
            <button class="gym-btn gym-btn-secondary" onclick="nextCard()">다음 ⏭</button>
        </div>

        <div id="progressText" style="text-align: center; font-size: 0.875rem; color: var(--text-muted);">
            준비 중...
        </div>
    </div>

    <audio id="ttsAudio" playsinline preload="auto"></audio>

    <script>
        let playlist = [];
        let currentIndex = 0;
        let isPlaying = false;
        let currentStepRunId = 0;
        let currentSpeechToken = 0;
        let gapSeconds = {gap};
        let voiceGender = '{gender}';
        let currentStep = 'idle';
        let stepTimer = null;
        let wakeLock = null;
        let speechWatchdogTimer = null;
        let sharedAudio = null;

        const audioBlobCache = new Map();
        const audioPrefetchInFlight = new Map();

        const playBtn = document.getElementById('playBtn');
        const stepIndicator = document.getElementById('stepIndicator');
        const koreanText = document.getElementById('koreanText');
        const timerDisplay = document.getElementById('timerDisplay');
        const chunkText = document.getElementById('chunkText');
        const sentenceText = document.getElementById('sentenceText');
        const progressText = document.getElementById('progressText');

        // 단일 Audio 인스턴스 반환 (iOS Safari / Android 무중단 연속 재생 보장)
        function getSharedAudio() {{
            if (!sharedAudio) {{
                sharedAudio = new Audio();
                sharedAudio.preload = 'auto';
                sharedAudio.preservesPitch = true;
                sharedAudio.playbackRate = 1.0;
                sharedAudio.setAttribute('playsinline', '');
                sharedAudio.setAttribute('webkit-playsinline', '');
            }}
            return sharedAudio;
        }}

        function getAudioCacheKey(text, lang, gender) {{
            return `${{lang}}:${{gender}}:${{text.trim()}}`;
        }}

        // 인메모리 Blob URL 사전 캐싱 (0ms 즉시 재생)
        function prefetchSpeechAudio(text, lang, gender) {{
            if (!text || !text.trim()) return Promise.resolve(null);
            const key = getAudioCacheKey(text, lang, gender);
            if (audioBlobCache.has(key)) {{
                return Promise.resolve(audioBlobCache.get(key));
            }}
            if (audioPrefetchInFlight.has(key)) {{
                return audioPrefetchInFlight.get(key);
            }}

            const url = `/api/tts?text=${{encodeURIComponent(text.trim())}}&lang=${{lang}}&gender=${{gender}}`;
            const promise = fetch(url)
                .then(res => {{
                    if (!res.ok) throw new Error(`HTTP ${{res.status}}`);
                    return res.blob();
                }})
                .then(blob => {{
                    if (!blob || blob.size === 0) throw new Error('Empty audio');
                    const blobUrl = URL.createObjectURL(blob);
                    audioBlobCache.set(key, blobUrl);
                    audioPrefetchInFlight.delete(key);
                    return blobUrl;
                }})
                .catch(() => {{
                    audioPrefetchInFlight.delete(key);
                    return null;
                }});

            audioPrefetchInFlight.set(key, promise);
            return promise;
        }}

        // 브라우저 보이스 목록 사전 로드
        if ('speechSynthesis' in window) {{
            window.speechSynthesis.onvoiceschanged = () => {{
                try {{ window.speechSynthesis.getVoices(); }} catch(e) {{}}
            }};
            try {{ window.speechSynthesis.getVoices(); }} catch(e) {{}}
        }}

        // 최적 원어민 보이스 매칭 (iOS Ava/Samantha, Windows Jenny/Aria, Android Google US)
        function getBestVoice(lang, gender) {{
            if (!('speechSynthesis' in window)) return null;
            const voices = window.speechSynthesis.getVoices();
            if (!voices || voices.length === 0) return null;

            if (lang === 'ko') {{
                return voices.find(v => v.lang.startsWith('ko')) || null;
            }}

            const roboticVoices = ['bad news', 'bells', 'boing', 'cellos', 'deranged', 'good news', 'hysterical', 'junior', 'kathy', 'organ', 'princess', 'ralph', 'trinoids', 'whisper', 'zarvox'];

            if (gender === 'male') {{
                const maleKeywords = ['evan', 'nathan', 'christopher', 'guy', 'daniel', 'alex', 'fred', 'david', 'mark', 'tom', 'aaron', 'george', 'eddy', 'reed', 'rocko', 'male'];
                for (const kw of maleKeywords) {{
                    const found = voices.find(v => v.lang === 'en-US' && v.name.toLowerCase().includes(kw));
                    if (found) return found;
                }}
                for (const kw of maleKeywords) {{
                    const found = voices.find(v => v.lang.startsWith('en') && v.name.toLowerCase().includes(kw));
                    if (found) return found;
                }}
                const femaleKeywords = ['ava', 'samantha', 'jenny', 'aria', 'allison', 'zoe', 'susan', 'karen', 'victoria', 'female'];
                const nonFemale = voices.find(v =>
                    v.lang === 'en-US' &&
                    !femaleKeywords.some(f => v.name.toLowerCase().includes(f)) &&
                    !roboticVoices.some(bad => v.name.toLowerCase().includes(bad))
                );
                if (nonFemale) return nonFemale;
            }} else {{
                const femaleKeywords = ['ava', 'samantha', 'jenny', 'aria', 'allison', 'zoe', 'susan', 'victoria', 'karen', 'female', 'google us english'];
                for (const kw of femaleKeywords) {{
                    const found = voices.find(v => v.lang === 'en-US' && v.name.toLowerCase().includes(kw));
                    if (found) return found;
                }}
                for (const kw of femaleKeywords) {{
                    const found = voices.find(v => v.lang.startsWith('en') && v.name.toLowerCase().includes(kw));
                    if (found) return found;
                }}
                const naturalEnUs = voices.find(v =>
                    v.lang === 'en-US' && !roboticVoices.some(bad => v.name.toLowerCase().includes(bad))
                );
                if (naturalEnUs) return naturalEnUs;
            }}
            return voices.find(v => v.lang.startsWith('en')) || null;
        }}

        // 브라우저 TTS 폴백 (비동기 멈춤 방지 워치독 타이머 포함)
        function playBrowserSpeech(text, lang, gender, token, onFinish) {{
            let finished = false;
            const completeOnce = () => {{
                if (finished) return;
                finished = true;
                if (speechWatchdogTimer) {{
                    clearTimeout(speechWatchdogTimer);
                    speechWatchdogTimer = null;
                }}
                window.currentUtterance = null;
                if (token !== undefined && token !== currentSpeechToken) return;
                if (onFinish) onFinish();
            }};

            if (!('speechSynthesis' in window)) {{
                completeOnce();
                return;
            }}

            try {{
                if (window.speechSynthesis.paused) {{
                    window.speechSynthesis.resume();
                }}
            }} catch(e) {{}}

            const u = new SpeechSynthesisUtterance(text);
            u.lang = (lang === 'ko') ? 'ko-KR' : 'en-US';
            u.rate = (lang === 'ko') ? 1.0 : 0.95;
            u.pitch = 1.0;
            const voice = getBestVoice(lang, gender);
            if (voice) u.voice = voice;

            u.onend = () => completeOnce();
            u.onerror = () => completeOnce();

            window.currentUtterance = u; // prevent GC

            const maxWaitMs = Math.min(12000, Math.max(3000, text.length * 110));
            speechWatchdogTimer = setTimeout(() => {{
                completeOnce();
            }}, maxWaitMs);

            try {{
                window.speechSynthesis.speak(u);
            }} catch(e) {{
                completeOnce();
            }}
        }}

        // 메인 음성 재생 함수 (1순위 Amazon Polly/Google TTS 캐시 -> 실패 시 브라우저 음성 폴백)
        function playSpeech(text, lang, onEndCallback) {{
            if (!text || !text.trim()) {{
                if (onEndCallback) onEndCallback();
                return;
            }}

            const myToken = ++currentSpeechToken;
            const gender = voiceGender;
            const key = getAudioCacheKey(text, lang, gender);
            const audio = getSharedAudio();

            let finished = false;
            let watchdog = null;

            const completeOnce = () => {{
                if (finished) return;
                finished = true;
                if (watchdog) {{
                    clearTimeout(watchdog);
                    watchdog = null;
                }}
                if (myToken !== currentSpeechToken) return;
                if (onEndCallback) onEndCallback();
            }};

            // 모바일/브라우저 오디오 지연 및 멈춤 방지 워치독
            const setWatchdog = (timeoutMs) => {{
                if (watchdog) clearTimeout(watchdog);
                watchdog = setTimeout(() => {{
                    console.warn('Speech playback watchdog triggered for text:', text.substring(0, 30));
                    completeOnce();
                }}, timeoutMs);
            }};

            // 초기 기본 타임아웃: 글자당 350ms (최소 8초, 최대 25초)
            setWatchdog(Math.min(25000, Math.max(8000, text.length * 350)));

            audio.onloadedmetadata = () => {{
                if (myToken !== currentSpeechToken) return;
                if (!isNaN(audio.duration) && audio.duration > 0) {{
                    // 실제 오디오 길이 + 3초 안전 버퍼
                    setWatchdog(Math.ceil((audio.duration + 3) * 1000));
                }}
            }};

            audio.onended = () => {{
                completeOnce();
            }};

            audio.onerror = (e) => {{
                if (finished || myToken !== currentSpeechToken) return;
                console.warn('Audio stream error, falling back to Web Speech:', e);
                if (watchdog) {{ clearTimeout(watchdog); watchdog = null; }}
                playBrowserSpeech(text, lang, gender, myToken, completeOnce);
            }};

            const cachedBlobUrl = audioBlobCache.get(key);
            const directUrl = `/api/tts?text=${{encodeURIComponent(text.trim())}}&lang=${{lang}}&gender=${{gender}}`;
            const targetUrl = cachedBlobUrl || directUrl;

            // 이미 동일한 src를 재생한 직후(예: STEP 3 -> STEP 4)라면 currentTime을 반드시 0으로 리셋
            if (audio.src === targetUrl || (cachedBlobUrl && audio.src === cachedBlobUrl)) {{
                try {{
                    audio.currentTime = 0;
                }} catch(e) {{}}
            }} else {{
                audio.src = targetUrl;
            }}
            try {{
                audio.currentTime = 0;
            }} catch(e) {{}}

            if (!cachedBlobUrl) {{
                prefetchSpeechAudio(text, lang, gender);
            }}

            const p = audio.play();
            if (p !== undefined) {{
                p.catch(err => {{
                    if (finished || myToken !== currentSpeechToken) return;
                    console.warn('audio.play() rejected, retrying with blob prefetch:', err);
                    prefetchSpeechAudio(text, lang, gender).then(blobUrl => {{
                        if (finished || myToken !== currentSpeechToken) return;
                        if (blobUrl) {{
                            audio.src = blobUrl;
                            try {{ audio.currentTime = 0; }} catch(e) {{}}
                        }}
                        audio.play().catch(retryErr => {{
                            if (finished || myToken !== currentSpeechToken) return;
                            console.warn('Retry failed, falling back to Web Speech:', retryErr);
                            if (watchdog) {{ clearTimeout(watchdog); watchdog = null; }}
                            playBrowserSpeech(text, lang, gender, myToken, completeOnce);
                        }});
                    }});
                }});
            }}
        }}

        function stopCurrentSpeech() {{
            currentSpeechToken++;
            if (speechWatchdogTimer) {{
                clearTimeout(speechWatchdogTimer);
                speechWatchdogTimer = null;
            }}
            if (sharedAudio) {{
                sharedAudio.onended = null;
                sharedAudio.onerror = null;
                try {{
                    sharedAudio.pause();
                    sharedAudio.currentTime = 0;
                }} catch(e) {{}}
            }}
            if ('speechSynthesis' in window && (window.speechSynthesis.speaking || window.speechSynthesis.pending)) {{
                try {{
                    window.speechSynthesis.cancel();
                }} catch(e) {{}}
            }}
        }}

        // 잠금화면 및 백그라운드 미디어 세션 연동
        function updateMediaSession(card, idx, total) {{
            if (!('mediaSession' in navigator) || !card) return;
            try {{
                navigator.mediaSession.metadata = new MediaMetadata({{
                    title: card.context_sentence || 'Audio Gym',
                    artist: card.korean_meaning || '말하기 훈련',
                    album: `Audio Gym (${{idx + 1}}/${{total}})`
                }});
                navigator.mediaSession.setActionHandler('play', () => {{ if (!isPlaying) togglePlay(); }});
                navigator.mediaSession.setActionHandler('pause', () => {{ if (isPlaying) togglePlay(); }});
                navigator.mediaSession.setActionHandler('previoustrack', () => {{ prevCard(); }});
                navigator.mediaSession.setActionHandler('nexttrack', () => {{ nextCard(); }});
                navigator.mediaSession.setActionHandler('stop', () => {{ pause(); }});
            }} catch(e) {{}}
        }}

        async function init() {{
            const filter = document.getElementById('filterSelect').value;
            gapSeconds = parseInt(document.getElementById('gapInput').value, 10) || 3;
            voiceGender = document.getElementById('genderSelect').value;
            try {{
                const res = await fetch(`/api/audio-gym/playlist?filter=${{encodeURIComponent(filter)}}&tag={tag}`);
                if (!res.ok) {{
                    throw new Error(`Playlist HTTP error: ${{res.status}}`);
                }}
                playlist = await res.json();
                currentIndex = 0;
                updateProgress();
                if (playlist.length === 0) {{
                    if (filter === 'due') {{
                        koreanText.innerHTML = '오늘 복습할 카드가 없습니다.<br><button class="gym-btn gym-btn-secondary" style="margin-top:0.75rem;font-size:0.875rem;" onclick="switchToAll()">전체 카드로 연습하기</button>';
                    }} else {{
                        koreanText.innerText = '재생 가능한 카드가 없습니다.';
                    }}
                    playBtn.disabled = true;
                }} else {{
                    playBtn.disabled = false;
                    koreanText.innerText = '▶ 시작하기 버튼을 누르면 자동 재생됩니다.';
                    // 첫 문장들 사전 로드 (최대 5장 백그라운드 캐싱)
                    for (let i = 0; i < Math.min(playlist.length, 5); i++) {{
                        prefetchSpeechAudio(playlist[i].korean_meaning, 'ko', voiceGender);
                        prefetchSpeechAudio(playlist[i].context_sentence, 'en', voiceGender);
                    }}
                }}
            }} catch (err) {{
                console.error(err);
                koreanText.innerText = '플레이리스트 로드 실패';
            }}
        }}

        function switchToAll() {{
            document.getElementById('filterSelect').value = 'all';
            applyFilters();
        }}

        function applyFilters() {{
            const filter = document.getElementById('filterSelect').value;
            const gap = document.getElementById('gapInput').value;
            const gender = document.getElementById('genderSelect').value;
            window.location.href = `/audio-gym?filter=${{encodeURIComponent(filter)}}&gap=${{encodeURIComponent(gap)}}&gender=${{encodeURIComponent(gender)}}&tag={tag}`;
        }}

        function updateProgress() {{
            if (playlist.length === 0) {{
                progressText.innerText = '재생 가능한 카드가 없습니다.';
            }} else {{
                progressText.innerText = `카드 ${{currentIndex + 1}} / ${{playlist.length}}`;
            }}
        }}

        async function acquireWakeLock() {{
            try {{
                if ('wakeLock' in navigator) {{
                    wakeLock = await navigator.wakeLock.request('screen');
                }}
            }} catch (e) {{}}
        }}

        function releaseWakeLock() {{
            if (wakeLock) {{
                wakeLock.release().catch(() => {{}});
                wakeLock = null;
            }}
        }}

        function togglePlay() {{
            if (isPlaying) {{
                pause();
            }} else {{
                play();
            }}
        }}

        function pause() {{
            isPlaying = false;
            currentStepRunId++;
            stopCurrentSpeech();
            playBtn.innerText = '▶ 시작하기';
            if (stepTimer) {{
                clearInterval(stepTimer);
                stepTimer = null;
            }}
            releaseWakeLock();
        }}

        async function play() {{
            if (playlist.length === 0) return;
            isPlaying = true;
            currentStepRunId++;
            playBtn.innerText = '⏸ 일시정지';
            acquireWakeLock();

            // 제스처 시점에 단일 Audio 인스턴스 준비
            getSharedAudio();

            runCurrentStep(currentStepRunId);
        }}

        function prevCard() {{
            if (currentIndex > 0) {{
                currentIndex--;
                if (isPlaying) {{
                    currentStepRunId++;
                    stopCurrentSpeech();
                    if (stepTimer) {{ clearInterval(stepTimer); stepTimer = null; }}
                    runCurrentStep(currentStepRunId);
                }} else {{
                    updateProgress();
                    const card = playlist[currentIndex];
                    koreanText.innerText = card.korean_meaning;
                    chunkText.innerText = card.chunk;
                    sentenceText.innerText = card.context_sentence;
                }}
            }}
        }}

        function nextCard() {{
            if (currentIndex < playlist.length - 1) {{
                currentIndex++;
                if (isPlaying) {{
                    currentStepRunId++;
                    stopCurrentSpeech();
                    if (stepTimer) {{ clearInterval(stepTimer); stepTimer = null; }}
                    runCurrentStep(currentStepRunId);
                }} else {{
                    updateProgress();
                    const card = playlist[currentIndex];
                    koreanText.innerText = card.korean_meaning;
                    chunkText.innerText = card.chunk;
                    sentenceText.innerText = card.context_sentence;
                }}
            }} else {{
                pause();
                stepIndicator.innerText = '완료 🎉';
                koreanText.innerText = '모든 카드를 완료했습니다! 🎉';
            }}
        }}

        async function runCurrentStep(runId) {{
            if (!isPlaying || runId !== currentStepRunId || currentIndex >= playlist.length) return;
            const card = playlist[currentIndex];
            updateProgress();
            updateMediaSession(card, currentIndex, playlist.length);

            // 다음 카드들의 오디오를 미리 백그라운드에서 다운로드 (지연 시간 0ms)
            for (let i = 1; i <= 3; i++) {{
                if (currentIndex + i < playlist.length) {{
                    const nextCard = playlist[currentIndex + i];
                    prefetchSpeechAudio(nextCard.korean_meaning, 'ko', voiceGender);
                    prefetchSpeechAudio(nextCard.context_sentence, 'en', voiceGender);
                }}
            }}
            // 현재 카드의 영어 오디오 사전 로드
            prefetchSpeechAudio(card.context_sentence, 'en', voiceGender);

            // STEP 1: 한국어 의미 듣기
            currentStep = 'ko';
            stepIndicator.innerHTML = '<span class="sound-wave"><span></span><span></span><span></span></span> STEP 1: 한국어 의미 듣기';
            koreanText.innerText = card.korean_meaning;
            timerDisplay.style.display = 'none';
            chunkText.style.display = 'none';
            sentenceText.style.display = 'none';

            await new Promise(resolve => {{
                playSpeech(card.korean_meaning, 'ko', resolve);
            }});
            if (!isPlaying || runId !== currentStepRunId) return;

            // STEP 2: 직접 영어로 말해보세요! (Think & Speak)
            currentStep = 'gap';
            stepIndicator.innerText = '⏱️ STEP 2: 직접 영어로 말해보세요! (Think & Speak)';
            timerDisplay.style.display = 'block';
            let remain = gapSeconds;
            timerDisplay.innerText = remain;

            await new Promise(resolve => {{
                stepTimer = setInterval(() => {{
                    if (!isPlaying || runId !== currentStepRunId) {{
                        clearInterval(stepTimer);
                        stepTimer = null;
                        return resolve();
                    }}
                    remain--;
                    if (remain <= 0) {{
                        clearInterval(stepTimer);
                        stepTimer = null;
                        timerDisplay.style.display = 'none';
                        resolve();
                    }} else {{
                        timerDisplay.innerText = remain;
                    }}
                }}, 1000);
            }});
            if (!isPlaying || runId !== currentStepRunId) return;

            // STEP 3: 원어민 영어 확인 (1/2)
            currentStep = 'en1';
            stepIndicator.innerHTML = '<span class="sound-wave"><span></span><span></span><span></span></span> STEP 3: 원어민 영어 확인 (1/2)';
            chunkText.style.display = 'block';
            chunkText.innerText = card.chunk;
            sentenceText.style.display = 'block';
            sentenceText.innerText = card.context_sentence;

            await new Promise(resolve => {{
                playSpeech(card.context_sentence, 'en', resolve);
            }});
            if (!isPlaying || runId !== currentStepRunId) return;

            await new Promise(r => setTimeout(r, 600));
            if (!isPlaying || runId !== currentStepRunId) return;

            // STEP 4: 입으로 따라 말하기 (Shadowing 2/2)
            currentStep = 'en2';
            stepIndicator.innerHTML = '<span class="sound-wave"><span></span><span></span><span></span></span> STEP 4: 입으로 따라 말하기 (Shadowing 2/2)';
            await new Promise(resolve => {{
                playSpeech(card.context_sentence, 'en', resolve);
            }});
            if (!isPlaying || runId !== currentStepRunId) return;

            await new Promise(r => setTimeout(r, 1200));
            if (!isPlaying || runId !== currentStepRunId) return;

            if (currentIndex < playlist.length - 1) {{
                currentIndex++;
                runCurrentStep(runId);
            }} else {{
                pause();
                stepIndicator.innerText = '완료 🎉';
                koreanText.innerText = '모든 카드를 완료했습니다! 🎉';
                timerDisplay.style.display = 'none';
                chunkText.style.display = 'none';
                sentenceText.style.display = 'none';
            }}
        }}

        init();
    </script>
</body>
</html>"#,
        css = APP_CSS,
        all_count = all_count,
        due_count = due_count,
        lapsed_count = lapsed_count,
        gap = gap,
        gender = gender,
        tag = escape_html(tag),
        sel_due = if filter == "due" { "selected" } else { "" },
        sel_lapsed = if filter == "lapsed" { "selected" } else { "" },
        sel_all = if filter == "all" { "selected" } else { "" },
        sel_female = if gender == "female" { "selected" } else { "" },
        sel_male = if gender == "male" { "selected" } else { "" },
    );

    Html(html).into_response()
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/audio-gym", get(audio_gym_page))
        .route("/api/audio-gym/playlist", get(audio_gym_playlist_api))
        .route("/api/tts", get(tts_proxy_handler))
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
}
