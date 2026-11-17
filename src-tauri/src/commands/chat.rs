use crate::ai::{agent, claude_code, provider, types::*};
use crate::error::{AppError, AppResult};
use crate::integrations::context::compose_prompt;
use crate::memory::{self, Conversation, StoredMessage};
use crate::state::AppState;
use serde::Serialize;
use tauri::State;
use tokio_util::sync::CancellationToken;

#[tauri::command]
pub fn list_conversations(state: State<AppState>, search: Option<String>) -> AppResult<Vec<Conversation>> {
    memory::list_conversations(&state.db, search.as_deref(), 200)
}

#[tauri::command]
pub fn create_conversation(state: State<AppState>, title: Option<String>, project_id: Option<String>) -> AppResult<Conversation> {
    memory::create_conversation(&state.db, title.as_deref().unwrap_or(""), project_id.as_deref())
}

#[tauri::command]
pub fn get_messages(state: State<AppState>, conversation_id: String) -> AppResult<Vec<StoredMessage>> {
    memory::list_messages(&state.db, &conversation_id)
}

#[tauri::command]
pub fn rename_conversation(state: State<AppState>, id: String, title: String) -> AppResult<()> {
    memory::rename_conversation(&state.db, &id, &title)
}

#[tauri::command]
pub fn pin_conversation(state: State<AppState>, id: String, pinned: bool) -> AppResult<()> {
    memory::set_conversation_pinned(&state.db, &id, pinned)
}

#[tauri::command]
pub fn set_conversation_project(state: State<AppState>, id: String, project_id: Option<String>) -> AppResult<()> {
    memory::set_conversation_project(&state.db, &id, project_id.as_deref())
}

#[tauri::command]
pub fn delete_conversation(state: State<AppState>, id: String) -> AppResult<()> {
    memory::delete_conversation(&state.db, &id)
}

#[derive(Serialize)]
pub struct SendResult {
    pub conversation_id: String,
    pub run_id: String,
    pub user_message: StoredMessage,
}

fn start_run(state: &AppState, conversation_id: &str) -> AppResult<String> {
    let cfg = state.config.get();
    let provider = provider::build_provider(&cfg)?;
    let run_id = crate::db::new_id();
    let cancel = CancellationToken::new();
    state.runs.lock().unwrap().insert(run_id.clone(), cancel.clone());
    let params = agent::RunParams {
        run_id: run_id.clone(),
        conversation_id: conversation_id.to_string(),
        cfg,
        db: state.db.clone(),
        provider,
        hub: state.hub.clone(),
        notifier: state.notifier.clone(),
        cancel,
        data_dir: state.data_dir.clone(),
        cli_runs: Some(state.cli_runs.clone()),
    };
    tauri::async_runtime::spawn(async move {
        let _ = agent::run(params).await;
    });
    Ok(run_id)
}

/// Sends a user message (optionally with captured contexts and a quick
/// action) and starts the agent. Streaming arrives as `chat-stream` events.
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    conversation_id: Option<String>,
    text: String,
    context_ids: Option<Vec<String>>,
    action: Option<String>,
) -> AppResult<SendResult> {
    let cfg = state.config.get();
    let contexts = state.inbox.take(&context_ids.unwrap_or_default());
    let prompt = compose_prompt(&contexts, action.as_deref(), &text, &cfg.integrations.translate_language);
    if prompt.trim().is_empty() {
        return Err(AppError::InvalidInput("message is empty".into()));
    }
    let conv_id = match conversation_id {
        Some(id) => memory::get_conversation(&state.db, &id)?.id,
        None => {
            let title_src = if !text.trim().is_empty() {
                text.clone()
            } else {
                let c = contexts.first();
                let label = action.clone().unwrap_or_else(|| "ask".into());
                let what = c.and_then(|c| c.title.clone().or(c.app.clone())).unwrap_or_else(|| "captured content".into());
                format!("{}{}: {what}", label[..1].to_uppercase(), &label[1..])
            };
            memory::create_conversation(&state.db, &agent::auto_title(&title_src), None)?.id
        }
    };
    let msg = ChatMessage { role: Role::User, content: vec![ContentBlock::Text { text: prompt }] };
    let stored = memory::append_message(&state.db, &conv_id, &msg, None)?;
    let run_id = start_run(&state, &conv_id)?;
    Ok(SendResult { conversation_id: conv_id, run_id, user_message: stored })
}

/// Re-runs the agent from the given user message (deleting everything after it).
#[tauri::command]
pub async fn regenerate(state: State<'_, AppState>, conversation_id: String, from_message_id: String) -> AppResult<String> {
    let msgs = memory::list_messages(&state.db, &conversation_id)?;
    let idx = msgs.iter().position(|m| m.id == from_message_id).ok_or_else(|| AppError::NotFound("message".into()))?;
    if let Some(next) = msgs.get(idx + 1) {
        memory::delete_messages_after(&state.db, &conversation_id, &next.id)?;
    }
    start_run(&state, &conversation_id)
}

#[tauri::command]
pub fn cancel_run(state: State<AppState>, run_id: String) -> AppResult<()> {
    if let Some(t) = state.runs.lock().unwrap().remove(&run_id) {
        t.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn provider_status(state: State<'_, AppState>) -> AppResult<provider::ProviderStatus> {
    let cfg = state.config.get();
    match provider::build_provider(&cfg)? {
        Some(p) => Ok(p.check().await),
        None => {
            let st = claude_code::status(&cfg.ai.claude_code).await;
            let (s, detail) = if !st.installed {
                ("missing_cli", st.error.clone().unwrap_or_default())
            } else if !st.logged_in {
                ("missing_credentials", "Claude Code is not signed in".to_string())
            } else if st.billing != "subscription" && !cfg.ai.claude_code.allow_api_billing {
                ("error", "Claude Code is signed in with API billing; subscription mode will not use it".to_string())
            } else {
                ("connected", format!("Claude {} subscription via Claude Code {}", st.subscription_type.clone().unwrap_or_default(), st.version.clone().unwrap_or_default()))
            };
            let model = if cfg.ai.claude_code.model.is_empty() { "default".into() } else { cfg.ai.claude_code.model.clone() };
            Ok(provider::ProviderStatus { provider: "claude_code".into(), model, state: s.into(), detail })
        }
    }
}

#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> AppResult<Vec<provider::ModelInfo>> {
    let cfg = state.config.get();
    match provider::build_provider(&cfg)? {
        Some(p) => p.list_models().await,
        None => Ok(claude_code::model_choices()),
    }
}

/// Full Claude Code CLI status (install, sign-in, billing mode).
#[tauri::command]
pub async fn claude_code_status(state: State<'_, AppState>) -> AppResult<claude_code::CliStatus> {
    claude_code::invalidate_auth_cache();
    Ok(claude_code::status(&state.config.get().ai.claude_code).await)
}

/// Opens the official `claude auth login` flow in a console window.
#[tauri::command]
pub fn claude_code_login(state: State<AppState>) -> AppResult<()> {
    claude_code::invalidate_auth_cache();
    claude_code::launch_login(&state.config.get().ai.claude_code)
}
