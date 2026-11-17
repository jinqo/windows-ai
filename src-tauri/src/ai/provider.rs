//! Provider abstraction. Add a new backend by implementing `Provider` and
//! registering it in `build_provider`.

use super::types::{ChatRequest, ChatResponse, StreamEvent};
use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub type EventSink<'a> = &'a mut (dyn FnMut(StreamEvent) + Send);

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatus {
    pub provider: String,
    pub model: String,
    /// "connected" | "missing_credentials" | "error" | "unreachable"
    pub state: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
}

#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn model(&self) -> String;
    /// Streams one model turn. Must honour `cancel` promptly.
    async fn stream(&self, req: &ChatRequest, cancel: &CancellationToken, sink: EventSink<'_>) -> AppResult<ChatResponse>;
    /// Cheap connectivity/credential check (must not spend generation tokens).
    async fn check(&self) -> ProviderStatus;
    async fn list_models(&self) -> AppResult<Vec<ModelInfo>>;
}

/// Returns the turn-level provider, or None for the Claude Code backend
/// (which runs its own agent loop in the CLI).
pub fn build_provider(cfg: &AppConfig) -> AppResult<Option<Arc<dyn Provider>>> {
    Ok(Some(match cfg.ai.provider.as_str() {
        "claude_code" => return Ok(None),
        "anthropic" => Arc::new(super::anthropic::AnthropicProvider::new(cfg.ai.anthropic.clone())),
        "openai_compatible" => Arc::new(super::openai_compat::OpenAiCompatProvider::new(cfg.ai.openai_compatible.clone())),
        other => return Err(AppError::Config(format!("unknown provider '{other}'"))),
    }))
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("WindowsAI/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("http client")
}
