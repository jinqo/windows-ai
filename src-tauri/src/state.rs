//! Shared application state managed by Tauri.

use crate::automation::TaskRunner;
use crate::config::ConfigStore;
use crate::db::Db;
use crate::integrations::Inbox;
use crate::mcp::McpManager;
use crate::permissions::{Approvals, Notifier};
use crate::tools::executor::ToolHub;
use crate::voice::VoiceManager;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub config: Arc<ConfigStore>,
    pub db: Db,
    pub approvals: Arc<Approvals>,
    pub mcp: Arc<McpManager>,
    pub hub: Arc<ToolHub>,
    pub tasks: Arc<TaskRunner>,
    pub voice: Arc<VoiceManager>,
    pub inbox: Arc<Inbox>,
    pub runs: Mutex<HashMap<String, CancellationToken>>,
    pub notifier: Arc<dyn Notifier>,
    pub cli_runs: Arc<crate::ai::claude_code::CliRunRegistry>,
    pub data_dir: std::path::PathBuf,
    /// Explorer multi-select batching: (action, paths)
    pub explorer_batch: Mutex<Option<(String, Vec<String>)>>,
}

/// Emits backend events to every window.
pub struct TauriNotifier {
    pub app: AppHandle,
}

impl Notifier for TauriNotifier {
    fn emit(&self, event: &str, payload: Value) {
        if event == "settings-change-request" {
            // A tool asked to change a safe setting: persist through the store.
            if let Some(state) = self.app.try_state::<AppState>() {
                if let Ok(cfg) = serde_json::from_value(payload["config"].clone()) {
                    if state.config.set(cfg).is_ok() {
                        let _ = self.app.emit("config-changed", state.config.get());
                    }
                }
            }
            return;
        }
        let _ = self.app.emit(event, payload);
    }
}
