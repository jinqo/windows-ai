//! `windows-ai.exe --mcp-stdio`: a tiny MCP server that Claude Code spawns
//! (via a per-run --mcp-config) in subscription mode. It exposes Windows AI's
//! tools by forwarding to the running app over the bridge, authorized by the
//! per-run token in WINDOWS_AI_RUN_TOKEN. Every call is executed by the app's
//! permission executor, so approvals, scopes and logging all apply.

use super::bridge;
use crate::error::AppResult;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub fn is_mcp_invocation(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a == "--mcp-stdio")
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn err(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Handles one JSON-RPC message. Returns None for notifications.
pub fn handle(msg: &Value, token: &str, forward: &dyn Fn(Value) -> AppResult<Value>) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg["method"].as_str().unwrap_or_default();
    Some(match method {
        "initialize" => ok(
            &id,
            json!({
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSION),
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "windows-ai", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Windows AI tools. Sensitive actions ask the user for approval in the Windows AI window; if denied, do not retry."
            }),
        ),
        "ping" => ok(&id, json!({})),
        "tools/list" => match forward(json!({"type": "run_tools_list", "run_token": token})) {
            Ok(r) if r["ok"] == true => ok(&id, json!({"tools": r["tools"].clone()})),
            Ok(r) => err(&id, -32000, r["error"].as_str().unwrap_or("Windows AI refused the request")),
            Err(e) => err(&id, -32000, &e.to_string()),
        },
        "tools/call" => {
            let req = json!({
                "type": "run_tool_call",
                "run_token": token,
                "name": msg["params"]["name"],
                "arguments": msg["params"].get("arguments").cloned().unwrap_or(json!({})),
            });
            match forward(req) {
                Ok(r) if r["ok"] == true => ok(&id, json!({"content": [{"type": "text", "text": r["content"].as_str().unwrap_or("")}], "isError": r["is_error"].as_bool().unwrap_or(false)})),
                Ok(r) => ok(&id, json!({"content": [{"type": "text", "text": r["error"].as_str().unwrap_or("refused")}], "isError": true})),
                Err(e) => ok(&id, json!({"content": [{"type": "text", "text": e.to_string()}], "isError": true})),
            }
        }
        _ => err(&id, -32601, "method not found"),
    })
}

pub fn run() {
    let token = std::env::var("WINDOWS_AI_RUN_TOKEN").unwrap_or_default();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else { continue };
        if let Some(resp) = handle(&msg, &token, &bridge::send_request) {
            if writeln!(stdout, "{resp}").is_err() || stdout.flush().is_err() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_calls_through_forwarder() {
        let fwd = |v: Value| -> AppResult<Value> {
            assert_eq!(v["run_token"], "tok");
            Ok(match v["type"].as_str().unwrap() {
                "run_tools_list" => json!({"ok": true, "tools": [{"name": "system_info", "description": "d", "inputSchema": {"type": "object"}}]}),
                _ => json!({"ok": true, "content": format!("ran {}", v["name"].as_str().unwrap()), "is_error": false}),
            })
        };
        let init = handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}), "tok", &fwd).unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "windows-ai");
        assert!(handle(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}), "tok", &fwd).is_none());
        let list = handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}), "tok", &fwd).unwrap();
        assert_eq!(list["result"]["tools"][0]["name"], "system_info");
        let call = handle(&json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "system_info", "arguments": {}}}), "tok", &fwd).unwrap();
        assert_eq!(call["result"]["content"][0]["text"], "ran system_info");
        assert_eq!(call["result"]["isError"], false);
    }

    #[test]
    fn refusal_becomes_tool_error() {
        let fwd = |_v: Value| -> AppResult<Value> { Ok(json!({"ok": false, "error": "unauthorized"})) };
        let call = handle(&json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "x"}}), "bad", &fwd).unwrap();
        assert_eq!(call["result"]["isError"], true);
        let list = handle(&json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"}), "bad", &fwd).unwrap();
        assert!(list.get("error").is_some());
    }
}
