//! MCP, voice, integrations and window commands.

use crate::error::{AppError, AppResult};
use crate::integrations::context::CapturedContext;
use crate::integrations::{explorer, native_host};
use crate::mcp::McpServerStatus;
use crate::state::AppState;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

// ---------------------------------------------------------------- MCP

#[tauri::command]
pub fn mcp_statuses(state: State<AppState>) -> Vec<McpServerStatus> {
    state.mcp.sync_config(&state.config.get().mcp.servers);
    state.mcp.statuses()
}

#[tauri::command]
pub async fn mcp_connect(state: State<'_, AppState>, id: String) -> AppResult<McpServerStatus> {
    state.mcp.sync_config(&state.config.get().mcp.servers);
    state.mcp.connect(&id).await
}

#[tauri::command]
pub async fn mcp_disconnect(state: State<'_, AppState>, id: String) -> AppResult<McpServerStatus> {
    state.mcp.disconnect(&id).await
}

/// Trusting a tool makes it available to the assistant (calls still pass
/// the permission executor). Newly discovered tools start untrusted.
#[tauri::command]
pub fn mcp_set_tool_trust(app: AppHandle, state: State<AppState>, server_id: String, tool: String, trusted: bool) -> AppResult<McpServerStatus> {
    let cfg = state.config.update(|c| {
        if let Some(s) = c.mcp.servers.iter_mut().find(|s| s.id == server_id) {
            s.trusted_tools.retain(|t| t != &tool);
            if trusted {
                s.trusted_tools.push(tool.clone());
            }
        }
    })?;
    state.mcp.sync_config(&cfg.mcp.servers);
    let _ = app.emit("config-changed", &cfg);
    state.mcp.status(&server_id)
}

// ---------------------------------------------------------------- voice

#[tauri::command]
pub async fn voice_start(state: State<'_, AppState>) -> AppResult<()> {
    let cfg = state.config.get();
    if cfg.voice.stt_provider != "windows" {
        return Err(AppError::Voice("native listening is only used with the Windows speech provider".into()));
    }
    state.voice.start(&cfg.voice.stt_language, state.notifier.clone()).await
}

#[tauri::command]
pub async fn voice_stop(state: State<'_, AppState>) -> AppResult<String> {
    state.voice.stop().await
}

#[tauri::command]
pub fn voice_cancel(state: State<AppState>) {
    state.voice.cancel();
}

/// Transcribes base64 audio recorded in the webview (Whisper provider).
#[tauri::command]
pub async fn voice_transcribe(state: State<'_, AppState>, audio_base64: String, mime: String) -> AppResult<String> {
    use base64::Engine;
    let cfg = state.config.get();
    let audio = base64::engine::general_purpose::STANDARD.decode(audio_base64.as_bytes()).map_err(|e| AppError::InvalidInput(e.to_string()))?;
    crate::voice::transcribe_whisper(&cfg.voice.whisper_base_url, audio, &mime, &cfg.voice.stt_language).await
}

// ---------------------------------------------------------------- integrations / context

#[tauri::command]
pub fn inbox_list(state: State<AppState>) -> Vec<CapturedContext> {
    state.inbox.list()
}

#[tauri::command]
pub fn inbox_remove(state: State<AppState>, id: String) {
    state.inbox.remove(&id);
}

#[tauri::command]
pub fn inbox_clear(state: State<AppState>) {
    state.inbox.clear();
}

/// Explicit, user-initiated "attach clipboard" (never automatic).
#[tauri::command]
pub fn capture_clipboard(app: AppHandle, state: State<AppState>) -> AppResult<CapturedContext> {
    #[cfg(windows)]
    {
        let text = crate::integrations::clipboard::read_text()?.filter(|t| !t.trim().is_empty()).ok_or_else(|| AppError::NotFound("the clipboard has no text".into()))?;
        let kind = crate::integrations::selection::classify("", &text);
        let mut c = CapturedContext::new("clipboard", kind, text);
        c.method = Some("clipboard".into());
        let c = c.sanitize(state.config.get().integrations.max_context_chars as usize).map_err(AppError::InvalidInput)?;
        state.inbox.push(c.clone());
        let _ = app.emit("context-captured", &c);
        Ok(c)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state);
        Err(AppError::Internal("not supported".into()))
    }
}

/// Copies text (e.g. a rewrite result) to the clipboard on user request.
#[tauri::command]
pub fn copy_text(text: String) -> AppResult<()> {
    #[cfg(windows)]
    {
        crate::integrations::clipboard::write_text(&text)
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        Err(AppError::Internal("not supported".into()))
    }
}

#[tauri::command]
pub fn integration_status() -> Value {
    #[cfg(windows)]
    {
        json!({
            "explorer_menu": explorer::is_installed(),
            "browser_host": native_host::status(),
            "bridge_pipe": crate::integrations::bridge::pipe_name(),
            "bridge_token_path": crate::integrations::bridge::token_path(),
            "extension_id": native_host::EXTENSION_ID,
        })
    }
    #[cfg(not(windows))]
    {
        json!({})
    }
}

#[tauri::command]
pub fn set_explorer_menu(enabled: bool) -> AppResult<Value> {
    #[cfg(windows)]
    {
        if enabled {
            explorer::install()?;
        } else {
            explorer::uninstall()?;
        }
        Ok(integration_status())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err(AppError::Internal("not supported".into()))
    }
}

#[tauri::command]
pub fn set_browser_host(enabled: bool, extra_extension_ids: Option<Vec<String>>) -> AppResult<Value> {
    #[cfg(windows)]
    {
        if enabled {
            native_host::install(&extra_extension_ids.unwrap_or_default())?;
        } else {
            native_host::uninstall()?;
        }
        Ok(integration_status())
    }
    #[cfg(not(windows))]
    {
        let _ = (enabled, extra_extension_ids);
        Err(AppError::Internal("not supported".into()))
    }
}

// ---------------------------------------------------------------- windows

#[tauri::command]
pub fn show_overlay(app: AppHandle) {
    crate::window::show_overlay(&app);
}

#[tauri::command]
pub fn hide_overlay(app: AppHandle) {
    crate::window::hide_overlay(&app);
}

#[tauri::command]
pub fn hide_toolbar(app: AppHandle) {
    crate::window::hide_toolbar(&app);
}

/// Toolbar → overlay hand-off: opens the overlay and asks it to run an
/// action on a captured context (or attach it for "ask").
#[tauri::command]
pub fn toolbar_action(app: AppHandle, context_id: String, action: String) -> AppResult<()> {
    if !crate::integrations::context::ACTIONS.contains(&action.as_str()) {
        return Err(AppError::InvalidInput(format!("unknown action {action}")));
    }
    crate::window::hide_toolbar(&app);
    crate::window::show_overlay(&app);
    let _ = app.emit_to(crate::window::MAIN, "run-context-action", json!({"context_id": context_id, "action": action}));
    Ok(())
}

#[tauri::command]
pub fn capture_selection_now(app: AppHandle) {
    crate::window::trigger_selection_capture(app);
}
