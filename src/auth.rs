use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::state::AppState;
use crate::ui;

type HmacSha256 = Hmac<Sha256>;

pub const SESSION_COOKIE_NAME: &str = "my_english_session";
pub const SESSION_MAX_AGE_SECS: u64 = 180 * 24 * 60 * 60; // 180 days

#[derive(Deserialize)]
pub struct LoginForm {
    pub passcode: String,
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_handler))
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", post(logout_submit))
        .route("/manifest.json", get(manifest_handler))
}

async fn health_handler() -> &'static str {
    "ok"
}

async fn manifest_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        ui::render_pwa_manifest(),
    )
}

async fn login_page() -> Html<String> {
    Html(ui::render_login_page(None))
}

async fn login_submit(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> Response {
    if verify_passcode(&form.passcode, &state.app_passcode) {
        let token = sign_session_token("authenticated", &state.cookie_secret);
        let cookie = format!(
            "{SESSION_COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_MAX_AGE_SECS}"
        );
        (
            StatusCode::SEE_OTHER,
            [
                (header::SET_COOKIE, cookie),
                (header::LOCATION, "/".to_string()),
            ],
        )
            .into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Html(ui::render_login_page(Some("패스코드가 올바르지 않습니다."))),
        )
            .into_response()
    }
}

async fn logout_submit() -> Response {
    let clear_cookie =
        format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    (
        StatusCode::SEE_OTHER,
        [
            (header::SET_COOKIE, clear_cookie),
            (header::LOCATION, "/login".to_string()),
        ],
    )
        .into_response()
}

pub async fn require_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let cookie_header = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok());

    if verify_session_cookie(cookie_header, &state.cookie_secret) {
        next.run(request).await
    } else {
        Redirect::to("/login").into_response()
    }
}

pub fn verify_passcode(input: &str, expected: &str) -> bool {
    let mut mac_input = HmacSha256::new_from_slice(b"my_english_passcode_compare")
        .expect("HMAC can take key of any size");
    mac_input.update(input.as_bytes());
    let input_tag = mac_input.finalize().into_bytes();

    let mut mac_expected = HmacSha256::new_from_slice(b"my_english_passcode_compare")
        .expect("HMAC can take key of any size");
    mac_expected.update(expected.as_bytes());
    mac_expected.verify_slice(&input_tag).is_ok() && !expected.is_empty()
}

pub fn sign_session_token(payload: &str, secret: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("{payload}.{sig}")
}

pub fn verify_session_cookie(cookie_header: Option<&str>, secret: &str) -> bool {
    let Some(header_val) = cookie_header else {
        return false;
    };
    for part in header_val.split(';') {
        let trimmed = part.trim();
        if let Some(token) = trimmed.strip_prefix(&format!("{SESSION_COOKIE_NAME}=")) {
            if verify_session_token(token, secret) {
                return true;
            }
        }
    }
    false
}

pub fn verify_session_token(token: &str, secret: &str) -> bool {
    let Some((payload, sig_hex)) = token.rsplit_once('.') else {
        return false;
    };
    if payload.is_empty() {
        return false;
    }
    let Ok(sig_bytes) = hex::decode(sig_hex) else {
        return false;
    };
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    mac.verify_slice(&sig_bytes).is_ok()
}
