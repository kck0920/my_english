use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use my_english::{build_app, db, state::AppState};
use tower::ServiceExt;

async fn test_app_with_auth() -> (axum::Router, String) {
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
        .expect("set-cookie header");
    let cookie = set_cookie.split(';').next().unwrap().trim().to_string();
    (app, cookie)
}

#[tokio::test]
async fn data_page_renders_import_forms_and_backup_links() {
    let (app, cookie) = test_app_with_auth().await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/data")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("/import/text"));
    assert!(html.contains("/import/sqlite"));
    assert!(html.contains("/export/sqlite"));
    assert!(html.contains("/export/json"));
    assert!(html.contains("/export/anki.csv"));
}

#[tokio::test]
async fn import_legacy_english_note_text_six_section_and_four_section_with_dedup() {
    let (app, cookie) = test_app_with_auth().await;

    let six_section_text = r#"
[원문]
I'm under the weather.

[올바른 해석]
몸 상태가 좀 안 좋아. 컨디션이 별로야.

[이 문장이 실제로 사용되는 상황]
출근 후 동료가 안색이 안 좋아 보인다고 물었을 때 가볍게 대답하는 상황.

[미국인이 가장 자연스럽게 쓰는 실제 뉘앙스와 감각]
'under the weather'는 감기 기운이 있거나 피로해서 평소보다 쳐질 때 가장 흔하게 쓰는 완곡한 표현입니다.

[미국인이 입버릇처럼 쓰는 실전 대체 표현]
1. I'm feeling a bit off today. | 오늘 컨디션이 좀 별로야.
2. I think I'm coming down with something.

[30년 차 강사의 실전 Anki 카드 정리]
앞면: I'm {{c1::under the weather}}.
뒷면: 몸 상태가 좀 안 좋아.
"#;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/import/text")
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .header("accept", "application/json")
                .body(Body::from(format!(
                    "text={}",
                    urlencoding::encode(six_section_text)
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("json response");
    assert_eq!(json["imported_count"], 1);
    assert_eq!(json["skipped_count"], 0);
    assert_eq!(json["cards"][0]["chunk"], "under the weather");
    assert_eq!(json["cards"][0]["surface_chunk"], "under the weather");
    assert_eq!(json["cards"][0]["context_sentence"], "I'm under the weather.");
    assert_eq!(
        json["cards"][0]["korean_meaning"],
        "몸 상태가 좀 안 좋아. 컨디션이 별로야."
    );
    assert_eq!(json["cards"][0]["source_type"], "legacy_import");
    assert!(
        json["cards"][0]["nuance_note"]
            .as_str()
            .unwrap()
            .contains("완곡한 표현")
    );
    assert_eq!(
        json["cards"][0]["extra_examples"][0]["english"],
        "I'm feeling a bit off today."
    );

    // Importing the exact same note again skips the duplicate
    let dup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/import/text")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "text": six_section_text }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dup_res.status(), StatusCode::OK);
    let dup_bytes = dup_res.into_body().collect().await.unwrap().to_bytes();
    let dup_json: serde_json::Value = serde_json::from_slice(&dup_bytes).unwrap();
    assert_eq!(dup_json["imported_count"], 0);
    assert_eq!(dup_json["skipped_count"], 1);

    // Legacy 4-section format ([원문], [자연스러운 표현], [분석], [예문])
    let four_section_text = r#"
[원문]
I have a headache.

[자연스러운 표현]
My head hurts.

[분석]
'have a headache'도 쓰이지만 'My head hurts'가 더 일상적입니다.

[예문]
1. My head is killing me.
"#;

    let res4 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/import/text")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "text": four_section_text }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res4.status(), StatusCode::OK);
    let bytes4 = res4.into_body().collect().await.unwrap().to_bytes();
    let json4: serde_json::Value = serde_json::from_slice(&bytes4).unwrap();
    assert_eq!(json4["imported_count"], 1);
    assert_eq!(json4["cards"][0]["chunk"], "have a headache");
    assert_eq!(json4["cards"][0]["context_sentence"], "I have a headache.");
    assert_eq!(json4["cards"][0]["source_type"], "legacy_import");
    assert!(
        json4["cards"][0]["nuance_note"]
            .as_str()
            .unwrap()
            .contains("My head hurts")
    );
}

#[tokio::test]
async fn import_legacy_notes_db_sqlite_file_via_multipart_and_raw_bytes() {
    let (app, cookie) = test_app_with_auth().await;

    // Create a temporary notes.db SQLite file matching English_note schema
    let tmp = tempfile::NamedTempFile::new().expect("temp sqlite file");
    let db_url = format!("sqlite://{}", tmp.path().display());
    let legacy_pool = sqlx::SqlitePool::connect(&db_url)
        .await
        .expect("connect legacy notes.db");

    sqlx::raw_sql(
        r#"
        CREATE TABLE notes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            raw_text TEXT NOT NULL,
            original TEXT NOT NULL DEFAULT '',
            natural TEXT NOT NULL DEFAULT '',
            analysis TEXT NOT NULL DEFAULT '',
            examples TEXT NOT NULL DEFAULT '',
            correct_translation TEXT NOT NULL DEFAULT '',
            used_context TEXT NOT NULL DEFAULT '',
            nuance TEXT NOT NULL DEFAULT '',
            alternatives TEXT NOT NULL DEFAULT '',
            anki_card TEXT NOT NULL DEFAULT '',
            is_parsed BOOLEAN NOT NULL DEFAULT 1,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .execute(&legacy_pool)
    .await
    .expect("create notes table");

    // Row 1: Structured 6-section columns
    sqlx::query(
        r#"
        INSERT INTO notes (raw_text, original, correct_translation, used_context, nuance, alternatives, anki_card)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("[원문]\nI can afford groceries easily.")
    .bind("I can afford groceries easily.")
    .bind("나는 식료품비 정도는 부담 없이 감당할 수 있어.\n* **can afford**: 감당할 여유가 있다")
    .bind("예산이나 생활비 이야기를 할 때")
    .bind("'can afford'는 금전적·시간적 여유가 있을 때 씁니다.")
    .bind("1. Groceries aren't a burden.\n2. I can easily cover the cost.")
    .bind("앞면: I {{c1::can afford}} groceries easily.")
    .execute(&legacy_pool)
    .await
    .unwrap();

    // Row 2: Duplicate of Row 1
    sqlx::query(
        r#"
        INSERT INTO notes (raw_text, original, correct_translation, nuance, anki_card)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind("[원문]\nI can afford groceries easily.")
    .bind("I can afford groceries easily.")
    .bind("나는 식료품비 정도는 부담 없이 감당할 수 있어.")
    .bind("'can afford' 중복 노트")
    .bind("앞면: I {{c1::can afford}} groceries easily.")
    .execute(&legacy_pool)
    .await
    .unwrap();

    // Row 3: Legacy 4-section row stored in raw_text / legacy columns
    sqlx::query(
        r#"
        INSERT INTO notes (raw_text, original, natural, analysis, examples)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind("[원문]\nLet's play it by ear.")
    .bind("Let's play it by ear.")
    .bind("상황 봐서 즉흥적으로 정하자.")
    .bind("'play it by ear'는 계획 없이 상황에 맞춰 행동한다는 뜻입니다.")
    .bind("1. We don't have a schedule, so let's play it by ear.")
    .execute(&legacy_pool)
    .await
    .unwrap();

    legacy_pool.close().await;

    let sqlite_bytes = std::fs::read(tmp.path()).expect("read sqlite file bytes");

    // Build multipart/form-data body
    let boundary = "----MyEnglishBoundary7MA4YWxkTrZu0gW";
    let mut multipart_body = Vec::new();
    multipart_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"notes.db\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    multipart_body.extend_from_slice(&sqlite_bytes);
    multipart_body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/import/sqlite")
                .header("cookie", &cookie)
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header("accept", "application/json")
                .body(Body::from(multipart_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("json response");
    assert_eq!(json["imported_count"], 2);
    assert_eq!(json["skipped_count"], 1);
    assert_eq!(json["cards"][0]["chunk"], "can afford");
    assert_eq!(
        json["cards"][0]["context_sentence"],
        "I can afford groceries easily."
    );
    assert_eq!(json["cards"][1]["chunk"], "play it by ear");
    assert_eq!(json["cards"][1]["context_sentence"], "Let's play it by ear.");

    // Uploading raw SQLite bytes again skips all 3 rows as duplicates
    let res2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/import/sqlite")
                .header("cookie", &cookie)
                .header("content-type", "application/octet-stream")
                .header("accept", "application/json")
                .body(Body::from(sqlite_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();
    let json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert_eq!(json2["imported_count"], 0);
    assert_eq!(json2["skipped_count"], 3);
}

#[tokio::test]
async fn export_sqlite_json_and_anki_csv_backups() {
    let (app, cookie) = test_app_with_auth().await;

    // Seed two Study Cards
    for payload in [
        serde_json::json!({
            "chunk": "get me every time",
            "context_sentence": "That final scene gets me every time.",
            "korean_meaning": "그 마지막 장면은 볼 때마다 나를 울려.",
            "extra_examples": [{"english": "His dad jokes get me every time.", "korean": "그의 아재 개그는 들을 때마다 웃겨."}],
            "nuance_note": "감정이 반복적으로 북받칠 때 사용",
            "tags": ["movie", "emotion"]
        }),
        serde_json::json!({
            "chunk": "call it a day",
            "context_sentence": "Let's call it a day and go home.",
            "korean_meaning": "오늘 일은 여기까지 하고 마무리하다",
            "tags": ["work"]
        }),
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cards")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    // 1. GET /export/json
    let json_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/export/json")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(json_res.status(), StatusCode::OK);
    let json_bytes = json_res.into_body().collect().await.unwrap().to_bytes();
    let exported_cards: Vec<serde_json::Value> =
        serde_json::from_slice(&json_bytes).expect("valid JSON array of StudyCards");
    assert_eq!(exported_cards.len(), 2);
    assert!(
        exported_cards
            .iter()
            .any(|c| c["chunk"] == "get me every time")
    );
    assert!(exported_cards.iter().any(|c| c["chunk"] == "call it a day"));

    // 2. GET /export/anki.csv
    let csv_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/export/anki.csv")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(csv_res.status(), StatusCode::OK);
    let csv_bytes = csv_res.into_body().collect().await.unwrap().to_bytes();
    // Must begin with UTF-8 BOM \u{FEFF} ([0xEF, 0xBB, 0xBF])
    assert!(
        csv_bytes.starts_with("\u{FEFF}".as_bytes()),
        "Anki CSV must start with UTF-8 BOM"
    );
    let csv_text = String::from_utf8(csv_bytes.to_vec()).expect("valid UTF-8 CSV");
    assert!(csv_text.contains("{{c1::gets me every time}}"));
    assert!(csv_text.contains("get me every time"));
    assert!(csv_text.contains("그 마지막 장면은 볼 때마다 나를 울려."));
    assert!(csv_text.contains("movie"));

    // 3. GET /export/sqlite
    let sqlite_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/export/sqlite")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sqlite_res.status(), StatusCode::OK);
    let sqlite_bytes = sqlite_res.into_body().collect().await.unwrap().to_bytes();
    assert!(
        sqlite_bytes.starts_with(b"SQLite format 3\0"),
        "Exported SQLite backup must have valid SQLite3 binary header"
    );

    // Verify the exported SQLite binary can be opened and has all 2 StudyCards
    let backup_tmp = tempfile::NamedTempFile::new().expect("temp backup file");
    std::fs::write(backup_tmp.path(), &sqlite_bytes).expect("write exported sqlite");
    let backup_url = format!("sqlite://{}", backup_tmp.path().display());
    let backup_pool = sqlx::SqlitePool::connect(&backup_url)
        .await
        .expect("open exported sqlite db");
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM study_cards")
        .fetch_one(&backup_pool)
        .await
        .expect("count study_cards in exported db");
    assert_eq!(count.0, 2);
    backup_pool.close().await;
}

#[test]
fn oci_deployment_configs_exist_for_systemd_and_nginx() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let service_path = manifest_dir.join("deploy/my-english.service");
    let service_content =
        std::fs::read_to_string(&service_path).expect("deploy/my-english.service must exist");
    assert!(service_content.contains("[Unit]"));
    assert!(service_content.contains("[Service]"));
    assert!(service_content.contains("ExecStart="));
    assert!(service_content.contains("my_english"));
    assert!(service_content.contains("Restart=always"));

    let nginx_path = manifest_dir.join("deploy/nginx-english-note.conf");
    let nginx_content =
        std::fs::read_to_string(&nginx_path).expect("deploy/nginx-english-note.conf must exist");
    assert!(nginx_content.contains("english-note.duckdns.org"));
    assert!(nginx_content.contains("proxy_pass"));

    let script_path = manifest_dir.join("deploy/deploy-oci.sh");
    let script_content =
        std::fs::read_to_string(&script_path).expect("deploy/deploy-oci.sh must exist");
    assert!(script_content.contains("cargo build --release"));
    assert!(script_content.contains("systemctl"));
    assert!(script_content.contains("english-note.duckdns.org"));
}




