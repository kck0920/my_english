use std::path::PathBuf;

use anyhow::Result;
use my_english::{build_app, db, state::AppState};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "my_english=info,tower_http=info".into()),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://my_english.db".to_string());
    let app_passcode = std::env::var("APP_PASSCODE")
        .map_err(|_| anyhow::anyhow!("APP_PASSCODE environment variable is required"))?;
    let cookie_secret = std::env::var("COOKIE_SECRET")
        .map_err(|_| anyhow::anyhow!("COOKIE_SECRET environment variable is required. Generate one with: openssl rand -hex 32"))?;
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8081);

    let pool = db::init_db(&database_url).await?;
    let db_path = database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
        .map(PathBuf::from);

    let mut state = AppState::new(pool, app_passcode, cookie_secret);
    if let Some(path) = db_path {
        state = state.with_db_path(path);
    }

    let app = build_app(state);
    let addr = format!("127.0.0.1:{port}");
    tracing::info!("Listening on http://{addr} (loopback only)");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
