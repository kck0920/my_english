use anyhow::Result;
use async_trait::async_trait;
use axum::Router;

use crate::domain::{DraftCard, OutputTransferEvaluation, OutputTransferPrompt};
use crate::state::AppState;

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn generate_draft_cards(&self, input: &str) -> Result<Vec<DraftCard>>;

    async fn generate_output_transfer_prompt(
        &self,
        chunk: &str,
        context_sentence: &str,
        korean_meaning: &str,
    ) -> Result<OutputTransferPrompt>;

    async fn evaluate_output_transfer(
        &self,
        chunk: &str,
        korean_prompt: &str,
        user_sentence: &str,
    ) -> Result<OutputTransferEvaluation>;
}

#[derive(Debug, Default, Clone)]
pub struct NoopAiProvider;

#[async_trait]
impl AiProvider for NoopAiProvider {
    async fn generate_draft_cards(&self, _input: &str) -> Result<Vec<DraftCard>> {
        anyhow::bail!("AI provider is not configured")
    }

    async fn generate_output_transfer_prompt(
        &self,
        _chunk: &str,
        _context_sentence: &str,
        _korean_meaning: &str,
    ) -> Result<OutputTransferPrompt> {
        anyhow::bail!("AI provider is not configured")
    }

    async fn evaluate_output_transfer(
        &self,
        _chunk: &str,
        _korean_prompt: &str,
        _user_sentence: &str,
    ) -> Result<OutputTransferEvaluation> {
        anyhow::bail!("AI provider is not configured")
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
}
