use crate::config::PermissionMode;
use crate::error::{AppError, AppResult};
use crate::permissions::Access;
use crate::tools::{str_arg, Tool, ToolContext, ToolMeta};
use serde_json::{json, Value};

pub struct LaunchApp;

#[async_trait::async_trait]
impl Tool for LaunchApp {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "launch_app".into(),
            description: "Launch one of the user's approved applications by its configured name. Arbitrary executables and extra arguments are not allowed.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {"name": {"type": "string", "minLength": 1, "description": "Approved app name, e.g. \"notepad\""}},
                "required": ["name"],
                "additionalProperties": false
            }),
            access: Access::Execute,
            default_mode: PermissionMode::Ask,
            locked_ask: false,
            origin: "builtin".into(),
        }
    }

    fn describe(&self, input: &Value) -> String {
        format!("Launch application \"{}\"", str_arg(input, "name"))
    }

    fn risk(&self, _input: &Value) -> &'static str {
        "medium"
    }

    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String> {
        let name = str_arg(&input, "name").trim().to_lowercase();
        let app = ctx
            .cfg
            .permissions
            .apps
            .iter()
            .find(|a| a.name.to_lowercase() == name)
            .ok_or_else(|| {
                let names: Vec<_> = ctx.cfg.permissions.apps.iter().map(|a| a.name.as_str()).collect();
                AppError::PermissionDenied(format!("\"{name}\" is not an approved application. Approved: {}", names.join(", ")))
            })?;
        let mut cmd = std::process::Command::new(&app.path);
        cmd.args(&app.args);
        cmd.spawn().map_err(|e| AppError::Tool(format!("failed to launch {}: {e}", app.path)))?;
        Ok(format!("Launched {}", app.name))
    }
}

pub struct OpenUrl;

pub fn validate_url(url: &str) -> AppResult<()> {
    let lower = url.trim().to_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(AppError::PermissionDenied("only http(s) URLs can be opened".into()));
    }
    if url.chars().any(|c| c.is_control() || c == '"' || c == ' ') {
        return Err(AppError::InvalidInput("URL contains invalid characters".into()));
    }
    Ok(())
}

#[async_trait::async_trait]
impl Tool for OpenUrl {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "open_url".into(),
            description: "Open an http(s) URL in the user's default browser.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {"url": {"type": "string", "minLength": 8}},
                "required": ["url"],
                "additionalProperties": false
            }),
            access: Access::Network,
            default_mode: PermissionMode::Ask,
            locked_ask: false,
            origin: "builtin".into(),
        }
    }

    fn describe(&self, input: &Value) -> String {
        format!("Open {} in the default browser", str_arg(input, "url"))
    }

    async fn run(&self, _ctx: &ToolContext, input: Value) -> AppResult<String> {
        let url = str_arg(&input, "url").trim();
        validate_url(url)?;
        tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| AppError::Tool(format!("could not open URL: {e}")))?;
        Ok(format!("Opened {url}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_validation() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("file:///C:/Windows/system32/calc.exe").is_err());
        assert!(validate_url("javascript:alert(1)").is_err());
        assert!(validate_url("https://a.com\" & calc").is_err());
    }
}
