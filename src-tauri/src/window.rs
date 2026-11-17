//! Overlay and toolbar window management, global shortcuts and tray.

use crate::config::AppConfig;
use crate::error::AppResult;
use crate::integrations::{explorer::CliCommand, selection};
use crate::state::AppState;
use serde_json::json;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const MAIN: &str = "main";
pub const TOOLBAR: &str = "toolbar";

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN)
}

pub fn apply_window_config(app: &AppHandle, cfg: &AppConfig) {
    if let Some(w) = main_window(app) {
        let _ = w.set_always_on_top(cfg.window.always_on_top);
        let _ = w.set_size(tauri::LogicalSize::new(cfg.window.width as f64, cfg.window.height as f64));
    }
}

fn position_main(app: &AppHandle, w: &WebviewWindow, cfg: &AppConfig) {
    match cfg.window.position.as_str() {
        "remember" => {
            if let (Some(x), Some(y)) = (cfg.window.x, cfg.window.y) {
                let _ = w.set_position(PhysicalPosition::new(x, y));
                return;
            }
            let _ = w.center();
        }
        "top" => {
            let monitor = cursor_monitor(app).or_else(|| w.current_monitor().ok().flatten());
            if let (Some(m), Ok(size)) = (monitor, w.outer_size()) {
                let area = m.work_area();
                let x = area.position.x + (area.size.width as i32 - size.width as i32) / 2;
                let y = area.position.y + (area.size.height as f64 * 0.08) as i32;
                let _ = w.set_position(PhysicalPosition::new(x, y));
            } else {
                let _ = w.center();
            }
        }
        _ => {
            // Center on the monitor under the cursor.
            let monitor = cursor_monitor(app);
            if let (Some(m), Ok(size)) = (monitor, w.outer_size()) {
                let area = m.work_area();
                let x = area.position.x + (area.size.width as i32 - size.width as i32) / 2;
                let y = area.position.y + (area.size.height as i32 - size.height as i32) / 2;
                let _ = w.set_position(PhysicalPosition::new(x, y));
            } else {
                let _ = w.center();
            }
        }
    }
}

fn cursor_monitor(app: &AppHandle) -> Option<tauri::Monitor> {
    let p = app.cursor_position().ok()?;
    app.monitor_from_point(p.x, p.y).ok().flatten()
}

pub fn show_overlay(app: &AppHandle) {
    let Some(w) = main_window(app) else { return };
    let cfg = app.state::<AppState>().config.get();
    if !w.is_visible().unwrap_or(false) {
        position_main(app, &w, &cfg);
    }
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
    let _ = app.emit_to(MAIN, "overlay-shown", json!({}));
}

pub fn hide_overlay(app: &AppHandle) {
    let Some(w) = main_window(app) else { return };
    let state = app.state::<AppState>();
    if state.config.get().window.position == "remember" {
        if let Ok(p) = w.outer_position() {
            let _ = state.config.update(|c| {
                c.window.x = Some(p.x);
                c.window.y = Some(p.y);
            });
        }
    }
    let _ = w.hide();
}

pub fn toggle_overlay(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
            hide_overlay(app);
        } else {
            show_overlay(app);
        }
    }
}

pub fn navigate(app: &AppHandle, view: &str) {
    show_overlay(app);
    let _ = app.emit_to(MAIN, "navigate", json!({ "view": view }));
}

/// Shows the floating selection toolbar just below the cursor, clamped to
/// the monitor's work area.
pub fn show_toolbar_at(app: &AppHandle, x: i32, y: i32) {
    let Some(w) = app.get_webview_window(TOOLBAR) else { return };
    let size = w.outer_size().unwrap_or(PhysicalSize::new(460, 64));
    let (mut px, mut py) = (x - 40, y + 18);
    if let Ok(Some(m)) = app.monitor_from_point(x as f64, y as f64) {
        let a = m.work_area();
        let (ax, ay, aw, ah) = (a.position.x, a.position.y, a.size.width as i32, a.size.height as i32);
        px = px.clamp(ax, ax + aw - size.width as i32);
        if py + size.height as i32 > ay + ah {
            py = y - size.height as i32 - 18;
        }
        py = py.clamp(ay, ay + ah - size.height as i32);
    }
    let _ = w.set_position(PhysicalPosition::new(px, py));
    let _ = w.show();
    let _ = w.set_focus();
    let _ = app.emit_to(TOOLBAR, "toolbar-shown", json!({}));
}

pub fn hide_toolbar(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(TOOLBAR) {
        let _ = w.hide();
    }
}

/// (Re)registers global shortcuts from config.
pub fn apply_shortcuts(app: &AppHandle, cfg: &AppConfig) -> AppResult<Vec<String>> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut errors = Vec::new();
    match cfg.shortcuts.toggle_overlay.parse::<Shortcut>() {
        Ok(s) => {
            if let Err(e) = gs.register(s) {
                errors.push(format!("{}: {e}", cfg.shortcuts.toggle_overlay));
            }
        }
        Err(e) => errors.push(format!("{}: {e}", cfg.shortcuts.toggle_overlay)),
    }
    if cfg.integrations.selection_enabled {
        match cfg.integrations.selection_shortcut.parse::<Shortcut>() {
            Ok(s) => {
                if let Err(e) = gs.register(s) {
                    errors.push(format!("{}: {e}", cfg.integrations.selection_shortcut));
                }
            }
            Err(e) => errors.push(format!("{}: {e}", cfg.integrations.selection_shortcut)),
        }
    }
    if !errors.is_empty() {
        log::warn!("shortcut registration: {errors:?}");
        let _ = app.emit("shortcut-error", json!({ "errors": errors }));
    }
    Ok(errors)
}

pub fn on_shortcut(app: &AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    if state != ShortcutState::Pressed {
        return;
    }
    let cfg = app.state::<AppState>().config.get();
    if cfg.shortcuts.toggle_overlay.parse::<Shortcut>().ok().as_ref() == Some(shortcut) {
        toggle_overlay(app);
    } else if cfg.integrations.selection_shortcut.parse::<Shortcut>().ok().as_ref() == Some(shortcut) {
        trigger_selection_capture(app.clone());
    }
}

/// Captures the selection in the foreground app and opens the toolbar or overlay.
pub fn trigger_selection_capture(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let cfg = app.state::<AppState>().config.get();
        #[cfg(windows)]
        let (cx, cy) = selection::cursor_position();
        #[cfg(not(windows))]
        let (cx, cy) = (0, 0);
        let result = tauri::async_runtime::spawn_blocking(move || {
            selection::capture_selection(cfg.integrations.clipboard_fallback, cfg.integrations.max_context_chars as usize)
        })
        .await;
        let cfg = app.state::<AppState>().config.get();
        match result {
            Ok(Ok(ctx)) => {
                let state = app.state::<AppState>();
                state.inbox.push(ctx.clone());
                let _ = app.emit("context-captured", &ctx);
                if cfg.integrations.selection_mode == "toolbar" {
                    show_toolbar_at(&app, cx, cy);
                } else {
                    show_overlay(&app);
                }
            }
            Ok(Err(e)) => {
                let _ = app.emit("capture-error", json!({ "error": e }));
                if cfg.integrations.selection_mode == "toolbar" {
                    show_toolbar_at(&app, cx, cy);
                } else {
                    show_overlay(&app);
                }
            }
            Err(e) => log::error!("selection capture panicked: {e}"),
        }
    });
}

/// Handles command-line requests (startup args and second-instance forwards).
pub fn handle_cli(app: &AppHandle, args: &[String]) {
    match crate::integrations::explorer::parse_cli(args) {
        Some(CliCommand::Show) => show_overlay(app),
        Some(CliCommand::Background) => {}
        Some(CliCommand::CaptureSelection) => trigger_selection_capture(app.clone()),
        Some(CliCommand::FileAction { action, paths }) => {
            for path in paths {
                queue_explorer_file(app, action.clone(), path);
            }
        }
        None => {
            if args.len() <= 1 {
                show_overlay(app);
            }
        }
    }
}

fn queue_explorer_file(app: &AppHandle, action: String, path: String) {
    let state = app.state::<AppState>();
    let start_timer = {
        let mut b = state.explorer_batch.lock().unwrap();
        match b.as_mut() {
            Some((_, paths)) => {
                if !paths.contains(&path) {
                    paths.push(path);
                }
                false
            }
            None => {
                *b = Some((action, vec![path]));
                true
            }
        }
    };
    if !start_timer {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Explorer launches one process per selected item; collect them.
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        let state = app.state::<AppState>();
        let Some((action, paths)) = state.explorer_batch.lock().unwrap().take() else { return };
        let cfg = state.config.get();
        let max_file = cfg.permissions.max_read_bytes;
        let max_total = cfg.integrations.max_context_chars as usize;
        let built = tauri::async_runtime::spawn_blocking(move || crate::integrations::explorer::build_file_context(&paths, &action, max_file, max_total)).await;
        match built {
            Ok(Ok(ctx)) => {
                state.inbox.push(ctx.clone());
                let _ = app.emit("context-captured", &ctx);
            }
            Ok(Err(e)) => {
                let _ = app.emit("capture-error", json!({ "error": e }));
            }
            Err(_) => {}
        }
        show_overlay(&app);
    });
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Windows AI", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let activity = MenuItem::with_id(app, "activity", "Activity", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Windows AI", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &settings, &activity, &PredefinedMenuItem::separator(app)?, &quit])?;
    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("Windows AI")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_overlay(app),
            "settings" => navigate(app, "settings"),
            "activity" => navigate(app, "activity"),
            "quit" => {
                let state = app.state::<AppState>();
                let mcp = state.mcp.clone();
                tauri::async_runtime::block_on(async move { mcp.shutdown_all().await });
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_overlay(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
