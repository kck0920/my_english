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
use crate::ui::{escape_html, APP_CSS};

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
            // 1st choice: Amazon Polly
            match self.fetch_amazon_polly(text, gender).await {
                Ok(bytes) if !bytes.is_empty() => return Ok(bytes),
                Err(err) => {
                    tracing::warn!("Amazon Polly failed, falling back to Google TTS: {:#}", err);
                }
                _ => {}
            }
            // 2nd choice: Google Translate TTS fallback
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
        Some(ref t) if !t.trim().is_empty() => t.trim(),
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

    let now_str = chrono::Utc::now().to_rfc3339();

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
    let filter = query.filter.as_deref().unwrap_or("due");
    let tag = query.tag.as_deref().unwrap_or("");
    let gap = query.gap.clamp(1, 15);
    let gender = if query.gender.eq_ignore_ascii_case("male") {
        "male"
    } else {
        "female"
    };

    let due_count = crate::cards::count_due_cards(&state.pool).await.unwrap_or(0);

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Audio Gym - My English</title>
    <link rel="manifest" href="/manifest.json">
    <style>
        {css}
        .gym-header {{ display: flex; justify-content: space-between; align-items: center; margin-bottom: 1.5rem; }}
        .gym-stage {{ background: var(--bg-card); border-radius: 1rem; padding: 2rem 1.5rem; text-align: center; box-shadow: 0 4px 6px -1px rgba(0,0,0,0.1); margin-bottom: 1.5rem; border: 1px solid var(--border); }}
        .gym-step-indicator {{ font-size: 0.875rem; font-weight: 700; color: var(--primary); text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.75rem; }}
        .gym-korean {{ font-size: 1.25rem; font-weight: 600; color: var(--text-dark); margin-bottom: 1rem; min-height: 2rem; }}
        .gym-chunk {{ font-size: 1.5rem; font-weight: 700; color: var(--primary); margin-bottom: 0.5rem; min-height: 2.25rem; }}
        .gym-sentence {{ font-size: 1.125rem; color: var(--text-muted); line-height: 1.6; min-height: 3rem; }}
        .gym-timer {{ font-size: 3rem; font-weight: 800; color: var(--accent); margin: 1rem 0; font-variant-numeric: tabular-nums; }}
        .gym-controls {{ display: flex; gap: 0.75rem; justify-content: center; align-items: center; margin-bottom: 1.5rem; }}
        .gym-btn {{ padding: 0.75rem 1.5rem; border-radius: 0.75rem; font-size: 1rem; font-weight: 700; cursor: pointer; border: none; }}
        .gym-btn-primary {{ background: var(--primary); color: white; }}
        .gym-btn-secondary {{ background: var(--bg-hover); color: var(--text-dark); }}
        .gym-config {{ display: flex; gap: 1rem; justify-content: center; flex-wrap: wrap; margin-bottom: 1.5rem; font-size: 0.875rem; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="gym-header">
            <div>
                <a href="/" style="text-decoration: none; font-size: 0.875rem; color: var(--primary);">← 홈으로</a>
                <h1 style="margin: 0.25rem 0 0 0; font-size: 1.5rem;">🎧 Audio Gym</h1>
            </div>
            <div style="font-size: 0.875rem; color: var(--text-muted);">
                오늘 복습 대상: <strong>{due_count}</strong>개
            </div>
        </div>

        <div class="gym-config">
            <div>
                <label>재생 필터: </label>
                <select id="filterSelect" onchange="applyFilters()">
                    <option value="due" {sel_due}>오늘 복습 카드 ({due_count})</option>
                    <option value="lapsed" {sel_lapsed}>자주 틀린 카드</option>
                    <option value="all" {sel_all}>전체 카드</option>
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

    <audio id="ttsAudio" preload="auto"></audio>

    <script>
        let playlist = [];
        let currentIndex = 0;
        let isPlaying = false;
        let gapSeconds = {gap};
        let voiceGender = '{gender}';
        let currentStep = 'idle';
        let stepTimer = null;
        let wakeLock = null;

        const audio = document.getElementById('ttsAudio');
        const playBtn = document.getElementById('playBtn');
        const stepIndicator = document.getElementById('stepIndicator');
        const koreanText = document.getElementById('koreanText');
        const timerDisplay = document.getElementById('timerDisplay');
        const chunkText = document.getElementById('chunkText');
        const sentenceText = document.getElementById('sentenceText');
        const progressText = document.getElementById('progressText');

        async function init() {{
            const filter = document.getElementById('filterSelect').value;
            gapSeconds = parseInt(document.getElementById('gapInput').value, 10) || 3;
            voiceGender = document.getElementById('genderSelect').value;
            try {{
                const res = await fetch(`/api/audio-gym/playlist?filter=${{filter}}&tag={tag}`);
                playlist = await res.json();
                updateProgress();
                if (playlist.length === 0) {{
                    koreanText.innerText = '재생할 카드가 없습니다.';
                    playBtn.disabled = true;
                }} else {{
                    playBtn.disabled = false;
                }}
            }} catch (err) {{
                console.error(err);
                koreanText.innerText = '플레이리스트 로드 실패';
            }}
        }}

        function applyFilters() {{
            const filter = document.getElementById('filterSelect').value;
            const gap = document.getElementById('gapInput').value;
            const gender = document.getElementById('genderSelect').value;
            window.location.href = `/audio-gym?filter=${{filter}}&gap=${{gap}}&gender=${{gender}}&tag={tag}`;
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
            playBtn.innerText = '▶ 재생';
            audio.pause();
            if ('speechSynthesis' in window) {{
                window.speechSynthesis.cancel();
            }}
            if (stepTimer) clearInterval(stepTimer);
            releaseWakeLock();
        }}

        function play() {{
            if (playlist.length === 0) return;
            isPlaying = true;
            playBtn.innerText = '⏸ 일시정지';
            acquireWakeLock();
            runCurrentStep();
        }}

        function prevCard() {{
            if (currentIndex > 0) {{
                currentIndex--;
                currentStep = 'ko';
                runCurrentStep();
            }}
        }}

        function nextCard() {{
            if (currentIndex < playlist.length - 1) {{
                currentIndex++;
                currentStep = 'ko';
                runCurrentStep();
            }} else {{
                pause();
                stepIndicator.innerText = '완료';
                koreanText.innerText = '모든 카드를 완료했습니다! 🎉';
            }}
        }}

        function playTts(text, lang) {{
            return new Promise((resolve) => {{
                if (!isPlaying) return resolve();

                let finished = false;
                const done = () => {{
                    if (!finished) {{
                        finished = true;
                        resolve();
                    }}
                }};

                const fallbackWebSpeech = () => {{
                    if (!isPlaying) return done();
                    if ('speechSynthesis' in window) {{
                        window.speechSynthesis.cancel();
                        const u = new SpeechSynthesisUtterance(text);
                        u.lang = (lang === 'ko') ? 'ko-KR' : 'en-US';
                        u.rate = (lang === 'ko') ? 1.0 : 0.95;
                        u.onend = () => done();
                        u.onerror = (e) => {{
                            console.warn('SpeechSynthesis error:', e);
                            done();
                        }};
                        window.speechSynthesis.speak(u);
                    }} else {{
                        done();
                    }}
                }};

                try {{
                    const url = `/api/tts?text=${{encodeURIComponent(text)}}&lang=${{lang}}&gender=${{voiceGender}}`;
                    const s = new Audio(url);
                    let timeoutId = setTimeout(() => {{
                        console.warn('Audio play timed out after 7s, falling back');
                        s.pause();
                        fallbackWebSpeech();
                    }}, 7000);

                    s.onended = () => {{
                        clearTimeout(timeoutId);
                        done();
                    }};
                    s.onerror = (e) => {{
                        clearTimeout(timeoutId);
                        console.warn('Server TTS failed, falling back to Web Speech API', e);
                        fallbackWebSpeech();
                    }};

                    const p = s.play();
                    if (p && typeof p.catch === 'function') {{
                        p.catch((err) => {{
                            clearTimeout(timeoutId);
                            console.warn('Audio play() rejected, falling back to Web Speech API', err);
                            fallbackWebSpeech();
                        }});
                    }}
                }} catch (e) {{
                    console.warn('Audio object initialization failed, falling back', e);
                    fallbackWebSpeech();
                }}
            }});
        }}

        async function runCurrentStep() {{
            if (!isPlaying || currentIndex >= playlist.length) return;
            const card = playlist[currentIndex];
            updateProgress();

            currentStep = 'ko';
            stepIndicator.innerText = 'STEP 1: 한국어 의미 듣기';
            koreanText.innerText = card.korean_meaning;
            timerDisplay.style.display = 'none';
            chunkText.style.display = 'none';
            sentenceText.style.display = 'none';

            await playTts(card.korean_meaning, 'ko');
            if (!isPlaying) return;

            currentStep = 'gap';
            stepIndicator.innerText = 'STEP 2: 직접 영어로 말해보세요! (Think & Speak)';
            timerDisplay.style.display = 'block';
            let remain = gapSeconds;
            timerDisplay.innerText = remain;

            await new Promise(resolve => {{
                stepTimer = setInterval(() => {{
                    remain--;
                    if (remain <= 0) {{
                        clearInterval(stepTimer);
                        timerDisplay.style.display = 'none';
                        resolve();
                    }} else {{
                        timerDisplay.innerText = remain;
                    }}
                }}, 1000);
            }});
            if (!isPlaying) return;

            currentStep = 'en1';
            stepIndicator.innerText = 'STEP 3: 원어민 영어 확인 (1/2)';
            chunkText.style.display = 'block';
            chunkText.innerText = card.chunk;
            sentenceText.style.display = 'block';
            sentenceText.innerText = card.context_sentence;

            await playTts(card.context_sentence, 'en');
            if (!isPlaying) return;

            await new Promise(r => setTimeout(r, 600));
            if (!isPlaying) return;

            currentStep = 'en2';
            stepIndicator.innerText = 'STEP 4: 입으로 따라 말하기 (Shadowing 2/2)';
            await playTts(card.context_sentence, 'en');
            if (!isPlaying) return;

            await new Promise(r => setTimeout(r, 1200));
            if (!isPlaying) return;

            nextCard();
        }}

        init();
    </script>
</body>
</html>"#,
        css = APP_CSS,
        due_count = due_count,
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
}

pub fn public_routes() -> Router<AppState> {
    Router::new().route("/api/tts", get(tts_proxy_handler))
}
