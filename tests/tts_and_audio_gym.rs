use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use anyhow::Result;
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use my_english::{
    build_app, cards, db,
    domain::CreateStudyCardInput,
    state::AppState,
    tts::{AudioGymItem, TtsProvider},
};
use tower::ServiceExt;

#[derive(Debug, Default)]
struct FakeTtsProvider {
    calls: AtomicUsize,
    recorded: Mutex<Vec<(String, String, String)>>,
}

impl FakeTtsProvider {
    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn recorded_calls(&self) -> Vec<(String, String, String)> {
        self.recorded.lock().unwrap().clone()
    }
}

#[async_trait]
impl TtsProvider for FakeTtsProvider {
    async fn synthesize(&self, text: &str, lang: &str, gender: &str) -> Result<Vec<u8>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.recorded
            .lock()
            .unwrap()
            .push((text.to_string(), lang.to_string(), gender.to_string()));
        Ok(format!("MP3:{lang}:{gender}:{text}").into_bytes())
    }
}

async fn test_app_with_tts(tts: Arc<FakeTtsProvider>) -> (axum::Router, String, sqlx::SqlitePool) {
    let pool = db::init_memory_db().await.expect("init memory db");
    let state = AppState::new(pool.clone(), "my-secret-passcode", "my-cookie-signing-key")
        .with_tts_provider(tts);
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
async fn tts_proxy_serves_audio_mpeg_and_caches_identical_requests_in_memory() {
    let fake_tts = Arc::new(FakeTtsProvider::default());
    let (app, cookie, _pool) = test_app_with_tts(fake_tts.clone()).await;

    // 1. First request synthesizes via TtsProvider and caches the MP3 bytes
    let res1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tts?text=That%20final%20scene%20gets%20me%20every%20time.&lang=en&gender=female")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    assert_eq!(
        res1.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("audio/mpeg")
    );
    assert_eq!(
        res1.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("public, max-age=86400")
    );
    let bytes1 = res1.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        &bytes1[..],
        b"MP3:en:female:That final scene gets me every time."
    );
    assert_eq!(fake_tts.call_count(), 1);

    // 2. Second identical request hits cache
    let res2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tts?text=%20That%20final%20scene%20gets%20me%20every%20time.%20&lang=en&gender=female")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let bytes2 = res2.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes1, bytes2);
    assert_eq!(fake_tts.call_count(), 1);

    // 3. Different gender or language misses cache
    let res3 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tts?text=That%20final%20scene%20gets%20me%20every%20time.&lang=en&gender=male")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res3.status(), StatusCode::OK);
    assert_eq!(fake_tts.call_count(), 2);

    // 4. Korean request
    let res4 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tts?text=%EA%B7%B8%20%EB%A7%88%EC%A7%89%20%EC%9E%A5%EB%A9%B4&lang=ko")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res4.status(), StatusCode::OK);
    assert_eq!(fake_tts.call_count(), 3);

    // 5. Empty text -> 400
    let res_empty = app
        .oneshot(
            Request::builder()
                .uri("/api/tts?text=%20%20&lang=en")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_empty.status(), StatusCode::BAD_REQUEST);
    assert_eq!(fake_tts.recorded_calls().len(), 3);
}

#[tokio::test]
async fn audio_gym_page_and_playlist_api_filters_cards() {
    let fake_tts = Arc::new(FakeTtsProvider::default());
    let (app, cookie, pool) = test_app_with_tts(fake_tts.clone()).await;

    // Create 2 study cards
    let _card1 = cards::insert_study_card(
        &pool,
        &CreateStudyCardInput {
            chunk: "get me every time".into(),
            surface_chunk: "gets me every time".into(),
            context_sentence: "That scene gets me every time.".into(),
            korean_meaning: "볼 때마다 웃겨/울컥해".into(),
            extra_examples: vec![],
            nuance_note: "감정을 건드리는 상황".into(),
            tags: vec!["movie".into(), "emotion".into()],
            source_type: Some("manual".into()),
        },
    )
    .await
    .unwrap();

    let _card2 = cards::insert_study_card(
        &pool,
        &CreateStudyCardInput {
            chunk: "hit the ground running".into(),
            surface_chunk: "hit the ground running".into(),
            context_sentence: "We need to hit the ground running tomorrow.".into(),
            korean_meaning: "지체 없이 전속력으로 착수하다".into(),
            extra_examples: vec![],
            nuance_note: "비즈니스 시작".into(),
            tags: vec!["work".into()],
            source_type: Some("manual".into()),
        },
    )
    .await
    .unwrap();

    // 1. GET /audio-gym renders HTML
    let gym_page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/audio-gym?filter=due&gap=4&gender=male")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(gym_page.status(), StatusCode::OK);
    let html = String::from_utf8(gym_page.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(html.contains("Audio Gym"));
    assert!(html.contains("Think & Speak"));
    assert!(html.contains("오늘 복습 대상: <strong>2</strong>개 (전체: 2개)"));
    assert!(html.contains("오늘 복습 카드 (2)"));

    // 2. GET /api/audio-gym/playlist?filter=due returns both cards (new cards default to due_date <= now)
    let playlist_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/audio-gym/playlist?filter=due")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(playlist_res.status(), StatusCode::OK);
    let items: Vec<AudioGymItem> = serde_json::from_slice(
        &playlist_res.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(items.len(), 2);
    let chunks: Vec<&str> = items.iter().map(|i| i.chunk.as_str()).collect();
    assert!(chunks.contains(&"get me every time"));
    assert!(chunks.contains(&"hit the ground running"));

    // 3. Filter by tag
    let tag_res = app
        .oneshot(
            Request::builder()
                .uri("/api/audio-gym/playlist?filter=all&tag=work")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let tag_items: Vec<AudioGymItem> = serde_json::from_slice(
        &tag_res.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(tag_items.len(), 1);
    assert_eq!(tag_items[0].chunk, "hit the ground running");
}

#[tokio::test]
async fn home_page_renders_voice_gender_picker_and_card_tts_speaker_buttons() {
    let fake_tts = Arc::new(FakeTtsProvider::default());
    let (app, cookie, pool) = test_app_with_tts(fake_tts.clone()).await;

    // Create a card
    cards::insert_study_card(
        &pool,
        &CreateStudyCardInput {
            chunk: "would you mind if I".to_string(),
            surface_chunk: "Would you mind if I".to_string(),
            context_sentence: "Would you mind if I left a bit early today?".to_string(),
            korean_meaning: "제가 오늘 조금 일찍 가도 될까요?".to_string(),
            extra_examples: vec![],
            nuance_note: String::new(),
            tags: vec!["polite".to_string()],
            source_type: None,
        },
    )
    .await
    .unwrap();

    let res = app
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
    let html = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();

    // 1. Navbar contains voice picker with female and male options
    assert!(html.contains("voice-picker"));
    assert!(html.contains("voice-select-input"));
    assert!(html.contains("switchVoiceGender"));
    assert!(html.contains("여성 음성"));
    assert!(html.contains("남성 음성"));

    // 2. Study Card contains the TTS speaker button next to the context sentence
    assert!(html.contains("context-sentence-wrap"));
    assert!(html.contains("btn-tts-speaker"));
    assert!(html.contains("playSentenceAudio"));
    assert!(html.contains("Would you mind if I left a bit early today?"));
}
