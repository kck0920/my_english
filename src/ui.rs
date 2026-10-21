use crate::domain::StudyCard;

pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn render_pwa_manifest() -> &'static str {
    r##"{
  "name": "My English — Chunk & SRS",
  "short_name": "MyEnglish",
  "start_url": "/",
  "display": "standalone",
  "background_color": "#0f172a",
  "theme_color": "#2563eb",
  "description": "Chunk-based AI English Study & SRS App"
}"##
}

pub const APP_CSS: &str = r#"
:root {
  --bg: #0f172a;
  --surface: #1e293b;
  --surface-hover: #334155;
  --text: #f8fafc;
  --muted: #94a3b8;
  --primary: #3b82f6;
  --primary-hover: #2563eb;
  --accent: #10b981;
  --danger: #ef4444;
  --border: #334155;
  --radius: 12px;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  background: var(--bg);
  color: var(--text);
  line-height: 1.5;
}
.container {
  max-width: 900px;
  margin: 0 auto;
  padding: 1rem;
}
header.navbar {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: 0.75rem;
  padding: 0.75rem 0;
  border-bottom: 1px solid var(--border);
  margin-bottom: 1.25rem;
}
.nav-links {
  display: flex;
  gap: 0.75rem;
  align-items: center;
  flex-wrap: wrap;
}
.nav-links a, button, .btn {
  background: var(--surface);
  color: var(--text);
  border: 1px solid var(--border);
  padding: 0.5rem 0.9rem;
  border-radius: 8px;
  text-decoration: none;
  cursor: pointer;
  font-size: 0.9rem;
}
.btn-primary {
  background: var(--primary);
  border-color: var(--primary);
  color: #fff;
  font-weight: 600;
}
.btn-danger {
  background: transparent;
  border-color: var(--danger);
  color: var(--danger);
}
.panel {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 1rem 1.25rem;
  margin-bottom: 1.25rem;
}
.review-banner {
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-left: 4px solid var(--accent);
}
.card-grid {
  display: grid;
  gap: 1rem;
}
.study-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 1rem 1.25rem;
}
.chunk-header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 0.5rem;
  flex-wrap: wrap;
}
.chunk-title {
  font-size: 1.2rem;
  font-weight: 700;
  color: #60a5fa;
}
.surface-badge, .stage-badge, .tag-badge {
  display: inline-block;
  padding: 0.15rem 0.55rem;
  border-radius: 999px;
  font-size: 0.78rem;
  background: #0f172a;
  border: 1px solid var(--border);
  color: var(--muted);
}
input, textarea, select {
  width: 100%;
  background: #0f172a;
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 0.6rem 0.75rem;
  font-size: 0.95rem;
  margin-top: 0.25rem;
  margin-bottom: 0.75rem;
}
"#;

pub fn render_login_page(error: Option<&str>) -> String {
    let error_html = match error {
        Some(msg) => format!(
            r#"<div class="alert alert-error" role="alert">{}</div>"#,
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
  <title>로그인 — My English</title>
  <link rel="manifest" href="/manifest.json">
  <style>{APP_CSS}</style>
</head>
<body>
  <main class="container">
    <section class="panel" style="max-width: 420px; margin: 4rem auto;">
      <h1>My English</h1>
      <p>나만의 영어 Chunk 수집 &amp; 간격 반복(SRS) 학습장</p>
      {error_html}
      <form method="POST" action="/login">
        <label for="passcode">Passcode</label>
        <input type="password" id="passcode" name="passcode" required autofocus placeholder="패스코드 (기본값: 1234)">
        <button type="submit" class="btn btn-primary" style="width:100%;">로그인</button>
      </form>
    </section>
  </main>
</body>
</html>"#
    )
}

pub fn render_dashboard(
    cards: &[StudyCard],
    due_count: i64,
    all_tags: &[String],
    q: Option<&str>,
    selected_tag: Option<&str>,
    selected_stage: Option<i64>,
) -> String {
    let q_val = escape_html(q.unwrap_or(""));
    let mut tag_options = String::from(r#"<option value="">모든 태그 (All Tags)</option>"#);
    for t in all_tags {
        let sel = if selected_tag == Some(t.as_str()) {
            " selected"
        } else {
            ""
        };
        tag_options.push_str(&format!(
            r#"<option value="{val}"{sel}>#{val}</option>"#,
            val = escape_html(t)
        ));
    }

    let mut stage_options = String::from(r#"<option value="">모든 SRS Stage</option>"#);
    for st in 0..=5 {
        let sel = if selected_stage == Some(st) {
            " selected"
        } else {
            ""
        };
        stage_options.push_str(&format!(
            r#"<option value="{st}"{sel}>SRS Stage {st}</option>"#
        ));
    }

    let mut cards_html = String::new();
    if cards.is_empty() {
        cards_html.push_str(
            r#"<div class="panel"><p>저장된 Study Card가 없습니다. 위에서 새 카드를 추가해 보세요!</p></div>"#,
        );
    } else {
        for card in cards {
            let mut tags_html = String::new();
            for tag in &card.tags {
                tags_html.push_str(&format!(
                    r#"<span class="tag-badge">#{}</span> "#,
                    escape_html(tag)
                ));
            }

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

            let extra_examples_text = card
                .extra_examples
                .iter()
                .map(|e| {
                    if e.korean.is_empty() {
                        e.english.clone()
                    } else {
                        format!("{} | {}", e.english, e.korean)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let tags_text = card.tags.join(", ");

            cards_html.push_str(&format!(
                r#"<article class="study-card" data-card-id="{id}">
  <div class="chunk-header">
    <div>
      <span class="chunk-title">{chunk}</span>
      <span class="surface-badge" title="Surface Chunk in Context Sentence">{surface_chunk}</span>
    </div>
    <span class="stage-badge">SRS Stage {srs_stage}</span>
  </div>
  <p class="context-sentence">{context_sentence}</p>
  <p class="korean-meaning">{korean_meaning}</p>
  {examples_html}
  {nuance_html}
  <div class="tags-row">{tags_html}</div>
  <details style="margin-top: 0.75rem;">
    <summary>편집 / 삭제 (Edit / Delete)</summary>
    <form method="POST" action="/cards/{id}/edit" style="margin-top: 0.75rem;">
      <label>Chunk (원형)</label>
      <input type="text" name="chunk" value="{chunk}" required>
      <label>Surface Chunk (문장 내 활용형)</label>
      <input type="text" name="surface_chunk" value="{surface_chunk}">
      <label>Context Sentence (대표 영어 예문)</label>
      <input type="text" name="context_sentence" value="{context_sentence}" required>
      <label>한국어 의미</label>
      <input type="text" name="korean_meaning" value="{korean_meaning}" required>
      <label>추가 예문 (영어 | 한국어, 줄바꿈 구분)</label>
      <textarea name="extra_examples" rows="2">{extra_examples_text}</textarea>
      <label>뉘앙스 / 해설</label>
      <textarea name="nuance_note" rows="2">{nuance_note}</textarea>
      <label>태그 (쉼표 구분)</label>
      <input type="text" name="tags" value="{tags_text}">
      <label>SRS Stage (0..=5)</label>
      <input type="number" name="srs_stage" min="0" max="5" value="{srs_stage}">
      <button type="submit" class="btn btn-primary">수정 저장</button>
    </form>
    <form method="POST" action="/cards/{id}/delete" style="margin-top: 0.5rem;">
      <button type="submit" class="btn btn-danger">카드 삭제</button>
    </form>
  </details>
</article>"#,
                id = card.id,
                chunk = escape_html(&card.chunk),
                surface_chunk = escape_html(&card.surface_chunk),
                srs_stage = card.srs_stage,
                context_sentence = escape_html(&card.context_sentence),
                korean_meaning = escape_html(&card.korean_meaning),
                nuance_note = escape_html(&card.nuance_note),
                extra_examples_text = escape_html(&extra_examples_text),
                tags_text = escape_html(&tags_text),
            ));
        }
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>My English — Chunk &amp; SRS</title>
  <link rel="manifest" href="/manifest.json">
  <style>{APP_CSS}</style>
</head>
<body>
  <div class="container">
    <header class="navbar">
      <strong>My English</strong>
      <nav class="nav-links">
        <a href="/">홈 (Home)</a>
        <a href="/review">복습 (Review: {due_count})</a>
        <a href="/audio-gym">Audio Gym</a>
        <a href="/data">데이터 백업/가져오기</a>
        <a href="/settings">설정</a>
        <form method="POST" action="/logout" style="display:inline;margin:0;">
          <button type="submit">로그아웃</button>
        </form>
      </nav>
    </header>

    <section class="panel" id="smart-input-section">
      <h2>Smart Input (AI 통합 입력창)</h2>
      <form method="POST" action="/smart-input">
        <textarea name="input" rows="2" placeholder="한국어 질문, 교정받고 싶은 내 영작 문장, 또는 영어 지문을 입력하세요..."></textarea>
        <button type="submit" class="btn btn-primary">AI로 Draft Card 생성</button>
      </form>
    </section>

    <section class="panel review-banner">
      <div>
        <strong>오늘의 Review 대기 카드: <span id="due-card-count">{due_count}</span>장</strong>
        <p style="margin: 0.25rem 0 0; color: var(--muted); font-size: 0.9rem;">빈칸 Context Sentence를 보고 타겟 Chunk를 인출해 보세요.</p>
      </div>
      <a href="/review" class="btn btn-primary">Review 시작하기</a>
    </section>

    <section class="panel">
      <details>
        <summary><strong>+ 수동으로 Study Card 추가 (Manual Add)</strong></summary>
        <form method="POST" action="/cards" style="margin-top: 1rem;">
          <label>Chunk (원형, 예: get me every time)</label>
          <input type="text" name="chunk" required placeholder="get me every time">
          <label>Surface Chunk (문장 내 실제 활용형, 비워두면 자동 감지)</label>
          <input type="text" name="surface_chunk" placeholder="gets me every time">
          <label>Context Sentence (대표 영어 예문)</label>
          <input type="text" name="context_sentence" required placeholder="That final scene gets me every time.">
          <label>한국어 의미</label>
          <input type="text" name="korean_meaning" required placeholder="그 마지막 장면은 볼 때마다 나를 울려.">
          <label>추가 응용 예문 (영어 | 한국어, 줄바꿈 구분)</label>
          <textarea name="extra_examples" rows="2" placeholder="His dad jokes get me every time. | 그의 아재 개그는 들을 때마다 웃겨."></textarea>
          <label>AI 뉘앙스 / 해설 메모</label>
          <textarea name="nuance_note" rows="2"></textarea>
          <label>상황 태그 (쉼표로 구분)</label>
          <input type="text" name="tags" placeholder="daily, emotion">
          <button type="submit" class="btn btn-primary">Study Card 저장</button>
        </form>
      </details>
    </section>

    <section class="panel">
      <form method="GET" action="/">
        <div style="display: grid; grid-template-columns: 2fr 1fr 1fr auto; gap: 0.5rem; align-items: center;">
          <input type="search" name="q" value="{q_val}" placeholder="Chunk, Context Sentence, 한국어 의미 검색..." style="margin:0;">
          <select name="tag" style="margin:0;">{tag_options}</select>
          <select name="srs_stage" style="margin:0;">{stage_options}</select>
          <button type="submit" class="btn btn-primary">검색/필터</button>
        </div>
      </form>
    </section>

    <section class="card-grid">
      {cards_html}
    </section>
  </div>
</body>
</html>"#
    )
}
