//! Windows AI — Tauri 2 backend entry point.

pub mod activity;
pub mod ai;
pub mod automation;
pub mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod integrations;
pub mod mcp;
pub mod memory;
pub mod permissions;
pub mod secrets;
pub mod state;
pub mod tools;
pub mod voice;
pub mod window;

use commands::*;
use serde_json::{json, Value};
use state::{AppState, TauriNotifier};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, WindowEvent};

/// Alternate process modes that must run before Tauri starts.
/// Returns true if the process handled a mode and should exit.
pub fn run_special_mode(args: &[String]) -> bool {
    if integrations::native_host::is_native_host_invocation(args) {
        integrations::native_host::run();
        return true;
    }
    if integrations::mcp_stdio::is_mcp_invocation(args) {
        integrations::mcp_stdio::run();
        return true;
    }
    false
}

/// Handles `run_*` bridge requests from the Claude Code MCP helper.
fn make_run_handler(app: tauri::AppHandle) -> integrations::bridge::RunHandler {
    Arc::new(move |req: Value| {
        let app = app.clone();
        Box::pin(async move {
            let state = app.state::<AppState>();
            let token = req["run_token"].as_str().unwrap_or_default();
            let Some((conversation_id, cancel)) = state.cli_runs.lookup(token) else {
                return json!({"ok": false, "error": "unauthorized: no active Windows AI run for this token"});
            };
            let cfg = state.config.get();
            match req["type"].as_str() {
                Some("run_tools_list") => {
                    let tools: Vec<Value> = state
                        .hub
                        .offered(&cfg)
                        .into_iter()
                        .map(|m| json!({"name": m.name, "description": m.description, "inputSchema": m.input_schema}))
                        .collect();
                    json!({"ok": true, "tools": tools})
                }
                Some("run_tool_call") => {
                    let name = req["name"].as_str().unwrap_or_default().to_string();
                    let input = req.get("arguments").cloned().unwrap_or(json!({}));
                    let ctx = tools::ToolContext { cfg, db: state.db.clone(), notifier: state.notifier.clone(), cancel, conversation_id: Some(conversation_id) };
                    let out = state.hub.execute(&ctx, &name, input, "chat", None).await;
                    json!({"ok": true, "content": out.content, "is_error": out.is_error, "status": out.status})
                }
                _ => json!({"ok": false, "error": "unknown run request"}),
            }
        })
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger_init();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            window::handle_cli(app, &argv);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--background"])))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| window::on_shortcut(app, shortcut, event.state()))
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let config = Arc::new(config::ConfigStore::load(config_dir.join("config.json")));
            let db = db::Db::open(&data_dir.join("windows-ai.db")).map_err(|e| e.to_string())?;
            let approvals = permissions::Approvals::new();
            let mcp = Arc::new(mcp::McpManager::new());
            mcp.sync_config(&config.get().mcp.servers);
            let hub = Arc::new(tools::executor::ToolHub::new(mcp.clone(), approvals.clone()));
            let notifier: Arc<dyn permissions::Notifier> = Arc::new(TauriNotifier { app: handle.clone() });
            let inbox = Arc::new(integrations::Inbox::default());
            app.manage(AppState {
                config: config.clone(),
                db,
                approvals,
                mcp: mcp.clone(),
                hub,
                tasks: automation::TaskRunner::new(),
                voice: voice::VoiceManager::new(),
                inbox: inbox.clone(),
                runs: Mutex::new(HashMap::new()),
                notifier,
                cli_runs: Arc::new(ai::claude_code::CliRunRegistry::default()),
                data_dir: data_dir.clone(),
                explorer_batch: Mutex::new(None),
            });

            let cfg = config.get();
            window::apply_window_config(&handle, &cfg);
            let _ = window::apply_shortcuts(&handle, &cfg);
            window::build_tray(&handle)?;

            // Keep autostart registration in sync with the setting.
            {
                use tauri_plugin_autostart::ManagerExt;
                let al = handle.autolaunch();
                let enabled = al.is_enabled().unwrap_or(false);
                if cfg.startup.launch_on_login != enabled {
                    let _ = if cfg.startup.launch_on_login { al.enable() } else { al.disable() };
                }
            }

            // Integration bridge (browser + VS Code + Claude Code MCP helper).
            #[cfg(windows)]
            match integrations::bridge::write_new_token() {
                Ok(token) => {
                    let cfg_for_bridge = config.clone();
                    let settings: Arc<dyn Fn() -> (bool, usize) + Send + Sync> = Arc::new(move || {
                        let c = cfg_for_bridge.get();
                        (c.integrations.bridge_enabled, c.integrations.max_context_chars as usize)
                    });
                    let app_for_capture = handle.clone();
                    let on_capture: integrations::bridge::CaptureHandler = Arc::new(move |ctx| {
                        let state = app_for_capture.state::<AppState>();
                        state.inbox.push(ctx.clone());
                        let _ = app_for_capture.emit("context-captured", &ctx);
                        let a = app_for_capture.clone();
                        let _ = app_for_capture.run_on_main_thread(move || window::show_overlay(&a));
                    });
                    let on_run = make_run_handler(handle.clone());
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = integrations::bridge::serve(token, settings, on_capture, on_run).await {
                            log::error!("integration bridge stopped: {e}");
                        }
                    });
                }
                Err(e) => log::error!("bridge token: {e}"),
            }

            // Connect enabled MCP servers in the background.
            tauri::async_runtime::spawn(async move { mcp.connect_enabled().await });

            // Startup arguments (e.g. launched from Explorer or autostart).
            let args: Vec<String> = std::env::args().collect();
            // No arguments shows the overlay unless "start hidden" is on;
            // --background stays in the tray; Explorer verbs are queued.
            if !(cfg.startup.start_hidden && args.len() <= 1) {
                window::handle_cli(&handle, &args);
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                // Close hides to the tray; quit is in the tray menu.
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) if window.label() == window::TOOLBAR => {
                let _ = window.hide();
            }
            WindowEvent::Focused(false) if window.label() == window::MAIN => {
                let app = window.app_handle();
                if app.state::<AppState>().config.get().window.hide_on_blur && app.state::<AppState>().approvals.pending().is_empty() {
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            config_cmds::get_config,
            config_cmds::save_config,
            config_cmds::secret_status,
            config_cmds::set_secret,
            config_cmds::delete_secret,
            config_cmds::app_info,
            chat::list_conversations,
            chat::create_conversation,
            chat::get_messages,
            chat::rename_conversation,
            chat::pin_conversation,
            chat::set_conversation_project,
            chat::delete_conversation,
            chat::send_message,
            chat::regenerate,
            chat::cancel_run,
            chat::provider_status,
            chat::list_models,
            chat::claude_code_status,
            chat::claude_code_login,
            memory_cmds::list_memories,
            memory_cmds::create_memory,
            memory_cmds::update_memory,
            memory_cmds::delete_memory,
            memory_cmds::detect_sensitive,
            memory_cmds::list_projects,
            memory_cmds::create_project,
            memory_cmds::update_project,
            memory_cmds::delete_project,
            tools_cmds::list_tools,
            tools_cmds::set_tool_mode,
            tools_cmds::resolve_approval,
            tools_cmds::pending_approvals,
            tools_cmds::clear_session_approvals,
            tools_cmds::list_activity,
            tools_cmds::clear_activity,
            tools_cmds::list_tasks,
            tools_cmds::get_task,
            tools_cmds::run_task,
            tools_cmds::cancel_task,
            tools_cmds::skip_task_step,
            tools_cmds::delete_task,
            system_cmds::mcp_statuses,
            system_cmds::mcp_connect,
            system_cmds::mcp_disconnect,
            system_cmds::mcp_set_tool_trust,
            system_cmds::voice_start,
            system_cmds::voice_stop,
            system_cmds::voice_cancel,
            system_cmds::voice_transcribe,
            system_cmds::inbox_list,
            system_cmds::inbox_remove,
            system_cmds::inbox_clear,
            system_cmds::capture_clipboard,
            system_cmds::copy_text,
            system_cmds::integration_status,
            system_cmds::set_explorer_menu,
            system_cmds::set_browser_host,
            system_cmds::show_overlay,
            system_cmds::hide_overlay,
            system_cmds::hide_toolbar,
            system_cmds::toolbar_action,
            system_cmds::capture_selection_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Windows AI");
}

fn env_logger_init() -> Result<(), log::SetLoggerError> {
    struct Stderr;
    impl log::Log for Stderr {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Info
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) {
                eprintln!("[{}] {}: {}", r.level(), r.target(), r.args());
            }
        }
        fn flush(&self) {}
    }
    log::set_boxed_logger(Box::new(Stderr)).map(|()| log::set_max_level(log::LevelFilter::Info))
}
