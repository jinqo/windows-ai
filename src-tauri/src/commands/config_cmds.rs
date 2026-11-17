use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::secrets;
use crate::state::AppState;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
pub fn get_config(state: State<AppState>) -> AppConfig {
    state.config.get()
}

/// Validates, persists and applies a new configuration.
#[tauri::command]
pub async fn save_config(app: AppHandle, state: State<'_, AppState>, config: AppConfig) -> AppResult<Value> {
    let old = state.config.get();
    state.config.set(config.clone())?;
    let mut warnings = Vec::new();
    if old.shortcuts != config.shortcuts || old.integrations.selection_shortcut != config.integrations.selection_shortcut || old.integrations.selection_enabled != config.integrations.selection_enabled {
        warnings.extend(crate::window::apply_shortcuts(&app, &config)?);
    }
    if old.window != config.window {
        crate::window::apply_window_config(&app, &config);
    }
    if old.startup.launch_on_login != config.startup.launch_on_login {
        let al = app.autolaunch();
        let r = if config.startup.launch_on_login { al.enable() } else { al.disable() };
        if let Err(e) = r {
            warnings.push(format!("startup setting: {e}"));
        }
    }
    if old.mcp != config.mcp {
        state.mcp.sync_config(&config.mcp.servers);
    }
    let _ = app.emit("config-changed", &config);
    Ok(json!({ "warnings": warnings }))
}

#[tauri::command]
pub fn secret_status(name: String) -> AppResult<secrets::SecretStatus> {
    secrets::status(&name)
}

#[tauri::command]
pub fn set_secret(name: String, value: String) -> AppResult<()> {
    secrets::set_secret(&name, &value)
}

#[tauri::command]
pub fn delete_secret(name: String) -> AppResult<()> {
    secrets::delete_secret(&name)
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<AppState>) -> AppResult<Value> {
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "config_path": state.config.path(),
        "data_dir": app.path().app_data_dir().map_err(|e| AppError::Internal(e.to_string()))?,
        "autostart_enabled": autostart,
        "bridge_pipe": crate::integrations::bridge::pipe_name(),
        "platform": std::env::consts::OS,
    }))
}
