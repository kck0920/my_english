use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::ai::{AiProvider, NoopAiProvider};
use crate::tts::{NoopTtsProvider, TtsProvider};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub db_path: Option<PathBuf>,
    pub app_passcode: String,
    pub cookie_secret: String,
    pub ai_provider: Arc<dyn AiProvider>,
    pub tts_provider: Arc<dyn TtsProvider>,
}

impl AppState {
    pub fn new(pool: SqlitePool, app_passcode: impl Into<String>, cookie_secret: impl Into<String>) -> Self {
        Self {
            pool,
            db_path: None,
            app_passcode: app_passcode.into(),
            cookie_secret: cookie_secret.into(),
            ai_provider: Arc::new(NoopAiProvider),
            tts_provider: Arc::new(NoopTtsProvider),
        }
    }

    pub fn with_db_path(mut self, path: PathBuf) -> Self {
        self.db_path = Some(path);
        self
    }

    pub fn with_ai_provider(mut self, provider: Arc<dyn AiProvider>) -> Self {
        self.ai_provider = provider;
        self
    }

    pub fn with_tts_provider(mut self, provider: Arc<dyn TtsProvider>) -> Self {
        self.tts_provider = provider;
        self
    }
}
