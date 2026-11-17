//! Tool framework. Every tool declares a JSON input schema, an access class
//! and a default permission mode. Execution always goes through
//! `executor::execute`, which enforces permissions and logs activity.

pub mod builtin;
pub mod executor;
pub mod schema;

use crate::config::{AppConfig, PermissionMode};
use crate::db::Db;
use crate::error::AppResult;
use crate::permissions::{Access, Notifier};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
pub struct ToolMeta {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub access: Access,
    pub default_mode: PermissionMode,
    /// Requires approval every time; cannot be switched to "allow".
    pub locked_ask: bool,
    /// "builtin" or "mcp:<server id>"
    pub origin: String,
}

pub struct ToolContext {
    pub cfg: AppConfig,
    pub db: Db,
    pub notifier: Arc<dyn Notifier>,
    pub cancel: CancellationToken,
    pub conversation_id: Option<String>,
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn meta(&self) -> ToolMeta;
    /// Human-readable description of exactly what this call will do.
    fn describe(&self, input: &Value) -> String;
    /// Risk shown in the approval dialog.
    fn risk(&self, _input: &Value) -> &'static str {
        match self.meta().access {
            Access::Read | Access::System => "low",
            Access::Network | Access::Memory => "medium",
            _ => "high",
        }
    }
    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String>;
}

pub fn str_arg<'a>(input: &'a Value, key: &str) -> &'a str {
    input.get(key).and_then(Value::as_str).unwrap_or("")
}

pub use builtin::read_text_file as builtin_read_text;

pub fn builtin_tools() -> Vec<Arc<dyn Tool>> {
    builtin::all()
}
