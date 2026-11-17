//! Local integration bridge: a per-user named pipe that the browser
//! native-messaging host and the VS Code extension use to hand captured
//! content to the running app.
//!
//! Security:
//! - The pipe rejects remote (network) clients.
//! - Every request must carry the bridge token, a random secret written to
//!   %LOCALAPPDATA%\WindowsAI\bridge.token (readable only by the user via
//!   the profile's default ACL). Requests without it are rejected.
//! - The bridge can only *deliver context* to the UI; it cannot run tools,
//!   send messages to the model, or read data back.

use super::context::CapturedContext;
use crate::error::{AppError, AppResult};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub const PROTOCOL_VERSION: u32 = 1;
const MAX_LINE: usize = 8 * 1024 * 1024;

pub fn bridge_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    base.join("WindowsAI")
}

pub fn token_path() -> PathBuf {
    bridge_dir().join("bridge.token")
}

pub fn pipe_name() -> String {
    let user: String = std::env::var("USERNAME")
        .unwrap_or_else(|_| "user".into())
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    format!(r"\\.\pipe\windows-ai-bridge-{user}")
}

/// Creates (or rotates) the bridge token. Returns it.
pub fn write_new_token() -> AppResult<String> {
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    std::fs::create_dir_all(bridge_dir())?;
    std::fs::write(token_path(), &token)?;
    Ok(token)
}

pub fn read_token() -> Option<String> {
    std::fs::read_to_string(token_path()).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub type CaptureHandler = Arc<dyn Fn(CapturedContext) + Send + Sync>;
/// Handles `run_*` requests from the Claude Code MCP helper. These are
/// authorized by a per-run token (checked by the handler), not the bridge token.
pub type RunHandler = Arc<dyn Fn(Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = Value> + Send>> + Send + Sync>;

/// Handles one request line. Pure function, unit-tested.
pub fn handle_request(line: &str, token: &str, max_chars: usize, enabled: bool, on_capture: &CaptureHandler) -> Value {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return json!({"ok": false, "error": format!("invalid JSON: {e}")}),
    };
    if !ct_eq(req["token"].as_str().unwrap_or(""), token) {
        return json!({"ok": false, "error": "unauthorized"});
    }
    match req["type"].as_str() {
        Some("ping") => json!({"ok": true, "version": PROTOCOL_VERSION, "app": "windows-ai", "enabled": enabled}),
        Some("capture") => {
            if !enabled {
                return json!({"ok": false, "error": "integrations are disabled in Windows AI settings"});
            }
            let ctx: CapturedContext = match serde_json::from_value(req["payload"].clone()) {
                Ok(c) => c,
                Err(e) => return json!({"ok": false, "error": format!("invalid payload: {e}")}),
            };
            if ctx.source == "selection" || ctx.source == "explorer" || ctx.source == "cli" {
                return json!({"ok": false, "error": "source not allowed over the bridge"});
            }
            match ctx.sanitize(max_chars) {
                Ok(c) => {
                    let id = c.id.clone();
                    on_capture(c);
                    json!({"ok": true, "id": id})
                }
                Err(e) => json!({"ok": false, "error": e}),
            }
        }
        other => json!({"ok": false, "error": format!("unknown request type {other:?}")}),
    }
}

/// Runs the pipe server until the process exits.
#[cfg(windows)]
pub async fn serve(token: String, settings: Arc<dyn Fn() -> (bool, usize) + Send + Sync>, on_capture: CaptureHandler, on_run: RunHandler) -> AppResult<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::ServerOptions;

    let name = pipe_name();
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .reject_remote_clients(true)
        .create(&name)
        .map_err(|e| AppError::Internal(format!("bridge pipe {name}: {e}")))?;
    loop {
        server.connect().await.map_err(|e| AppError::Internal(format!("bridge accept: {e}")))?;
        let conn = server;
        server = ServerOptions::new().reject_remote_clients(true).create(&name).map_err(|e| AppError::Internal(format!("bridge pipe: {e}")))?;
        let token = token.clone();
        let settings = settings.clone();
        let on_capture = on_capture.clone();
        let on_run = on_run.clone();
        tokio::spawn(async move {
            let (r, mut w) = tokio::io::split(conn);
            let mut reader = tokio::io::BufReader::new(r);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) if n > MAX_LINE => {
                        let _ = w.write_all(b"{\"ok\":false,\"error\":\"message too large\"}\n").await;
                        break;
                    }
                    Ok(_) => {
                        let (enabled, max) = settings();
                        let parsed: Option<Value> = serde_json::from_str(line.trim()).ok();
                        let is_run = parsed.as_ref().and_then(|v| v["type"].as_str()).map(|t| t.starts_with("run_")).unwrap_or(false);
                        let resp = if is_run {
                            on_run(parsed.unwrap_or(Value::Null)).await
                        } else {
                            handle_request(line.trim(), &token, max, enabled, &on_capture)
                        };
                        let mut out = resp.to_string();
                        out.push('\n');
                        if w.write_all(out.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
}

/// Synchronous client used by the native-messaging host and the CLI.
pub fn send_request(mut req: Value) -> AppResult<Value> {
    let is_run = req["type"].as_str().map(|t| t.starts_with("run_")).unwrap_or(false);
    if !is_run {
        let token = read_token().ok_or_else(|| AppError::NotFound("Windows AI is not running (no bridge token)".into()))?;
        req["token"] = json!(token);
    }
    let name = pipe_name();
    let mut last_err = None;
    for _ in 0..20 {
        match std::fs::OpenOptions::new().read(true).write(true).open(&name) {
            Ok(mut f) => {
                let mut line = req.to_string();
                line.push('\n');
                f.write_all(line.as_bytes())?;
                f.flush()?;
                let mut reader = BufReader::new(f);
                let mut resp = String::new();
                reader.read_line(&mut resp)?;
                return serde_json::from_str(resp.trim()).map_err(AppError::from);
            }
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
    }
    Err(AppError::NotFound(format!("Windows AI bridge unavailable: {}", last_err.map(|e| e.to_string()).unwrap_or_default())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn handler() -> (CaptureHandler, Arc<Mutex<Vec<CapturedContext>>>) {
        let got = Arc::new(Mutex::new(Vec::new()));
        let g2 = got.clone();
        (Arc::new(move |c| g2.lock().unwrap().push(c)), got)
    }

    #[test]
    fn requires_token() {
        let (h, got) = handler();
        let r = handle_request(r#"{"type":"ping","token":"wrong"}"#, "secret", 100, true, &h);
        assert_eq!(r["error"], "unauthorized");
        let r = handle_request(r#"{"type":"ping"}"#, "secret", 100, true, &h);
        assert_eq!(r["ok"], false);
        let r = handle_request(r#"{"type":"ping","token":"secret"}"#, "secret", 100, true, &h);
        assert_eq!(r["ok"], true);
        assert!(got.lock().unwrap().is_empty());
    }

    #[test]
    fn delivers_sanitized_capture() {
        let (h, got) = handler();
        let line = json!({"token": "t", "type": "capture", "payload": {"source": "browser", "kind": "page", "title": "Docs", "url": "https://x.dev", "content": "abcdef"}}).to_string();
        let r = handle_request(&line, "t", 3, true, &h);
        assert_eq!(r["ok"], true, "{r}");
        let g = got.lock().unwrap();
        assert_eq!(g[0].content, "abc");
        assert!(g[0].truncated);
    }

    #[test]
    fn rejects_disabled_spoofed_and_malformed() {
        let (h, got) = handler();
        let cap = json!({"token": "t", "type": "capture", "payload": {"source": "browser", "kind": "page", "content": "x"}}).to_string();
        assert_eq!(handle_request(&cap, "t", 10, false, &h)["ok"], false);
        let spoof = json!({"token": "t", "type": "capture", "payload": {"source": "explorer", "kind": "files", "content": "x"}}).to_string();
        assert_eq!(handle_request(&spoof, "t", 10, true, &h)["ok"], false);
        assert_eq!(handle_request("{nope", "t", 10, true, &h)["ok"], false);
        assert!(got.lock().unwrap().is_empty());
    }

    #[test]
    fn pipe_name_is_per_user_and_safe() {
        let n = pipe_name();
        assert!(n.starts_with(r"\\.\pipe\windows-ai-bridge-"));
        assert!(!n[9..].contains('\\'));
    }

    /// Full round trip over a real named pipe.
    #[cfg(windows)]
    #[tokio::test]
    async fn pipe_round_trip() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};
        let name = format!(r"\\.\pipe\windows-ai-test-{}", uuid::Uuid::new_v4().simple());
        let server = ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create(&name).unwrap();
        let (h, got) = handler();
        let srv = tokio::spawn(async move {
            server.connect().await.unwrap();
            let (r, mut w) = tokio::io::split(server);
            let mut line = String::new();
            tokio::io::BufReader::new(r).read_line(&mut line).await.unwrap();
            let resp = handle_request(line.trim(), "tok", 100, true, &h);
            w.write_all(format!("{resp}\n").as_bytes()).await.unwrap();
        });
        let client = ClientOptions::new().open(&name).unwrap();
        let (r, mut w) = tokio::io::split(client);
        let req = json!({"token": "tok", "type": "capture", "payload": {"source": "vscode", "kind": "code", "language": "rust", "content": "fn a(){}"}});
        w.write_all(format!("{req}\n").as_bytes()).await.unwrap();
        let mut resp = String::new();
        tokio::io::BufReader::new(r).read_line(&mut resp).await.unwrap();
        srv.await.unwrap();
        assert!(resp.contains("\"ok\":true"), "{resp}");
        assert_eq!(got.lock().unwrap()[0].language.as_deref(), Some("rust"));
    }
}
