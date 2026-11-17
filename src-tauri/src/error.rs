//! Structured application errors.
//!
//! Every fallible command returns `AppResult<T>`. Errors serialize to
//! `{ "kind": "...", "message": "..." }` so the frontend can branch on `kind`.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Config(String),
    #[error("database error: {0}")]
    Database(String),
    #[error("{0}")]
    Provider(String),
    #[error("missing credential: {0}")]
    MissingCredential(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("tool error: {0}")]
    Tool(String),
    #[error("mcp error: {0}")]
    Mcp(String),
    #[error("voice error: {0}")]
    Voice(String),
    #[error("cancelled")]
    Cancelled,
    #[error("io error: {0}")]
    Io(String),
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Config(_) => "config",
            AppError::Database(_) => "database",
            AppError::Provider(_) => "provider",
            AppError::MissingCredential(_) => "missing_credential",
            AppError::PermissionDenied(_) => "permission_denied",
            AppError::InvalidInput(_) => "invalid_input",
            AppError::NotFound(_) => "not_found",
            AppError::Tool(_) => "tool",
            AppError::Mcp(_) => "mcp",
            AppError::Voice(_) => "voice",
            AppError::Cancelled => "cancelled",
            AppError::Io(_) => "io",
            AppError::Internal(_) => "internal",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::InvalidInput(format!("json: {e}"))
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Provider(format!("network: {e}"))
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
