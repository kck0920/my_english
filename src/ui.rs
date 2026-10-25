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

pub const FAVICON_SVG: &str = r##"<svg aria-hidden="true" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
  <defs>
    <linearGradient id="fav-bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#3b82f6"/>
      <stop offset="100%" stop-color="#1d4ed8"/>
    </linearGradient>
    <linearGradient id="fav-border" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.45"/>
      <stop offset="100%" stop-color="#ffffff" stop-opacity="0.08"/>
    </linearGradient>
  </defs>
  <rect width="32" height="32" rx="9" fill="url(#fav-bg)"/>
  <rect x="0.75" y="0.75" width="30.5" height="30.5" rx="8.25" stroke="url(#fav-border)" stroke-width="1.5"/>
  <rect x="8.5" y="11.5" width="3.5" height="9" rx="1.75" fill="#ffffff" fill-opacity="0.88"/>
  <rect x="14.25" y="7.5" width="3.5" height="17" rx="1.75" fill="#ffffff"/>
  <rect x="20" y="10.5" width="3.5" height="11" rx="1.75" fill="#ffffff" fill-opacity="0.92"/>
</svg>"##;

pub const HEAD_META: &str = r##"<meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="description" content="My English: 나만의 영어 Chunk 수집 및 간격 반복(SRS) 학습장">
  <meta name="theme-color" content="#090d16">
  <link rel="icon" type="image/svg+xml" href="/favicon.svg">
  <link rel="alternate icon" href="/favicon.ico">
  <link rel="apple-touch-icon" href="/favicon.svg">
  <link rel="manifest" href="/manifest.json">
  <script>
    (function() {
      try {
        var t = localStorage.getItem('my_english_theme') || 'obsidian';
        document.documentElement.setAttribute('data-theme', t);
      } catch (e) {}
    })();
    function switchTheme(t) {
      try {
        document.documentElement.setAttribute('data-theme', t);
        localStorage.setItem('my_english_theme', t);
        var sels = document.querySelectorAll('.theme-select-input');
        sels.forEach(function(s) { s.value = t; });
        var cards = document.querySelectorAll('.theme-card-option');
        cards.forEach(function(c) {
          if (c.getAttribute('data-theme-val') === t) {
            c.classList.add('active');
          } else {
            c.classList.remove('active');
          }
        });
      } catch (e) {}
    }
    window.switchTheme = switchTheme;

    function getPreferredVoiceGender() {
      try {
        return localStorage.getItem('my_english_voice_gender') || 'female';
      } catch (e) {
        return 'female';
      }
    }
    function switchVoiceGender(g) {
      try {
        localStorage.setItem('my_english_voice_gender', g);
        var sels = document.querySelectorAll('.voice-select-input');
        sels.forEach(function(s) { s.value = g; });
        var gymSel = document.getElementById('genderSelect');
        if (gymSel) gymSel.value = g;
      } catch (e) {}
    }
    window.getPreferredVoiceGender = getPreferredVoiceGender;
    window.switchVoiceGender = switchVoiceGender;

    window.playSentenceAudio = function(btn, text) {
      if (!text || !text.trim()) return;
      var clean = text.trim();
      var gender = getPreferredVoiceGender();
      var origHtml = btn ? btn.innerHTML : '';
      if (btn) {
        btn.disabled = true;
        btn.classList.add('playing');
        btn.setAttribute('data-orig-html', origHtml);
        btn.innerHTML = '<span class="audio-spinner"></span> <span>재생중...</span>';
      }

      var done = false;
      var finish = function() {
        if (!done) {
          done = true;
          if (btn) {
            btn.disabled = false;
            btn.classList.remove('playing');
            btn.innerHTML = btn.getAttribute('data-orig-html') || origHtml;
          }
        }
      };

      var timer = setTimeout(finish, 8000);

      var fallbackSpeech = function() {
        clearTimeout(timer);
        if ('speechSynthesis' in window) {
          try {
            window.speechSynthesis.cancel();
            var u = new SpeechSynthesisUtterance(clean);
            u.lang = 'en-US';
            u.rate = 0.95;
            if (gender === 'male') {
              var voices = window.speechSynthesis.getVoices();
              var maleVoice = voices.find(function(v) {
                return v.lang.startsWith('en') && (v.name.includes('Male') || v.name.includes('David') || v.name.includes('Guy') || v.name.includes('Matthew'));
              });
              if (maleVoice) u.voice = maleVoice;
            }
            u.onend = finish;
            u.onerror = finish;
            window.speechSynthesis.speak(u);
          } catch (e) {
            finish();
          }
        } else {
          finish();
        }
      };

      try {
        if (!window._globalAudioPlayer) {
          window._globalAudioPlayer = new Audio();
        }
        var audio = window._globalAudioPlayer;
        audio.pause();
        audio.onended = function() {
          clearTimeout(timer);
          finish();
        };
        audio.onerror = function() {
          clearTimeout(timer);
          fallbackSpeech();
        };

        audio.src = '/api/tts?text=' + encodeURIComponent(clean) + '&lang=en&gender=' + encodeURIComponent(gender);
        var p = audio.play();
        if (p !== undefined) {
          p.catch(function(err) {
            console.warn('Audio play failed, falling back to Web Speech:', err);
            fallbackSpeech();
          });
        }
      } catch (err) {
        fallbackSpeech();
      }
    };

    document.addEventListener('DOMContentLoaded', function() {
      try {
        var t = document.documentElement.getAttribute('data-theme') || 'obsidian';
        var sels = document.querySelectorAll('.theme-select-input');
        sels.forEach(function(s) { s.value = t; });
        var cards = document.querySelectorAll('.theme-card-option');
        cards.forEach(function(c) {
          if (c.getAttribute('data-theme-val') === t) {
            c.classList.add('active');
          } else {
            c.classList.remove('active');
          }
        });

        var g = getPreferredVoiceGender();
        var vSels = document.querySelectorAll('.voice-select-input');
        vSels.forEach(function(s) { s.value = g; });
        var gymSel = document.getElementById('genderSelect');
        if (gymSel) gymSel.value = g;
      } catch (e) {}
    });
  </script>"##;

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

pub const ICON_LOGO: &str = r##"<svg aria-hidden="true" width="24" height="24" viewBox="0 0 32 32" fill="none" style="flex-shrink:0;">
  <defs>
    <linearGradient id="nl-bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#3b82f6"/>
      <stop offset="100%" stop-color="#1d4ed8"/>
    </linearGradient>
    <linearGradient id="nl-border" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.45"/>
      <stop offset="100%" stop-color="#ffffff" stop-opacity="0.08"/>
    </linearGradient>
  </defs>
  <rect width="32" height="32" rx="9" fill="url(#nl-bg)"/>
  <rect x="0.75" y="0.75" width="30.5" height="30.5" rx="8.25" stroke="url(#nl-border)" stroke-width="1.5"/>
  <rect x="8.5" y="11.5" width="3.5" height="9" rx="1.75" fill="#ffffff" fill-opacity="0.88"/>
  <rect x="14.25" y="7.5" width="3.5" height="17" rx="1.75" fill="#ffffff"/>
  <rect x="20" y="10.5" width="3.5" height="11" rx="1.75" fill="#ffffff" fill-opacity="0.92"/>
</svg>"##;

pub const ICON_LOGO_LARGE: &str = r##"<svg aria-hidden="true" width="56" height="56" viewBox="0 0 32 32" fill="none" style="flex-shrink:0; filter: drop-shadow(0 10px 25px rgba(37, 99, 235, 0.45));">
  <defs>
    <linearGradient id="nll-bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#3b82f6"/>
      <stop offset="100%" stop-color="#1d4ed8"/>
    </linearGradient>
    <linearGradient id="nll-border" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.5"/>
      <stop offset="100%" stop-color="#ffffff" stop-opacity="0.1"/>
    </linearGradient>
  </defs>
  <rect width="32" height="32" rx="9" fill="url(#nll-bg)"/>
  <rect x="0.75" y="0.75" width="30.5" height="30.5" rx="8.25" stroke="url(#nll-border)" stroke-width="1.5"/>
  <rect x="8.5" y="11.5" width="3.5" height="9" rx="1.75" fill="#ffffff" fill-opacity="0.88"/>
  <rect x="14.25" y="7.5" width="3.5" height="17" rx="1.75" fill="#ffffff"/>
  <rect x="20" y="10.5" width="3.5" height="11" rx="1.75" fill="#ffffff" fill-opacity="0.92"/>
</svg>"##;

pub const ICON_HOME: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>"##;
pub const ICON_REVIEW: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2"/></svg>"##;
pub const ICON_AUDIO: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 18v-6a9 9 0 0 1 18 0v6"/><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/></svg>"##;
pub const ICON_DATA: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/><path d="M3 12c0 1.66 4 3 9 3s9-1.34 9-3"/></svg>"##;
pub const ICON_SETTINGS: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" x2="20" y1="21" y2="21"/><line x1="4" x2="20" y1="14" y2="14"/><line x1="4" x2="20" y1="7" y2="7"/><circle cx="8" cy="7" r="2"/><circle cx="16" cy="14" r="2"/><circle cx="10" cy="21" r="2"/></svg>"##;
pub const ICON_LOGOUT: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" x2="9" y1="12" y2="12"/></svg>"##;
pub const ICON_SEARCH: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" x2="16.65" y1="21" y2="16.65"/></svg>"##;
pub const ICON_PLUS: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>"##;
pub const ICON_SPARKLE: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z"/></svg>"##;
pub const ICON_SPEAKER: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>"##;
pub const ICON_MIC: &str = r##"<svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0;"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>"##;
pub const ICON_TAG: &str = r##"<svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2H2v10l9.29 9.29c.94.94 2.48.94 3.42 0l6.58-6.58c.94-.94.94-2.48 0-3.42L12 2Z"/><circle cx="7" cy="7" r="1.5" fill="currentColor"/></svg>"##;
pub const ICON_CHECK: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>"##;
pub const ICON_PALETTE: &str = r##"<svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="13.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".5" fill="currentColor"/><circle cx="8.5" cy="7.5" r=".5" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".5" fill="currentColor"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.554C21.965 6.012 17.461 2 12 2z"/></svg>"##;

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
      <div class="navbar-brand-wrap">
        <a href="/" class="brand-logo">
          {ICON_LOGO}
          <span>My English</span>
          <span class="brand-badge">Chunk &amp; SRS</span>
        </a>
      </div>
      <nav class="nav-links">
        <a href="/" class="{home_cls}">{ICON_HOME} <span>홈<span class="nav-label-long"> (Home)</span></span></a>
        <a href="/review" class="{review_cls}">{ICON_REVIEW} <span>복습<span class="nav-label-long"> (Review)</span>{due_badge}</span></a>
        <a href="/audio-gym" class="{gym_cls}">{ICON_AUDIO} <span>Audio Gym</span></a>
        <a href="/data" class="{data_cls}">{ICON_DATA} <span>데이터<span class="nav-label-long"> 백업/가져오기</span></span></a>
        <a href="/settings" class="{settings_cls}">{ICON_SETTINGS} <span>설정</span></a>
      </nav>
      <div class="navbar-actions">
        <div class="voice-picker" title="TTS 음성(보이스) 선택">
          <span class="voice-picker-icon">{ICON_MIC}</span>
          <select class="voice-select-input" onchange="window.switchVoiceGender(this.value)" aria-label="TTS 음성 선택">
            <option value="female">여성 음성</option>
            <option value="male">남성 음성</option>
          </select>
        </div>
        <div class="theme-picker" title="테마 선택">
          {ICON_PALETTE}
          <select class="theme-select-input" onchange="window.switchTheme(this.value)" aria-label="테마 선택">
            <option value="obsidian">옵시디언</option>
            <option value="nordic">노르딕</option>
            <option value="paper">페이퍼</option>
            <option value="sunset">선셋</option>
            <option value="terminal">터미널</option>
          </select>
        </div>
        <form method="POST" action="/logout" style="display:inline;margin:0;">
          <button type="submit" class="navbar-logout-btn" title="로그아웃">{ICON_LOGOUT}<span class="logout-text">로그아웃</span></button>
        </form>
      </div>
    </header>"#
    )
}

pub const APP_CSS: &str = r#"/* Hallmark · v2.0.0 · redesign: full · scope: app · theme: obsidian · genre: modern-minimal */
/* Hallmark · pre-emit critique: P4 H5 E5 S4 R5 V4 · weakest: P, app screen functional layout prioritized over editorial voice */
/* Hallmark · waive 7b · surface: absolute · terminal and obsidian themes use deliberate dark solid canvas */
/* Hallmark · answered F3 · display heads in panels are concise and balance automatically */

/* ==========================================================================
   MULTI-THEME PALETTES (5 Distinct Aesthetic Themes)
   ========================================================================== */

:root, [data-theme="obsidian"] {
  --theme-name: "obsidian";
  --font-body: -apple-system, BlinkMacSystemFont, "Pretendard", "Inter", -system-ui, "Segoe UI", Roboto, sans-serif;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  --bg: #090d16;
  --bg-mesh: radial-gradient(at 0% 0%, rgba(37, 99, 235, 0.09) 0px, transparent 50%),
             radial-gradient(at 100% 100%, rgba(6, 182, 212, 0.05) 0px, transparent 50%);
  --surface: #111827;
  --surface-card: #131d2e;
  --surface-hover: #1a273e;
  --surface-subtle: #0b1120;
  --border: oklch(0.24 0.02 250);
  --border-hover: #2e3e58;
  --border-accent: #38bdf8;
  --text: oklch(0.97 0.005 250);
  --text-hover: #ffffff;
  --text-muted: #94a3b8;
  --text-dim: oklch(0.55 0.02 250);
  --primary: #2563eb;
  --primary-hover: #1d4ed8;
  --primary-light: #38bdf8;
  --accent: #10b981;
  --accent-hover: #059669;
  --accent-ink: #07130e;
  --warning: #f59e0b;
  --danger: #ef4444;
  --danger-hover: #dc2626;
  --navbar-bg: #0d1424;
  --btn-primary-bg: linear-gradient(135deg, #2563eb, #1d4ed8);
  --btn-primary-hover: linear-gradient(135deg, oklch(0.58 0.18 250), #2563eb);
  --btn-primary-border: oklch(0.58 0.18 250);
  --btn-accent-bg: linear-gradient(135deg, #10b981, #059669);
  --btn-accent-hover: linear-gradient(135deg, #34d399, #10b981);
  --btn-accent-border: #34d399;
  --badge-bg: rgba(56, 189, 248, 0.1);
  --badge-border: rgba(56, 189, 248, 0.25);
  --nuance-bg: rgba(11, 17, 32, 0.7);
  --review-banner-bg: linear-gradient(135deg, rgba(16, 185, 129, 0.08), rgba(6, 182, 212, 0.04));
  --radius-sm: 6px;
  --radius: 12px;
  --radius-lg: 16px;
  --radius-pill: 999px;
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.3);
  --shadow: 0 4px 20px -2px rgba(0, 0, 0, 0.45);
  --shadow-lg: 0 12px 32px -4px rgba(0, 0, 0, 0.55);
  --white-ink: oklch(0.98 0.005 250);
  --white-soft: #f1f5f9;
  --dark-ink: #0b0f17;
  --danger-surface: rgba(239, 68, 68, 0.12);
  --danger-border: rgba(239, 68, 68, 0.35);
  --danger-ink: #fca5a5;
  --success-surface: rgba(16, 185, 129, 0.12);
  --success-border: rgba(16, 185, 129, 0.35);
  --success-ink: #6ee7b7;
  --playing-surface: rgba(16, 185, 129, 0.1);
  --stage0-bg: rgba(148, 163, 184, 0.1);
  --stage0-border: rgba(148, 163, 184, 0.25);
  --stage0-color: #94a3b8;
  --stage1-bg: rgba(52, 211, 153, 0.1);
  --stage1-border: rgba(52, 211, 153, 0.25);
  --stage1-color: #34d399;
  --stage2-bg: rgba(56, 189, 248, 0.1);
  --stage2-border: rgba(56, 189, 248, 0.25);
  --stage2-color: #38bdf8;
  --stage3-bg: rgba(96, 165, 250, 0.1);
  --stage3-border: rgba(96, 165, 250, 0.25);
  --stage3-color: #60a5fa;
  --stage4-bg: rgba(59, 130, 246, 0.12);
  --stage4-border: rgba(59, 130, 246, 0.3);
  --stage4-color: #93c5fd;
  --stage5-bg: rgba(251, 191, 36, 0.1);
  --stage5-border: rgba(251, 191, 36, 0.25);
  --stage5-color: #fbbf24;
  --focus-ring: rgba(37, 99, 235, 0.2);
  --card-border-subtle: rgba(128, 128, 128, 0.2);
  --card-dot-border: rgba(0, 0, 0, 0.15);
}

[data-theme="nordic"] {
  --theme-name: "nordic";
  --bg: #09130e;
  --bg-mesh: radial-gradient(at 0% 0%, rgba(16, 185, 129, 0.1) 0px, transparent 50%),
             radial-gradient(at 100% 100%, rgba(5, 150, 105, 0.06) 0px, transparent 50%);
  --surface: #0f1e16;
  --surface-card: #13241b;
  --surface-hover: #1b3225;
  --surface-subtle: #0b1711;
  --border: #1e3b2c;
  --border-hover: #2d5540;
  --border-accent: #34d399;
  --text: #f0f7f4;
  --text-hover: #ffffff;
  --text-muted: #95ad9e;
  --text-dim: #5c7466;
  --primary: #059669;
  --primary-hover: #047857;
  --primary-light: #34d399;
  --accent: #eab308;
  --accent-hover: #ca8a04;
  --accent-ink: #161003;
  --warning: #f59e0b;
  --danger: #f87171;
  --danger-hover: #ef4444;
  --navbar-bg: #0c1812;
  --btn-primary-bg: linear-gradient(135deg, #059669, #047857);
  --btn-primary-hover: linear-gradient(135deg, #10b981, #059669);
  --btn-primary-border: #10b981;
  --btn-accent-bg: linear-gradient(135deg, #eab308, #ca8a04);
  --btn-accent-hover: linear-gradient(135deg, #facc15, #eab308);
  --btn-accent-border: #facc15;
  --badge-bg: rgba(52, 211, 153, 0.1);
  --badge-border: rgba(52, 211, 153, 0.25);
  --nuance-bg: rgba(11, 23, 17, 0.7);
  --review-banner-bg: linear-gradient(135deg, rgba(5, 150, 105, 0.12), rgba(234, 179, 8, 0.05));
  --radius-sm: 6px;
  --radius: 12px;
  --radius-lg: 16px;
  --radius-pill: 999px;
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.35);
  --shadow: 0 4px 20px -2px rgba(0, 0, 0, 0.5);
  --shadow-lg: 0 12px 32px -4px rgba(0, 0, 0, 0.6);
  --white-ink: #f0f7f4;
  --white-soft: #e2ede7;
  --dark-ink: #070f0b;
  --danger-surface: rgba(248, 113, 113, 0.12);
  --danger-border: rgba(248, 113, 113, 0.35);
  --danger-ink: #fca5a5;
  --success-surface: rgba(52, 211, 153, 0.12);
  --success-border: rgba(52, 211, 153, 0.35);
  --success-ink: #6ee7b7;
  --playing-surface: rgba(52, 211, 153, 0.1);
  --stage0-bg: rgba(149, 173, 158, 0.1);
  --stage0-border: rgba(149, 173, 158, 0.25);
  --stage0-color: #95ad9e;
  --stage1-bg: rgba(52, 211, 153, 0.1);
  --stage1-border: rgba(52, 211, 153, 0.25);
  --stage1-color: #34d399;
  --stage2-bg: rgba(56, 189, 248, 0.1);
  --stage2-border: rgba(56, 189, 248, 0.25);
  --stage2-color: #38bdf8;
  --stage3-bg: rgba(96, 165, 250, 0.1);
  --stage3-border: rgba(96, 165, 250, 0.25);
  --stage3-color: #60a5fa;
  --stage4-bg: rgba(59, 130, 246, 0.12);
  --stage4-border: rgba(59, 130, 246, 0.3);
  --stage4-color: #93c5fd;
  --stage5-bg: rgba(251, 191, 36, 0.1);
  --stage5-border: rgba(251, 191, 36, 0.25);
  --stage5-color: #fbbf24;
  --focus-ring: rgba(52, 211, 153, 0.2);
  --card-border-subtle: rgba(128, 128, 128, 0.2);
  --card-dot-border: rgba(0, 0, 0, 0.15);
}

[data-theme="paper"] {
  --theme-name: "paper";
  --bg: #f8f6f0;
  --bg-mesh: radial-gradient(at 0% 0%, rgba(180, 83, 9, 0.035) 0px, transparent 50%),
             radial-gradient(at 100% 100%, rgba(37, 99, 235, 0.025) 0px, transparent 50%);
  --surface: #ffffff;
  --surface-card: #ffffff;
  --surface-hover: #f1efe6;
  --surface-subtle: #eeece2;
  --border: #dfdbce;
  --border-hover: #c7c1b0;
  --border-accent: #2563eb;
  --text: #1a1714;
  --text-hover: #000000;
  --text-muted: #57524a;
  --text-dim: #847d73;
  --primary: #2563eb;
  --primary-hover: #1d4ed8;
  --primary-light: #1d4ed8;
  --accent: #d97706;
  --accent-hover: #b45309;
  --accent-ink: #ffffff;
  --warning: #d97706;
  --danger: #dc2626;
  --danger-hover: #b91c1c;
  --navbar-bg: #f5f2e9;
  --btn-primary-bg: linear-gradient(135deg, #2563eb, #1d4ed8);
  --btn-primary-hover: linear-gradient(135deg, #1d4ed8, #1e40af);
  --btn-primary-border: #1d4ed8;
  --btn-accent-bg: linear-gradient(135deg, #d97706, #b45309);
  --btn-accent-hover: linear-gradient(135deg, #b45309, #92400e);
  --btn-accent-border: #d97706;
  --badge-bg: rgba(37, 99, 235, 0.08);
  --badge-border: rgba(37, 99, 235, 0.2);
  --nuance-bg: #f2f0e6;
  --review-banner-bg: linear-gradient(135deg, rgba(37, 99, 235, 0.06), rgba(217, 119, 6, 0.04));
  --radius-sm: 6px;
  --radius: 12px;
  --radius-lg: 16px;
  --radius-pill: 999px;
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.06);
  --shadow: 0 4px 16px -2px rgba(0, 0, 0, 0.07);
  --shadow-lg: 0 10px 28px -4px rgba(0, 0, 0, 0.1);
  --white-ink: #ffffff;
  --white-soft: #fcfbf7;
  --dark-ink: #1a1714;
  --danger-surface: #fef2f2;
  --danger-border: #fecaca;
  --danger-ink: #b91c1c;
  --success-surface: #ecfdf5;
  --success-border: #a7f3d0;
  --success-ink: #047857;
  --playing-surface: rgba(37, 99, 235, 0.08);
  --stage0-bg: #f1f5f9;
  --stage0-border: #cbd5e1;
  --stage0-color: #475569;
  --stage1-bg: #ecfdf5;
  --stage1-border: #a7f3d0;
  --stage1-color: #047857;
  --stage2-bg: #f0f9ff;
  --stage2-border: #bae6fd;
  --stage2-color: #0369a1;
  --stage3-bg: #eff6ff;
  --stage3-border: #bfdbfe;
  --stage3-color: #1d4ed8;
  --stage4-bg: #e0f2fe;
  --stage4-border: #7dd3fc;
  --stage4-color: #0284c7;
  --stage5-bg: #fffbeb;
  --stage5-border: #fde68a;
  --stage5-color: #b45309;
  --focus-ring: rgba(37, 99, 235, 0.2);
  --card-border-subtle: rgba(128, 128, 128, 0.2);
  --card-dot-border: rgba(0, 0, 0, 0.15);
}

[data-theme="sunset"] {
  --theme-name: "sunset";
  --bg: #13100e;
  --bg-mesh: radial-gradient(at 0% 0%, rgba(245, 158, 11, 0.09) 0px, transparent 50%),
             radial-gradient(at 100% 100%, rgba(234, 88, 12, 0.05) 0px, transparent 50%);
  --surface: #1c1714;
  --surface-card: #231d19;
  --surface-hover: #302722;
  --surface-subtle: #161210;
  --border: #382d26;
  --border-hover: #4e3f35;
  --border-accent: #fbbf24;
  --text: #fef3c7;
  --text-hover: #ffffff;
  --text-muted: #bfa58f;
  --text-dim: #7a6859;
  --primary: #d97706;
  --primary-hover: #b45309;
  --primary-light: #fbbf24;
  --accent: #10b981;
  --accent-hover: #059669;
  --accent-ink: #07130e;
  --warning: #f59e0b;
  --danger: #f87171;
  --danger-hover: #ef4444;
  --navbar-bg: #181412;
  --btn-primary-bg: linear-gradient(135deg, #d97706, #b45309);
  --btn-primary-hover: linear-gradient(135deg, #f59e0b, #d97706);
  --btn-primary-border: #f59e0b;
  --btn-accent-bg: linear-gradient(135deg, #10b981, #059669);
  --btn-accent-hover: linear-gradient(135deg, #34d399, #10b981);
  --btn-accent-border: #34d399;
  --badge-bg: rgba(251, 191, 36, 0.1);
  --badge-border: rgba(251, 191, 36, 0.25);
  --nuance-bg: rgba(22, 18, 16, 0.7);
  --review-banner-bg: linear-gradient(135deg, rgba(217, 119, 6, 0.1), rgba(16, 185, 129, 0.04));
  --radius-sm: 6px;
  --radius: 12px;
  --radius-lg: 16px;
  --radius-pill: 999px;
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.35);
  --shadow: 0 4px 20px -2px rgba(0, 0, 0, 0.5);
  --shadow-lg: 0 12px 32px -4px rgba(0, 0, 0, 0.6);
  --white-ink: #fef3c7;
  --white-soft: #fde68a;
  --dark-ink: #100d0b;
  --danger-surface: rgba(248, 113, 113, 0.12);
  --danger-border: rgba(248, 113, 113, 0.35);
  --danger-ink: #fca5a5;
  --success-surface: rgba(16, 185, 129, 0.12);
  --success-border: rgba(16, 185, 129, 0.35);
  --success-ink: #6ee7b7;
  --playing-surface: rgba(251, 191, 36, 0.1);
  --stage0-bg: rgba(191, 165, 143, 0.1);
  --stage0-border: rgba(191, 165, 143, 0.25);
  --stage0-color: #bfa58f;
  --stage1-bg: rgba(52, 211, 153, 0.1);
  --stage1-border: rgba(52, 211, 153, 0.25);
  --stage1-color: #34d399;
  --stage2-bg: rgba(56, 189, 248, 0.1);
  --stage2-border: rgba(56, 189, 248, 0.25);
  --stage2-color: #38bdf8;
  --stage3-bg: rgba(96, 165, 250, 0.1);
  --stage3-border: rgba(96, 165, 250, 0.25);
  --stage3-color: #60a5fa;
  --stage4-bg: rgba(59, 130, 246, 0.12);
  --stage4-border: rgba(59, 130, 246, 0.3);
  --stage4-color: #93c5fd;
  --stage5-bg: rgba(251, 191, 36, 0.1);
  --stage5-border: rgba(251, 191, 36, 0.25);
  --stage5-color: #fbbf24;
  --focus-ring: rgba(251, 191, 36, 0.2);
  --card-border-subtle: rgba(128, 128, 128, 0.2);
  --card-dot-border: rgba(0, 0, 0, 0.15);
}

[data-theme="terminal"] {
  --theme-name: "terminal";
  --bg: #050805;
  --bg-mesh: radial-gradient(at 0% 0%, rgba(34, 197, 94, 0.08) 0px, transparent 50%),
             radial-gradient(at 100% 100%, rgba(6, 182, 212, 0.04) 0px, transparent 50%);
  --surface: #0a110c;
  --surface-card: #0d1710;
  --surface-hover: #142419;
  --surface-subtle: #070d08;
  --border: #16301f;
  --border-hover: #234c31;
  --border-accent: #22c55e;
  --text: #dcfce7;
  --text-hover: #ffffff;
  --text-muted: #86efac;
  --text-dim: #4ade80;
  --primary: #22c55e;
  --primary-hover: #16a34a;
  --primary-light: #4ade80;
  --accent: #06b6d4;
  --accent-hover: #0891b2;
  --accent-ink: #031519;
  --warning: #eab308;
  --danger: #f87171;
  --danger-hover: #ef4444;
  --navbar-bg: #070f09;
  --btn-primary-bg: linear-gradient(135deg, #16a34a, #15803d);
  --btn-primary-hover: linear-gradient(135deg, #22c55e, #16a34a);
  --btn-primary-border: #22c55e;
  --btn-accent-bg: linear-gradient(135deg, #0891b2, #0e7490);
  --btn-accent-hover: linear-gradient(135deg, #06b6d4, #0891b2);
  --btn-accent-border: #06b6d4;
  --badge-bg: rgba(34, 197, 94, 0.1);
  --badge-border: rgba(34, 197, 94, 0.25);
  --nuance-bg: rgba(7, 13, 8, 0.7);
  --review-banner-bg: linear-gradient(135deg, rgba(34, 197, 94, 0.1), rgba(6, 182, 212, 0.04));
  --radius-sm: 6px;
  --radius: 12px;
  --radius-lg: 16px;
  --radius-pill: 999px;
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.4);
  --shadow: 0 4px 20px -2px rgba(0, 0, 0, 0.6);
  --shadow-lg: 0 12px 32px -4px rgba(0, 0, 0, 0.7);
  --white-ink: #dcfce7;
  --white-soft: #bbf7d0;
  --dark-ink: #040805;
  --danger-surface: rgba(248, 113, 113, 0.12);
  --danger-border: rgba(248, 113, 113, 0.35);
  --danger-ink: #fca5a5;
  --success-surface: rgba(34, 197, 94, 0.12);
  --success-border: rgba(34, 197, 94, 0.35);
  --success-ink: #86efac;
  --playing-surface: rgba(34, 197, 94, 0.1);
  --stage0-bg: rgba(134, 239, 172, 0.1);
  --stage0-border: rgba(134, 239, 172, 0.25);
  --stage0-color: #86efac;
  --stage1-bg: rgba(34, 197, 94, 0.1);
  --stage1-border: rgba(34, 197, 94, 0.25);
  --stage1-color: #22c55e;
  --stage2-bg: rgba(6, 182, 212, 0.1);
  --stage2-border: rgba(6, 182, 212, 0.25);
  --stage2-color: #06b6d4;
  --stage3-bg: rgba(56, 189, 248, 0.1);
  --stage3-border: rgba(56, 189, 248, 0.25);
  --stage3-color: #38bdf8;
  --stage4-bg: rgba(74, 222, 128, 0.12);
  --stage4-border: rgba(74, 222, 128, 0.3);
  --stage4-color: #4ade80;
  --stage5-bg: rgba(234, 179, 8, 0.1);
  --stage5-border: rgba(234, 179, 8, 0.25);
  --stage5-color: #eab308;
  --focus-ring: rgba(34, 197, 94, 0.2);
  --card-border-subtle: rgba(128, 128, 128, 0.2);
  --card-dot-border: rgba(0, 0, 0, 0.15);
}

/* ==========================================================================
   GLOBAL LAYOUT & BASE STYLES
   ========================================================================== */

* {
  box-sizing: border-box;
}

html, body {
  overflow-x: clip;
}

body {
  margin: 0;
  font-family: var(--font-body);
  background-color: var(--bg);
  background-image: var(--bg-mesh);
  background-attachment: fixed;
  color: var(--text);
  line-height: 1.6;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

.container {
  max-width: 960px;
  margin-inline: auto;
  padding: 1rem 1.25rem 3rem;
  width: 100%;
}

header.navbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  padding: 0.75rem 1.25rem;
  position: sticky;
  top: 0;
  z-index: 100;
  background: var(--navbar-bg);
  border-bottom: 1px solid var(--border);
  margin-bottom: 1.5rem;
}

.navbar-brand-wrap {
  display: flex;
  align-items: center;
}

.navbar-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.navbar-logout-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.35rem;
  background: transparent;
  color: var(--text-muted);
  border: 1px solid var(--border);
  padding: 0.45rem 0.75rem;
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 0.825rem;
  font-weight: 500;
  transition: background-color 0.18s ease, border-color 0.18s ease, color 0.18s ease, transform 0.18s cubic-bezier(0.16, 1, 0.3, 1);
}

.navbar-logout-btn:hover {
  background: var(--danger-surface);
  border-color: var(--danger);
  color: var(--danger);
  transform: translateY(-1px);
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
  background: var(--badge-bg);
  border: 1px solid var(--badge-border);
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  margin-left: 4px;
}

.nav-links {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  flex-wrap: wrap;
}

.nav-links a, .nav-links button, .btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  background: var(--surface-card);
  color: var(--text);
  border: 1px solid var(--border);
  padding: 0.5rem 0.85rem;
  border-radius: var(--radius-sm);
  text-decoration: none;
  cursor: pointer;
  font-size: 0.875rem;
  font-weight: 500;
  transition: background-color 0.18s ease, border-color 0.18s ease, color 0.18s ease, transform 0.18s cubic-bezier(0.16, 1, 0.3, 1), box-shadow 0.18s ease;
}

.nav-links a:hover, .nav-links button:hover, .btn:hover {
  background: var(--surface-hover);
  border-color: var(--border-hover);
  color: var(--text-hover);
  transform: translateY(-1px);
}

.nav-links a:active, .nav-links button:active, .btn:active {
  transform: translateY(1px);
}

.nav-links a.active {
  background: var(--badge-bg);
  border-color: var(--border-accent);
  color: var(--primary-light);
  font-weight: 600;
}

/* Theme Selector in Navbar */
.theme-picker {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  background: var(--surface-subtle);
  border: 1px solid var(--border);
  padding: 0.4rem 0.65rem;
  border-radius: var(--radius-pill);
  color: var(--text-muted);
  font-size: 0.825rem;
  transition: border-color 0.18s ease, color 0.18s ease;
}

.theme-picker:hover {
  border-color: var(--border-hover);
  color: var(--text);
}

.theme-select-input {
  background: transparent !important;
  border: none !important;
  color: var(--text) !important;
  font-size: 0.825rem !important;
  font-weight: 500 !important;
  cursor: pointer;
  padding: 0 !important;
  margin: 0 !important;
  width: auto !important;
  outline: none !important;
  box-shadow: none !important;
}

.navbar-left {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
}

/* Voice Gender Selector in Navbar */
.voice-picker {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  background: var(--surface-subtle);
  border: 1px solid var(--border);
  padding: 0.4rem 0.65rem;
  border-radius: var(--radius-pill);
  color: var(--text-muted);
  font-size: 0.825rem;
  transition: border-color 0.18s ease, color 0.18s ease;
}

.voice-picker:hover {
  border-color: var(--border-hover);
  color: var(--text);
}

.voice-picker-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
}

.voice-select-input {
  background: transparent !important;
  border: none !important;
  color: var(--text) !important;
  font-size: 0.825rem !important;
  font-weight: 500 !important;
  cursor: pointer;
  padding: 0 !important;
  margin: 0 !important;
  width: auto !important;
  outline: none !important;
  box-shadow: none !important;
}

.voice-select-input option {
  background: var(--surface-card) !important;
  color: var(--text) !important;
}

/* Context Sentence & TTS Play Button in Study Card */
.context-sentence-wrap {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  margin-top: 0.4rem;
  margin-bottom: 0.4rem;
  flex-wrap: wrap;
}

.context-sentence-wrap .context-sentence {
  margin: 0;
  font-size: 1.15rem;
  font-weight: 600;
  color: var(--text);
  line-height: 1.45;
}

.btn-tts-speaker {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  background: var(--surface-card);
  border: 1px solid var(--border);
  color: var(--primary-light);
  padding: 0.28rem 0.65rem;
  border-radius: var(--radius-sm);
  font-size: 0.825rem;
  font-weight: 600;
  cursor: pointer;
  transition: background-color 0.18s ease, border-color 0.18s ease, color 0.18s ease, transform 0.18s cubic-bezier(0.16, 1, 0.3, 1);
  flex-shrink: 0;
  user-select: none;
}

.btn-tts-speaker:hover {
  background: var(--surface-hover);
  border-color: var(--border-accent);
  color: var(--text-hover);
  transform: translateY(-1px);
}

.btn-tts-speaker:active {
  transform: translateY(0);
}

.btn-tts-speaker.playing {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--playing-surface);
}

.btn-tts-speaker-sm {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: 1px solid var(--border);
  color: var(--text-muted);
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 0.75rem;
  cursor: pointer;
  margin-left: 0.4rem;
  vertical-align: middle;
  transition: border-color 0.15s ease, color 0.15s ease;
}

.btn-tts-speaker-sm:hover {
  border-color: var(--border-accent);
  color: var(--primary-light);
}

@keyframes tts-spin {
  0% { transform: rotate(0deg); }
  100% { transform: rotate(360deg); }
}

.audio-spinner {
  display: inline-block;
  width: 12px;
  height: 12px;
  border: 2px solid currentColor;
  border-right-color: transparent;
  border-radius: 50%;
  animation: tts-spin 0.6s linear infinite;
  vertical-align: middle;
}

/* Badges & Counters */
.count-pill {
  background: var(--accent);
  color: var(--accent-ink);
  font-size: 0.72rem;
  font-weight: 700;
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  line-height: 1.2;
}

#due-card-count {
  background: var(--accent);
  color: var(--accent-ink);
  font-size: 0.8rem;
  font-weight: 700;
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  margin: 0 2px;
}

.count-pill-zero {
  background: var(--stage0-bg);
  color: var(--text-muted);
  font-size: 0.72rem;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  line-height: 1.2;
}

/* Buttons */
.btn-primary {
  background: var(--btn-primary-bg) !important;
  border: 1px solid var(--btn-primary-border) !important;
  color: var(--white-ink) !important;
  font-weight: 600;
  box-shadow: var(--shadow-sm);
}

.btn-primary:hover {
  background: var(--btn-primary-hover) !important;
  border-color: var(--primary-light) !important;
  box-shadow: var(--shadow);
}

.btn-accent {
  background: var(--btn-accent-bg) !important;
  border: 1px solid var(--btn-accent-border) !important;
  color: var(--white-ink) !important;
  font-weight: 600;
  box-shadow: var(--shadow-sm);
}

.btn-accent:hover {
  background: var(--btn-accent-hover) !important;
  box-shadow: var(--shadow);
}

.btn-danger {
  background: transparent;
  border-color: var(--danger-border);
  color: var(--danger-ink) !important;
}

.btn-danger:hover {
  background: var(--danger-surface);
  border-color: var(--danger);
  color: var(--white-ink) !important;
}

/* Panels & Cards */
.panel {
  background: var(--surface-card);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 1.25rem 1.5rem;
  margin-bottom: 1.5rem;
  box-shadow: var(--shadow);
  transition: border-color 0.2s ease, background-color 0.2s ease;
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
  background: var(--review-banner-bg);
  border: 1px solid var(--border-accent);
}

.card-grid {
  display: grid;
  gap: 1.15rem;
}

.study-card {
  background: var(--surface-card);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 1.25rem 1.4rem;
  box-shadow: var(--shadow);
  transition: border-color 0.18s ease, box-shadow 0.18s ease, background-color 0.18s ease;
}

.study-card:hover {
  border-color: var(--border-accent);
  box-shadow: var(--shadow-lg);
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
  border-radius: var(--radius-pill);
  font-size: 0.78rem;
  font-weight: 600;
  background: var(--badge-bg);
  border: 1px solid var(--badge-border);
  color: var(--primary-light);
}

.stage-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  font-size: 0.75rem;
  font-weight: 700;
  background: var(--stage0-bg);
  border: 1px solid var(--stage0-border);
  color: var(--text-muted);
}

.stage-badge[data-stage="0"] { background: var(--stage0-bg); color: var(--stage0-color); border-color: var(--stage0-border); }
.stage-badge[data-stage="1"] { background: var(--stage1-bg); color: var(--stage1-color); border-color: var(--stage1-border); }
.stage-badge[data-stage="2"] { background: var(--stage2-bg); color: var(--stage2-color); border-color: var(--stage2-border); }
.stage-badge[data-stage="3"] { background: var(--stage3-bg); color: var(--stage3-color); border-color: var(--stage3-border); }
.stage-badge[data-stage="4"] { background: var(--stage4-bg); color: var(--stage4-color); border-color: var(--stage4-border); }
.stage-badge[data-stage="5"] { background: var(--stage5-bg); color: var(--stage5-color); border-color: var(--stage5-border); }

.tag-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
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
  gap: 0.4rem;
  margin-top: 0.5rem;
}

.context-sentence {
  font-size: 1.05rem;
  line-height: 1.6;
  color: var(--text);
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
  color: var(--text-muted);
}

.extra-examples li {
  margin-bottom: 0.25rem;
}

.nuance-note {
  background: var(--nuance-bg);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 0.5rem 0.75rem;
  font-size: 0.88rem;
  color: var(--text);
  margin: 0.6rem 0;
}

/* Forms & Inputs */
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

input:focus-visible, textarea:focus-visible, select:focus-visible {
  outline: 2px solid var(--border-accent);
  outline-offset: 1px;
  border-color: var(--border-accent);
}

a:focus-visible, button:focus-visible, details summary:focus-visible {
  outline: 2px solid var(--border-accent);
  outline-offset: 2px;
}

label {
  display: block;
  font-size: 0.825rem;
  font-weight: 600;
  color: var(--text-muted);
  text-transform: none;
  letter-spacing: normal;
}

h1, h2, h3 {
  letter-spacing: -0.02em;
  font-weight: 700;
  text-wrap: balance;
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
  background: var(--danger-surface);
  border: 1px solid var(--danger-border);
  color: var(--danger-ink);
}

.alert-success {
  background: var(--success-surface);
  border: 1px solid var(--success-border);
  color: var(--success-ink);
}

/* Theme Picker Cards (Settings Page) */
.theme-cards-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 1rem;
  margin-top: 1rem;
}

.theme-card-option {
  border: 2px solid var(--border);
  border-radius: var(--radius);
  padding: 1rem;
  background: var(--surface-card);
  color: var(--text);
  cursor: pointer;
  transition: border-color 0.2s ease, transform 0.2s cubic-bezier(0.16, 1, 0.3, 1), box-shadow 0.2s ease;
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  text-align: left;
  width: 100%;
}

.theme-card-option:hover {
  transform: translateY(-2px);
  border-color: var(--border-accent);
  box-shadow: var(--shadow);
}

.theme-card-option.active {
  border-color: var(--primary-light);
  box-shadow: 0 0 0 2px var(--primary-light);
}

.theme-card-preview {
  height: 54px;
  border-radius: var(--radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  border: 1px solid var(--card-border-subtle);
}

.theme-card-dot {
  width: 16px;
  height: 16px;
  border-radius: var(--radius-pill);
  border: 1px solid var(--card-dot-border);
}

@media (max-width: 860px) {
  header.navbar {
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-rows: auto auto;
    gap: 0.5rem 0.5rem;
    padding: 0.55rem 0.85rem;
    align-items: center;
    margin-bottom: 1rem;
  }
  .navbar-brand-wrap {
    grid-column: 1 / 2;
    grid-row: 1 / 2;
  }
  .navbar-actions {
    grid-column: 2 / 3;
    grid-row: 1 / 2;
    justify-content: flex-end;
    gap: 0.35rem;
  }
  .nav-links {
    grid-column: 1 / 3;
    grid-row: 2 / 3;
    width: 100%;
    display: flex;
    overflow-x: auto;
    white-space: nowrap;
    scrollbar-width: none;
    -ms-overflow-style: none;
    -webkit-overflow-scrolling: touch;
    gap: 0.35rem;
    padding: 2px 0 2px;
  }
  .nav-links::-webkit-scrollbar {
    display: none;
  }
  .nav-links a {
    flex-shrink: 0;
    padding: 0.42rem 0.65rem;
    font-size: 0.825rem;
  }
  .nav-label-long {
    display: none;
  }
  .logout-text {
    display: none;
  }
  .navbar-logout-btn {
    padding: 0.35rem 0.45rem;
  }
  .brand-logo {
    font-size: 1.05rem;
  }
  .brand-logo svg {
    width: 22px;
    height: 22px;
  }
  .brand-badge {
    display: none;
  }
  .voice-picker, .theme-picker {
    padding: 0.32rem 0.45rem;
    font-size: 0.78rem;
  }
  .voice-select-input, .theme-select-input {
    font-size: 0.78rem !important;
  }
  .container {
    padding: 0.65rem 0.85rem 2rem;
  }
  .theme-cards-grid {
    grid-template-columns: 1fr;
  }
}

@media (prefers-reduced-motion: reduce) {
  *, ::before, ::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }
}

button:disabled, input:disabled, select:disabled, textarea:disabled, .btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
  pointer-events: none;
  filter: grayscale(0.5);
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
  <title>로그인: My English</title>
  <style>{APP_CSS}</style>
</head>
<body>
  <main class="container">
    <section class="panel" style="max-width: 420px; margin: 4rem auto; padding: 2.25rem; text-align: center; border-color: var(--border-accent); box-shadow: var(--shadow-lg);">
      <div style="display:flex; justify-content:center; margin-bottom: 1.25rem;">
        {ICON_LOGO_LARGE}
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
                    let ex_enc = urlencoding::encode(&ex.english);
                    examples_html.push_str(&format!(
                        r#"<li><strong>{}</strong> <button type="button" class="btn-tts-speaker-sm" onclick="window.playSentenceAudio(this, decodeURIComponent('{ex_enc}'))" title="예문 발음 듣기" aria-label="예문 발음 듣기">{ICON_SPEAKER}</button>: {}</li>"#,
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
            let context_sentence_enc = urlencoding::encode(&card.context_sentence);

            cards_html.push_str(&format!(
                r#"<article class="study-card" data-card-id="{id}">
  <div class="chunk-header">
    <div style="display:flex; align-items:center; gap:0.5rem; flex-wrap:wrap;">
      <span class="chunk-title">{chunk}</span>
      <span class="surface-badge" title="Surface Chunk in Context Sentence">{surface_chunk}</span>
    </div>
    <span class="stage-badge" data-stage="{srs_stage}">SRS Stage {srs_stage}</span>
  </div>
  <div class="context-sentence-wrap">
    <p class="context-sentence">{context_sentence}</p>
    <button type="button" class="btn-tts-speaker" onclick="window.playSentenceAudio(this, decodeURIComponent('{context_sentence_enc}'))" title="문장 발음 듣기" aria-label="문장 발음 듣기">
      {ICON_SPEAKER}
      <span>듣기</span>
    </button>
  </div>
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
                context_sentence_enc = context_sentence_enc,
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
  <title>My English: Chunk &amp; SRS</title>
  <style>{APP_CSS}</style>
</head>
<body>
  {navbar}
  <div class="container">
    <section class="panel" id="smart-input-section" style="border-color: var(--border-accent);">
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
