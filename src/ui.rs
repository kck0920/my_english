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

pub const FAVICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
  <defs>
    <linearGradient id="favg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#38bdf8"/>
      <stop offset="100%" stop-color="#2563eb"/>
    </linearGradient>
  </defs>
  <rect width="32" height="32" rx="8" fill="#090d16"/>
  <rect x="0.5" y="0.5" width="31" height="31" rx="7.5" fill="none" stroke="#1e293b"/>
  <path d="M7 10C7 8.343 8.343 7 10 7H22C23.657 7 25 8.343 25 10V18C25 19.657 23.657 21 22 21H16L11 25V21H10C8.343 21 7 19.657 7 18V10Z" fill="url(#favg)"/>
  <line x1="11" y1="12.5" x2="21" y2="12.5" stroke="#ffffff" stroke-width="2.2" stroke-linecap="round"/>
  <line x1="11" y1="16.5" x2="17" y2="16.5" stroke="#ffffff" stroke-width="2.2" stroke-linecap="round"/>
</svg>"##;

pub const HEAD_META: &str = r##"<meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="theme-color" content="#090d16">
  <link rel="icon" type="image/svg+xml" href="/favicon.svg">
  <link rel="alternate icon" href="/favicon.ico">
  <link rel="apple-touch-icon" href="/favicon.svg">
  <link rel="manifest" href="/manifest.json">"##;

pub fn render_pwa_manifest() -> &'static str {
    r##"{
  "name": "My English — Chunk & SRS",
  "short_name": "MyEnglish",
  "start_url": "/",
  "display": "standalone",
  "background_color": "#090d16",
  "theme_color": "#2563eb",
  "description": "Chunk-based AI English Study & SRS App",
  "icons": [
    {
      "src": "/favicon.svg",
      "sizes": "any",
      "type": "image/svg+xml",
      "purpose": "any maskable"
    }
  ]
}"##
}

pub const ICON_LOGO: &str = r##"<svg width="24" height="24" viewBox="0 0 32 32" fill="none" style="flex-shrink:0;">
  <defs>
    <linearGradient id="nl-g" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#38bdf8"/>
      <stop offset="100%" stop-color="#2563eb"/>
    </linearGradient>
  </defs>
  <rect width="32" height="32" rx="8" fill="#131c2e"/>
  <rect x="0.5" y="0.5" width="31" height="31" rx="7.5" fill="none" stroke="#2563eb" stroke-opacity="0.35"/>
  <path d="M7 10C7 8.343 8.343 7 10 7H22C23.657 7 25 8.343 25 10V18C25 19.657 23.657 21 22 21H16L11 25V21H10C8.343 21 7 19.657 7 18V10Z" fill="url(#nl-g)"/>
  <line x1="11" y1="12.5" x2="21" y2="12.5" stroke="#ffffff" stroke-width="2.2" stroke-linecap="round"/>
  <line x1="11" y1="16.5" x2="17" y2="16.5" stroke="#ffffff" stroke-width="2.2" stroke-linecap="round"/>
</svg>"##;

pub const ICON_HOME: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>"##;
pub const ICON_REVIEW: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2"/></svg>"##;
pub const ICON_AUDIO: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 18v-6a9 9 0 0 1 18 0v6"/><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/></svg>"##;
pub const ICON_DATA: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/><path d="M3 12c0 1.66 4 3 9 3s9-1.34 9-3"/></svg>"##;
pub const ICON_SETTINGS: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" x2="20" y1="21" y2="21"/><line x1="4" x2="20" y1="14" y2="14"/><line x1="4" x2="20" y1="7" y2="7"/><circle cx="8" cy="7" r="2"/><circle cx="16" cy="14" r="2"/><circle cx="10" cy="21" r="2"/></svg>"##;
pub const ICON_LOGOUT: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" x2="9" y1="12" y2="12"/></svg>"##;
pub const ICON_SEARCH: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" x2="16.65" y1="21" y2="16.65"/></svg>"##;
pub const ICON_PLUS: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>"##;
pub const ICON_SPARKLE: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z"/></svg>"##;
pub const ICON_SPEAKER: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>"##;
pub const ICON_TAG: &str = r##"<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2H2v10l9.29 9.29c.94.94 2.48.94 3.42 0l6.58-6.58c.94-.94.94-2.48 0-3.42L12 2Z"/><circle cx="7" cy="7" r="1.5" fill="currentColor"/></svg>"##;
pub const ICON_CHECK: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>"##;

pub fn render_navbar(active: &str, due_count: Option<i64>) -> String {
    let home_cls = if active == "home" { "active" } else { "" };
    let review_cls = if active == "review" { "active" } else { "" };
    let gym_cls = if active == "gym" { "active" } else { "" };
    let data_cls = if active == "data" { "active" } else { "" };
    let settings_cls = if active == "settings" { "active" } else { "" };

    let due_badge = match due_count {
        Some(c) if c > 0 => format!(r#" <span class="count-pill">{c}</span>"#),
        Some(_) => r#" <span class="count-pill-zero">0</span>"#.to_string(),
        None => String::new(),
    };

    format!(
        r#"<header class="navbar">
      <a href="/" class="brand-logo">
        {ICON_LOGO}
        <span>My English</span>
        <span class="brand-badge">Chunk &amp; SRS</span>
      </a>
      <nav class="nav-links">
        <a href="/" class="{home_cls}">{ICON_HOME} <span>홈 (Home)</span></a>
        <a href="/review" class="{review_cls}">{ICON_REVIEW} <span>복습 (Review){due_badge}</span></a>
        <a href="/audio-gym" class="{gym_cls}">{ICON_AUDIO} <span>Audio Gym</span></a>
        <a href="/data" class="{data_cls}">{ICON_DATA} <span>데이터 백업/가져오기</span></a>
        <a href="/settings" class="{settings_cls}">{ICON_SETTINGS} <span>설정</span></a>
        <form method="POST" action="/logout" style="display:inline;margin:0;">
          <button type="submit" title="로그아웃">{ICON_LOGOUT} <span>로그아웃</span></button>
        </form>
      </nav>
    </header>"#
    )
}

pub const APP_CSS: &str = r#"
:root {
  --bg: #090d16;
  --bg-mesh: radial-gradient(at 0% 0%, rgba(37, 99, 235, 0.08) 0px, transparent 50%),
             radial-gradient(at 100% 100%, rgba(6, 182, 212, 0.05) 0px, transparent 50%);
  --surface: #111827;
  --surface-card: #131d2e;
  --surface-hover: #1a273e;
  --surface-subtle: #0b1120;
  --border: #1e293b;
  --border-hover: #2e3e58;
  --border-accent: #38bdf8;
  --text: #f8fafc;
  --text-muted: #94a3b8;
  --text-dim: #64748b;
  --primary: #2563eb;
  --primary-hover: #1d4ed8;
  --primary-light: #38bdf8;
  --accent: #10b981;
  --accent-hover: #059669;
  --warning: #f59e0b;
  --danger: #ef4444;
  --danger-hover: #dc2626;
  --radius-sm: 8px;
  --radius: 14px;
  --radius-lg: 18px;
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.3);
  --shadow: 0 4px 20px -2px rgba(0, 0, 0, 0.45);
  --shadow-lg: 0 12px 32px -4px rgba(0, 0, 0, 0.55);
}

* { box-sizing: border-box; }

body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Pretendard", "Inter", -system-ui, "Segoe UI", Roboto, sans-serif;
  background-color: var(--bg);
  background-image: var(--bg-mesh);
  background-attachment: fixed;
  color: var(--text);
  line-height: 1.6;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

.container {
  max-width: 920px;
  margin: 0 auto;
  padding: 1rem 1.25rem 3rem;
}

header.navbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  padding: 0.85rem 1.25rem;
  position: sticky;
  top: 0;
  z-index: 100;
  background: rgba(9, 13, 22, 0.88);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border-bottom: 1px solid var(--border);
  margin-bottom: 1.5rem;
}

.brand-logo {
  display: inline-flex;
  align-items: center;
  gap: 0.65rem;
  text-decoration: none;
  color: var(--text);
  font-weight: 700;
  font-size: 1.15rem;
  letter-spacing: -0.02em;
}

.brand-logo .brand-badge {
  font-size: 0.7rem;
  font-weight: 600;
  color: var(--primary-light);
  background: rgba(56, 189, 248, 0.1);
  border: 1px solid rgba(56, 189, 248, 0.25);
  padding: 2px 7px;
  border-radius: 999px;
  margin-left: 2px;
}

.nav-links {
  display: flex;
  gap: 0.45rem;
  align-items: center;
  flex-wrap: wrap;
}

.nav-links a, .nav-links button, .btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.45rem;
  background: var(--surface-card);
  color: var(--text);
  border: 1px solid var(--border);
  padding: 0.5rem 0.85rem;
  border-radius: var(--radius-sm);
  text-decoration: none;
  cursor: pointer;
  font-size: 0.875rem;
  font-weight: 500;
  transition: all 0.18s cubic-bezier(0.16, 1, 0.3, 1);
}

.nav-links a:hover, .nav-links button:hover, .btn:hover {
  background: var(--surface-hover);
  border-color: var(--border-hover);
  color: #fff;
  transform: translateY(-1px);
}

.nav-links a:active, .nav-links button:active, .btn:active {
  transform: translateY(1px);
}

.nav-links a.active {
  background: rgba(37, 99, 235, 0.15);
  border-color: rgba(56, 189, 248, 0.4);
  color: var(--primary-light);
  font-weight: 600;
}

.count-pill {
  background: var(--accent);
  color: #fff;
  font-size: 0.72rem;
  font-weight: 700;
  padding: 1px 6px;
  border-radius: 999px;
  line-height: 1.2;
}

#due-card-count {
  background: var(--accent);
  color: #fff;
  font-size: 0.8rem;
  font-weight: 700;
  padding: 1px 7px;
  border-radius: 999px;
  margin: 0 2px;
}

.count-pill-zero {
  background: rgba(148, 163, 184, 0.2);
  color: var(--text-muted);
  font-size: 0.72rem;
  font-weight: 600;
  padding: 1px 6px;
  border-radius: 999px;
  line-height: 1.2;
}

.btn-primary {
  background: linear-gradient(135deg, #2563eb, #1d4ed8);
  border: 1px solid #3b82f6;
  color: #fff !important;
  font-weight: 600;
  box-shadow: 0 2px 8px rgba(37, 99, 235, 0.35);
}

.btn-primary:hover {
  background: linear-gradient(135deg, #3b82f6, #2563eb);
  border-color: #60a5fa;
  box-shadow: 0 4px 14px rgba(37, 99, 235, 0.5);
}

.btn-accent {
  background: linear-gradient(135deg, #10b981, #059669);
  border: 1px solid #34d399;
  color: #fff !important;
  font-weight: 600;
  box-shadow: 0 2px 8px rgba(16, 185, 129, 0.35);
}

.btn-accent:hover {
  background: linear-gradient(135deg, #34d399, #10b981);
  box-shadow: 0 4px 14px rgba(16, 185, 129, 0.5);
}

.btn-danger {
  background: transparent;
  border-color: rgba(239, 68, 68, 0.4);
  color: #f87171 !important;
}

.btn-danger:hover {
  background: rgba(239, 68, 68, 0.12);
  border-color: #ef4444;
  color: #fff !important;
}

.panel {
  background: var(--surface-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 1.25rem 1.5rem;
  margin-bottom: 1.5rem;
  box-shadow: var(--shadow);
  transition: border-color 0.2s ease;
}

.panel:hover {
  border-color: var(--border-hover);
}

.review-banner {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  flex-wrap: wrap;
  background: linear-gradient(135deg, rgba(16, 185, 129, 0.08), rgba(6, 182, 212, 0.04));
  border: 1px solid rgba(16, 185, 129, 0.3);
  border-left: 4px solid var(--accent);
}

.card-grid {
  display: grid;
  gap: 1.15rem;
}

.study-card {
  background: var(--surface-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 1.25rem 1.4rem;
  box-shadow: var(--shadow);
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease;
}

.study-card:hover {
  border-color: rgba(56, 189, 248, 0.35);
  box-shadow: 0 6px 20px -2px rgba(0, 0, 0, 0.5);
}

.chunk-header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 0.65rem;
  flex-wrap: wrap;
}

.chunk-title {
  font-size: 1.25rem;
  font-weight: 700;
  color: var(--primary-light);
  letter-spacing: -0.01em;
}

.surface-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 0.78rem;
  font-weight: 600;
  background: rgba(56, 189, 248, 0.1);
  border: 1px solid rgba(56, 189, 248, 0.25);
  color: var(--primary-light);
}

.stage-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 0.75rem;
  font-weight: 700;
  background: rgba(148, 163, 184, 0.1);
  border: 1px solid rgba(148, 163, 184, 0.25);
  color: var(--text-muted);
  letter-spacing: 0.03em;
}

.stage-badge[data-stage="0"] { background: rgba(148, 163, 184, 0.1); color: #94a3b8; border-color: rgba(148, 163, 184, 0.25); }
.stage-badge[data-stage="1"] { background: rgba(52, 211, 153, 0.1); color: #34d399; border-color: rgba(52, 211, 153, 0.25); }
.stage-badge[data-stage="2"] { background: rgba(56, 189, 248, 0.1); color: #38bdf8; border-color: rgba(56, 189, 248, 0.25); }
.stage-badge[data-stage="3"] { background: rgba(96, 165, 250, 0.1); color: #60a5fa; border-color: rgba(96, 165, 250, 0.25); }
.stage-badge[data-stage="4"] { background: rgba(59, 130, 246, 0.12); color: #93c5fd; border-color: rgba(59, 130, 246, 0.3); }
.stage-badge[data-stage="5"] { background: rgba(251, 191, 36, 0.1); color: #fbbf24; border-color: rgba(251, 191, 36, 0.25); }

.tag-badge {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 2px 8px;
  border-radius: 6px;
  font-size: 0.78rem;
  background: var(--surface-subtle);
  border: 1px solid var(--border);
  color: var(--text-muted);
}

.tags-row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
  margin-top: 0.5rem;
}

.context-sentence {
  font-size: 1.05rem;
  line-height: 1.6;
  color: #e2e8f0;
  margin: 0.75rem 0 0.4rem;
}

.korean-meaning {
  font-size: 0.95rem;
  color: var(--text-muted);
  margin: 0 0 0.75rem;
  font-weight: 500;
}

.extra-examples {
  margin: 0.5rem 0;
  padding-left: 1.25rem;
  font-size: 0.9rem;
  color: #cbd5e1;
}

.extra-examples li {
  margin-bottom: 0.25rem;
}

.nuance-note {
  background: rgba(11, 17, 32, 0.7);
  border-left: 3px solid var(--primary-light);
  border-radius: 4px;
  padding: 0.5rem 0.75rem;
  font-size: 0.88rem;
  color: #cbd5e1;
  margin: 0.6rem 0;
}

input, textarea, select {
  width: 100%;
  background: var(--surface-subtle);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: 0.65rem 0.85rem;
  font-size: 0.925rem;
  margin-top: 0.35rem;
  margin-bottom: 0.85rem;
  font-family: inherit;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

input:focus, textarea:focus, select:focus {
  outline: none;
  border-color: var(--primary-light);
  box-shadow: 0 0 0 3px rgba(56, 189, 248, 0.2);
}

label {
  display: block;
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--text-muted);
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

h1, h2, h3 {
  letter-spacing: -0.02em;
  font-weight: 700;
}

details {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface-subtle);
  padding: 0.65rem 0.9rem;
}

details summary {
  cursor: pointer;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text-muted);
  user-select: none;
}

details summary:hover {
  color: var(--text);
}

.alert {
  padding: 0.85rem 1.15rem;
  border-radius: var(--radius-sm);
  margin-bottom: 1.25rem;
  font-size: 0.9rem;
  font-weight: 500;
}

.alert-error {
  background: rgba(239, 68, 68, 0.12);
  border: 1px solid rgba(239, 68, 68, 0.35);
  color: #fca5a5;
}

.alert-success {
  background: rgba(16, 185, 129, 0.12);
  border: 1px solid rgba(16, 185, 129, 0.35);
  color: #6ee7b7;
}

@media (max-width: 640px) {
  header.navbar {
    flex-direction: column;
    align-items: stretch;
    padding: 0.75rem 1rem;
  }
  .nav-links {
    justify-content: flex-start;
    overflow-x: auto;
    padding-bottom: 0.25rem;
  }
  .container {
    padding: 0.75rem 1rem 2rem;
  }
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
  {HEAD_META}
  <title>로그인 — My English</title>
  <style>{APP_CSS}</style>
</head>
<body>
  <main class="container">
    <section class="panel" style="max-width: 400px; margin: 5rem auto; padding: 2.25rem; text-align: center; border-color: rgba(56, 189, 248, 0.25); box-shadow: var(--shadow-lg);">
      <div style="display:flex; justify-content:center; margin-bottom: 1rem;">
        {ICON_LOGO}
      </div>
      <h1 style="margin: 0 0 0.5rem; font-size: 1.75rem; letter-spacing: -0.03em;">My English</h1>
      <p style="margin: 0 0 1.75rem; color: var(--text-muted); font-size: 0.92rem;">나만의 영어 Chunk 수집 &amp; 간격 반복(SRS) 학습장</p>
      {error_html}
      <form method="POST" action="/login" style="text-align: left;">
        <label for="passcode">Passcode</label>
        <input type="password" id="passcode" name="passcode" required autofocus placeholder="패스코드 (기본값: 1234)">
        <button type="submit" class="btn btn-primary" style="width:100%; margin-top: 0.5rem; padding: 0.7rem;">로그인</button>
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
            r#"<div class="panel" style="text-align:center; padding: 3rem 1.5rem; color: var(--text-muted);"><p style="font-size:1.05rem; margin:0 0 0.5rem;">저장된 Study Card가 없습니다.</p><p style="font-size:0.9rem; margin:0;">위의 Smart Input으로 표현을 추천받거나 수동으로 새 카드를 추가해 보세요!</p></div>"#,
        );
    } else {
        for card in cards {
            let mut tags_html = String::new();
            for tag in &card.tags {
                tags_html.push_str(&format!(
                    r#"<span class="tag-badge">{ICON_TAG} #{}</span> "#,
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
    <div style="display:flex; align-items:center; gap:0.5rem; flex-wrap:wrap;">
      <span class="chunk-title">{chunk}</span>
      <span class="surface-badge" title="Surface Chunk in Context Sentence">{surface_chunk}</span>
    </div>
    <span class="stage-badge" data-stage="{srs_stage}">SRS Stage {srs_stage}</span>
  </div>
  <p class="context-sentence">{context_sentence}</p>
  <p class="korean-meaning">{korean_meaning}</p>
  {examples_html}
  {nuance_html}
  <div class="tags-row">{tags_html}</div>
  <details style="margin-top: 0.85rem;">
    <summary>편집 / 삭제 (Edit / Delete)</summary>
    <form method="POST" action="/cards/{id}/edit" style="margin-top: 0.85rem;">
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
      <button type="submit" class="btn btn-primary" style="margin-right: 0.5rem;">수정 저장</button>
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

    let navbar = render_navbar("home", Some(due_count));

    format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
  {HEAD_META}
  <title>My English — Chunk &amp; SRS</title>
  <style>{APP_CSS}</style>
</head>
<body>
  {navbar}
  <div class="container">
    <section class="panel" id="smart-input-section" style="border-color: rgba(56, 189, 248, 0.25);">
      <h2 style="margin: 0 0 0.5rem; display: flex; align-items: center; gap: 0.5rem; font-size: 1.15rem;">
        {ICON_SPARKLE}
        <span>Smart Input (AI 통합 입력창)</span>
      </h2>
      <p style="margin: 0 0 0.75rem; color: var(--text-muted); font-size: 0.88rem;">한국어 질문, 교정받고 싶은 내 영작 문장, 또는 영어 지문을 입력하면 AI가 핵심 Chunk를 추출해 Draft Card를 자동 생성합니다.</p>
      <form method="POST" action="/smart-input">
        <textarea name="input" rows="2" placeholder="예: take back / I gave him the benefit of the doubt / 그 영화 결말 볼 때마다 울컥해"></textarea>
        <button type="submit" class="btn btn-primary">{ICON_SPARKLE} <span>AI로 Draft Card 생성</span></button>
      </form>
    </section>

    <section class="panel review-banner">
      <div>
        <strong style="font-size: 1.05rem;">오늘의 Review 대기 카드: <span id="due-card-count">{due_count}</span>장</strong>
        <p style="margin: 0.35rem 0 0; color: var(--text-muted); font-size: 0.9rem;">빈칸 Context Sentence를 보고 타겟 Chunk를 인출하는 Cloze 테스트를 진행합니다.</p>
      </div>
      <a href="/review" class="btn btn-accent">{ICON_REVIEW} <span>Review 시작하기</span></a>
    </section>

    <section class="panel">
      <details>
        <summary style="display:flex; align-items:center; gap:0.4rem;">
          {ICON_PLUS}
          <span>수동으로 Study Card 추가 (Manual Add)</span>
        </summary>
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
          <button type="submit" class="btn btn-primary">{ICON_PLUS} <span>Study Card 저장</span></button>
        </form>
      </details>
    </section>

    <section class="panel" style="padding: 1rem 1.25rem;">
      <form method="GET" action="/">
        <div style="display: grid; grid-template-columns: 2fr 1fr 1fr auto; gap: 0.5rem; align-items: center;">
          <input type="search" name="q" value="{q_val}" placeholder="Chunk, 예문, 한국어 의미 검색..." style="margin:0;">
          <select name="tag" style="margin:0;">{tag_options}</select>
          <select name="srs_stage" style="margin:0;">{stage_options}</select>
          <button type="submit" class="btn btn-primary">{ICON_SEARCH} <span>검색/필터</span></button>
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
