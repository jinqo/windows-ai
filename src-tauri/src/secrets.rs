//! Secret storage backed by Windows Credential Manager (via `keyring`).
//!
//! API keys are never written to config.json, the SQLite DB, or logs.
//! The frontend can set/clear a secret and ask whether one exists, but can
//! never read a stored value back.

use crate::error::{AppError, AppResult};

const SERVICE: &str = "WindowsAI";

/// Known secret slots. Arbitrary names are rejected.
pub const SECRET_NAMES: &[&str] = &["anthropic_api_key", "openai_compatible_api_key", "openai_whisper_api_key"];

fn check_name(name: &str) -> AppResult<()> {
    if SECRET_NAMES.contains(&name) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!("unknown secret '{name}'")))
    }
}

fn entry(name: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(SERVICE, name).map_err(|e| AppError::Internal(format!("credential store: {e}")))
}

pub fn set_secret(name: &str, value: &str) -> AppResult<()> {
    check_name(name)?;
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::InvalidInput("secret value is empty".into()));
    }
    entry(name)?
        .set_password(value)
        .map_err(|e| AppError::Internal(format!("credential store: {e}")))
}

pub fn delete_secret(name: &str) -> AppResult<()> {
    check_name(name)?;
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Internal(format!("credential store: {e}"))),
    }
}

/// Reads a secret for backend use only. Falls back to well-known env vars.
pub fn get_secret(name: &str) -> Option<String> {
    if check_name(name).is_err() {
        return None;
    }
    if let Ok(e) = entry(name) {
        if let Ok(v) = e.get_password() {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    let env = match name {
        "anthropic_api_key" => "ANTHROPIC_API_KEY",
        "openai_whisper_api_key" => "OPENAI_API_KEY",
        "openai_compatible_api_key" => "OPENAI_COMPATIBLE_API_KEY",
        _ => return None,
    };
    std::env::var(env).ok().filter(|v| !v.is_empty())
}

#[derive(serde::Serialize)]
pub struct SecretStatus {
    pub name: String,
    pub stored: bool,
    pub from_env: bool,
}

pub fn status(name: &str) -> AppResult<SecretStatus> {
    check_name(name)?;
    let stored = entry(name)?.get_password().map(|v| !v.is_empty()).unwrap_or(false);
    let from_env = !stored && get_secret(name).is_some();
    Ok(SecretStatus { name: name.into(), stored, from_env })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_names() {
        assert!(set_secret("evil", "x").is_err());
        assert!(get_secret("evil").is_none());
    }
}
