use anyhow::Result;
use async_trait::async_trait;
use axum::Router;

use crate::state::AppState;

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

pub fn routes() -> Router<AppState> {
    Router::new()
}
