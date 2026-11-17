use crate::activity::{self, ActivityEntry};
use crate::automation::store::{self, Task};
use crate::config::PermissionMode;
use crate::error::{AppError, AppResult};
use crate::permissions::{effective_mode, ApprovalRequest, Decision};
use crate::state::AppState;
use crate::tools::ToolContext;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

#[derive(Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub access: crate::permissions::Access,
    pub default_mode: PermissionMode,
    pub mode: PermissionMode,
    pub locked_ask: bool,
    pub origin: String,
}

#[tauri::command]
pub fn list_tools(state: State<AppState>) -> Vec<ToolInfo> {
    let cfg = state.config.get();
    state
        .hub
        .all_tools()
        .into_iter()
        .map(|t| {
            let m = t.meta();
            ToolInfo {
                mode: effective_mode(&cfg, &m.name, m.default_mode, m.locked_ask),
                name: m.name,
                description: m.description,
                input_schema: m.input_schema,
                access: m.access,
                default_mode: m.default_mode,
                locked_ask: m.locked_ask,
                origin: m.origin,
            }
        })
        .collect()
}

#[tauri::command]
pub fn set_tool_mode(app: AppHandle, state: State<AppState>, tool: String, mode: PermissionMode) -> AppResult<()> {
    let locked = state.hub.find(&tool).map(|t| t.meta().locked_ask).unwrap_or(false);
    if locked && mode == PermissionMode::Allow {
        return Err(AppError::PermissionDenied(format!("{tool} always requires confirmation")));
    }
    let cfg = state.config.update(|c| {
        c.permissions.tool_modes.insert(tool.clone(), mode);
    })?;
    let _ = app.emit("config-changed", &cfg);
    Ok(())
}

#[tauri::command]
pub fn resolve_approval(state: State<AppState>, id: String, decision: Decision) -> AppResult<()> {
    state.approvals.resolve(&id, decision)
}

#[tauri::command]
pub fn pending_approvals(state: State<AppState>) -> Vec<ApprovalRequest> {
    state.approvals.pending()
}

#[tauri::command]
pub fn clear_session_approvals(state: State<AppState>) {
    state.approvals.clear_session();
}

#[tauri::command]
pub fn list_activity(state: State<AppState>, status: Option<String>, limit: Option<u32>) -> AppResult<Vec<ActivityEntry>> {
    activity::list(&state.db, status.as_deref(), limit.unwrap_or(300))
}

#[tauri::command]
pub fn clear_activity(state: State<AppState>) -> AppResult<()> {
    activity::clear(&state.db)
}

#[tauri::command]
pub fn list_tasks(state: State<AppState>) -> AppResult<Vec<Task>> {
    store::list_tasks(&state.db, 100)
}

#[tauri::command]
pub fn get_task(state: State<AppState>, id: String) -> AppResult<Task> {
    store::get_task(&state.db, &id)
}

/// Starts or resumes a task in the background.
#[tauri::command]
pub fn run_task(state: State<AppState>, id: String) -> AppResult<()> {
    if state.tasks.is_running(&id) {
        return Err(AppError::InvalidInput("task already running".into()));
    }
    let task = store::get_task(&state.db, &id)?;
    let ctx = ToolContext {
        cfg: state.config.get(),
        db: state.db.clone(),
        notifier: state.notifier.clone(),
        cancel: CancellationToken::new(),
        conversation_id: task.conversation_id.clone(),
    };
    let hub = state.hub.clone();
    let runner = state.tasks.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = runner.run(&hub, ctx, &id).await {
            log::warn!("task {id}: {e}");
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_task(app: AppHandle, state: State<AppState>, id: String) -> AppResult<()> {
    state.tasks.cancel(&state.db, &id)?;
    let _ = app.emit("task-updated", serde_json::json!({"task_id": id}));
    Ok(())
}

#[tauri::command]
pub fn skip_task_step(app: AppHandle, state: State<AppState>, id: String) -> AppResult<()> {
    state.tasks.skip_current(&state.db, &id)?;
    let _ = app.emit("task-updated", serde_json::json!({"task_id": id}));
    Ok(())
}

#[tauri::command]
pub fn delete_task(state: State<AppState>, id: String) -> AppResult<()> {
    if state.tasks.is_running(&id) {
        return Err(AppError::InvalidInput("cancel the task first".into()));
    }
    store::delete_task(&state.db, &id)
}
