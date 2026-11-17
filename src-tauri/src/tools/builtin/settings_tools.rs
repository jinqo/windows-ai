//! Let the assistant read and change a small, safe subset of app settings.
//! Permissions, MCP servers, directories and commands are deliberately
//! not changeable from chat.

use crate::config::{AppConfig, PermissionMode};
use crate::error::{AppError, AppResult};
use crate::permissions::Access;
use crate::tools::{str_arg, Tool, ToolContext, ToolMeta};
use serde_json::{json, Value};

pub const CHANGEABLE: &[&str] = &[
    "appearance.accent",
    "appearance.opacity",
    "appearance.font_size",
    "appearance.compact",
    "voice.tts_enabled",
    "voice.tts_rate",
    "ai.anthropic.effort",
    "ai.anthropic.model",
];

pub fn apply_setting(cfg: &mut AppConfig, key: &str, value: &Value) -> AppResult<()> {
    let bad = || AppError::InvalidInput(format!("invalid value for {key}"));
    match key {
        "appearance.accent" => {
            let v = value.as_str().ok_or_else(bad)?;
            if !["violet", "purple", "indigo", "fuchsia"].contains(&v) {
                return Err(bad());
            }
            cfg.appearance.accent = v.into();
        }
        "appearance.opacity" => cfg.appearance.opacity = value.as_f64().ok_or_else(bad)? as f32,
        "appearance.font_size" => cfg.appearance.font_size = value.as_u64().filter(|v| (11..=20).contains(v)).ok_or_else(bad)? as u32,
        "appearance.compact" => cfg.appearance.compact = value.as_bool().ok_or_else(bad)?,
        "voice.tts_enabled" => cfg.voice.tts_enabled = value.as_bool().ok_or_else(bad)?,
        "voice.tts_rate" => cfg.voice.tts_rate = value.as_f64().filter(|v| (0.5..=2.0).contains(v)).ok_or_else(bad)? as f32,
        "ai.anthropic.effort" => cfg.ai.anthropic.effort = value.as_str().ok_or_else(bad)?.into(),
        "ai.anthropic.model" => cfg.ai.anthropic.model = value.as_str().filter(|s| s.starts_with("claude-")).ok_or_else(bad)?.into(),
        _ => return Err(AppError::PermissionDenied(format!("{key} cannot be changed from chat; allowed: {}", CHANGEABLE.join(", ")))),
    }
    cfg.validate()
}

pub struct GetSettings;

#[async_trait::async_trait]
impl Tool for GetSettings {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "get_app_settings".into(),
            description: "Read Windows AI's current (non-secret) settings.".into(),
            input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
            access: Access::Read,
            default_mode: PermissionMode::Allow,
            locked_ask: false,
            origin: "builtin".into(),
        }
    }

    fn describe(&self, _input: &Value) -> String {
        "Read Windows AI settings".into()
    }

    async fn run(&self, ctx: &ToolContext, _input: Value) -> AppResult<String> {
        let c = &ctx.cfg;
        Ok(serde_json::to_string_pretty(&json!({
            "appearance": c.appearance,
            "ai": {"provider": c.ai.provider, "model": c.ai.anthropic.model, "effort": c.ai.anthropic.effort},
            "voice": {"stt_provider": c.voice.stt_provider, "tts_enabled": c.voice.tts_enabled},
            "shortcuts": c.shortcuts,
            "startup": c.startup,
            "changeable_from_chat": CHANGEABLE,
        }))?)
    }
}

pub struct UpdateSetting;

#[async_trait::async_trait]
impl Tool for UpdateSetting {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "update_app_setting".into(),
            description: format!("Change one Windows AI setting. Allowed keys: {}", CHANGEABLE.join(", ")),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": {"enum": CHANGEABLE},
                    "value": {"description": "New value (string, number or boolean)"}
                },
                "required": ["key", "value"],
                "additionalProperties": false
            }),
            access: Access::System,
            default_mode: PermissionMode::Ask,
            locked_ask: false,
            origin: "builtin".into(),
        }
    }

    fn describe(&self, input: &Value) -> String {
        format!("Change setting {} to {}", str_arg(input, "key"), input.get("value").cloned().unwrap_or(Value::Null))
    }

    fn risk(&self, _input: &Value) -> &'static str {
        "low"
    }

    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String> {
        let key = str_arg(&input, "key").to_string();
        let value = input.get("value").cloned().unwrap_or(Value::Null);
        let mut cfg = ctx.cfg.clone();
        apply_setting(&mut cfg, &key, &value)?;
        // Persisting is done by the app layer, which owns the ConfigStore.
        ctx.notifier.emit("settings-change-request", json!({"config": cfg}));
        Ok(format!("Updated {key}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_safe_keys_change() {
        let mut c = AppConfig::default();
        apply_setting(&mut c, "appearance.compact", &json!(true)).unwrap();
        assert!(c.appearance.compact);
        assert!(apply_setting(&mut c, "permissions.commands", &json!([])).is_err());
        assert!(apply_setting(&mut c, "appearance.font_size", &json!(99)).is_err());
        assert!(apply_setting(&mut c, "ai.anthropic.effort", &json!("ultra")).is_err());
    }
}
