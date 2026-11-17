//! Minimal MCP client over stdio (newline-delimited JSON-RPC 2.0).

use crate::config::McpServerConfig;
use crate::error::{AppError, AppResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

pub struct McpClient {
    stdin: AsyncMutex<ChildStdin>,
    child: AsyncMutex<Child>,
    pending: Pending,
    next_id: AtomicU64,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    pub server_info: Value,
}

impl McpClient {
    pub async fn connect(cfg: &McpServerConfig) -> AppResult<Arc<Self>> {
        let mut cmd = Command::new(&cfg.command);
        cmd.args(&cfg.args)
            .envs(&cfg.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = &cfg.cwd {
            cmd.current_dir(cwd);
        }
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd.spawn().map_err(|e| AppError::Mcp(format!("failed to start '{}': {e}", cfg.command)))?;
        let stdin = child.stdin.take().ok_or_else(|| AppError::Mcp("no stdin".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| AppError::Mcp("no stdout".into()))?;
        let stderr = child.stderr.take().ok_or_else(|| AppError::Mcp("no stderr".into()))?;

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
        let stdin = AsyncMutex::new(stdin);

        // stderr collector (diagnostics only)
        {
            let tail = stderr_tail.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    let mut t = tail.lock().unwrap();
                    t.push_back(l);
                    while t.len() > 20 {
                        t.pop_front();
                    }
                }
            });
        }

        let client = Self { stdin, child: AsyncMutex::new(child), pending: pending.clone(), next_id: AtomicU64::new(1), stderr_tail, server_info: Value::Null };

        // stdout reader: route responses to waiters; answer server requests.
        let (req_tx, mut req_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        {
            let pending = pending.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else { continue };
                    route_message(&pending, &req_tx, msg);
                }
                // process exited: fail all waiters
                for (_, tx) in pending.lock().unwrap().drain() {
                    let _ = tx.send(Err("server closed the connection".into()));
                }
            });
        }

        let mut client = client;
        let init = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "windows-ai", "version": env!("CARGO_PKG_VERSION")}
                }),
                Duration::from_secs(20),
            )
            .await?;
        client.server_info = init;
        client.notify("notifications/initialized", json!({})).await?;

        // Server-initiated requests: we answer ping and decline the rest.
        let client = Arc::new(client);
        let weak = Arc::downgrade(&client);
        tokio::spawn(async move {
            while let Some(req) = req_rx.recv().await {
                let Some(c) = weak.upgrade() else { break };
                let id = req["id"].clone();
                let resp = if req["method"] == "ping" {
                    json!({"jsonrpc": "2.0", "id": id, "result": {}})
                } else {
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not supported by Windows AI"}})
                };
                if c.write(&resp).await.is_err() {
                    break;
                }
            }
        });
        Ok(client)
    }

    async fn write(&self, msg: &Value) -> AppResult<()> {
        let mut line = serde_json::to_string(msg)?;
        line.push('\n');
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(line.as_bytes()).await.map_err(|e| AppError::Mcp(format!("write failed: {e}")))?;
        stdin.flush().await.map_err(|e| AppError::Mcp(format!("flush failed: {e}")))?;
        Ok(())
    }

    pub async fn notify(&self, method: &str, params: Value) -> AppResult<()> {
        self.write(&json!({"jsonrpc": "2.0", "method": method, "params": params})).await
    }

    pub async fn request(&self, method: &str, params: Value, timeout: Duration) -> AppResult<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await?;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(AppError::Mcp(format!("{method}: {e}{}", self.stderr_hint()))),
            Ok(Err(_)) => Err(AppError::Mcp(format!("{method}: connection closed{}", self.stderr_hint()))),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(AppError::Mcp(format!("{method}: timed out")))
            }
        }
    }

    fn stderr_hint(&self) -> String {
        let t = self.stderr_tail.lock().unwrap();
        match t.back() {
            Some(l) => format!(" (stderr: {l})"),
            None => String::new(),
        }
    }

    pub async fn list_tools(&self) -> AppResult<Vec<McpToolInfo>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let r = self.request("tools/list", params, Duration::from_secs(20)).await?;
            for t in r["tools"].as_array().cloned().unwrap_or_default() {
                out.push(McpToolInfo {
                    name: t["name"].as_str().unwrap_or_default().to_string(),
                    description: t["description"].as_str().unwrap_or_default().to_string(),
                    input_schema: t.get("inputSchema").cloned().unwrap_or(json!({"type": "object"})),
                });
            }
            cursor = r["nextCursor"].as_str().map(String::from);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    pub async fn call_tool(&self, name: &str, args: Value, timeout: Duration) -> AppResult<(String, bool)> {
        let r = self.request("tools/call", json!({"name": name, "arguments": args}), timeout).await?;
        Ok(render_tool_result(&r))
    }

    pub async fn shutdown(&self) {
        let _ = self.child.lock().await.kill().await;
    }
}

pub fn route_message(pending: &Pending, req_tx: &tokio::sync::mpsc::UnboundedSender<Value>, msg: Value) {
    let has_method = msg.get("method").is_some();
    match msg.get("id").and_then(Value::as_u64) {
        Some(id) if !has_method => {
            if let Some(tx) = pending.lock().unwrap().remove(&id) {
                let res = if let Some(err) = msg.get("error") {
                    Err(err["message"].as_str().unwrap_or("error").to_string())
                } else {
                    Ok(msg.get("result").cloned().unwrap_or(Value::Null))
                };
                let _ = tx.send(res);
            }
        }
        _ if has_method && msg.get("id").is_some() => {
            let _ = req_tx.send(msg);
        }
        _ => {} // notifications are ignored
    }
}

/// Flattens an MCP CallToolResult into text.
pub fn render_tool_result(r: &Value) -> (String, bool) {
    let is_error = r["isError"].as_bool().unwrap_or(false);
    let mut parts = Vec::new();
    for c in r["content"].as_array().cloned().unwrap_or_default() {
        match c["type"].as_str() {
            Some("text") => parts.push(c["text"].as_str().unwrap_or_default().to_string()),
            Some("resource") => parts.push(c["resource"]["text"].as_str().map(String::from).unwrap_or_else(|| format!("[resource {}]", c["resource"]["uri"]))),
            Some(other) => parts.push(format!("[{other} content omitted]")),
            None => {}
        }
    }
    if parts.is_empty() {
        if let Some(s) = r.get("structuredContent") {
            parts.push(s.to_string());
        }
    }
    (parts.join("\n"), is_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_results() {
        let (t, e) = render_tool_result(&json!({"content": [{"type": "text", "text": "a"}, {"type": "image", "data": ""}], "isError": true}));
        assert_eq!(t, "a\n[image content omitted]");
        assert!(e);
    }

    #[tokio::test]
    async fn routes_responses_and_requests() {
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (otx, orx) = oneshot::channel();
        pending.lock().unwrap().insert(7, otx);
        route_message(&pending, &tx, json!({"jsonrpc": "2.0", "id": 7, "result": {"ok": true}}));
        assert_eq!(orx.await.unwrap().unwrap()["ok"], true);
        route_message(&pending, &tx, json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}));
        assert_eq!(rx.recv().await.unwrap()["method"], "ping");
    }
}
