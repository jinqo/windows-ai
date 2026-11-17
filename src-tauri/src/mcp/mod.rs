//! MCP server management. Servers are user-configured stdio processes.
//! Discovered tools are untrusted until the user trusts each one; only
//! trusted tools from enabled, connected servers are offered to the model,
//! and every call still goes through the permission executor.

pub mod client;

use crate::config::{McpServerConfig, PermissionMode};
use crate::error::{AppError, AppResult};
use crate::permissions::Access;
use crate::tools::{Tool, ToolContext, ToolMeta};
use client::{McpClient, McpToolInfo};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct McpServerStatus {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// disconnected | connecting | connected | error
    pub state: String,
    pub error: Option<String>,
    pub server_name: Option<String>,
    pub tools: Vec<McpToolStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpToolStatus {
    pub name: String,
    pub exposed_name: String,
    pub description: String,
    pub input_schema: Value,
    pub trusted: bool,
}

struct ServerState {
    cfg: McpServerConfig,
    state: String,
    error: Option<String>,
    tools: Vec<McpToolInfo>,
    client: Option<Arc<McpClient>>,
}

#[derive(Default)]
pub struct McpManager {
    servers: Mutex<HashMap<String, ServerState>>,
}

/// Name exposed to the model: `mcp__<server>__<tool>`, restricted to
/// `[A-Za-z0-9_-]` and 64 chars as required by tool-calling APIs.
pub fn exposed_name(server_id: &str, tool: &str) -> String {
    let clean: String = tool.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let mut s = format!("mcp__{server_id}__{clean}");
    s.truncate(64);
    s
}

impl McpManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconciles server entries with config (does not connect).
    pub fn sync_config(&self, servers: &[McpServerConfig]) -> Vec<String> {
        let mut map = self.servers.lock().unwrap();
        let mut removed = Vec::new();
        map.retain(|id, st| {
            let keep = servers.iter().any(|s| &s.id == id);
            if !keep {
                if let Some(c) = st.client.take() {
                    tokio::spawn(async move { c.shutdown().await });
                }
                removed.push(id.clone());
            }
            keep
        });
        for s in servers {
            map.entry(s.id.clone())
                .and_modify(|st| st.cfg = s.clone())
                .or_insert_with(|| ServerState { cfg: s.clone(), state: "disconnected".into(), error: None, tools: vec![], client: None });
        }
        removed
    }

    pub async fn connect(&self, id: &str) -> AppResult<McpServerStatus> {
        let cfg = {
            let mut map = self.servers.lock().unwrap();
            let st = map.get_mut(id).ok_or_else(|| AppError::NotFound(format!("MCP server {id}")))?;
            st.state = "connecting".into();
            st.error = None;
            st.cfg.clone()
        };
        self.disconnect_client(id).await;
        let result = async {
            let c = McpClient::connect(&cfg).await?;
            let tools = c.list_tools().await?;
            Ok::<_, AppError>((c, tools))
        }
        .await;
        {
            let mut map = self.servers.lock().unwrap();
            if let Some(st) = map.get_mut(id) {
                match result {
                    Ok((c, tools)) => {
                        st.client = Some(c);
                        st.tools = tools;
                        st.state = "connected".into();
                    }
                    Err(e) => {
                        st.state = "error".into();
                        st.error = Some(e.to_string());
                    }
                }
            }
        }
        self.status(id)
    }

    async fn disconnect_client(&self, id: &str) {
        let c = self.servers.lock().unwrap().get_mut(id).and_then(|s| s.client.take());
        if let Some(c) = c {
            c.shutdown().await;
        }
    }

    pub async fn disconnect(&self, id: &str) -> AppResult<McpServerStatus> {
        self.disconnect_client(id).await;
        if let Some(st) = self.servers.lock().unwrap().get_mut(id) {
            st.state = "disconnected".into();
            st.tools.clear();
        }
        self.status(id)
    }

    pub async fn connect_enabled(&self) {
        let ids: Vec<String> = self.servers.lock().unwrap().values().filter(|s| s.cfg.enabled).map(|s| s.cfg.id.clone()).collect();
        for id in ids {
            let _ = self.connect(&id).await;
        }
    }

    fn to_status(st: &ServerState) -> McpServerStatus {
        McpServerStatus {
            id: st.cfg.id.clone(),
            name: st.cfg.name.clone(),
            enabled: st.cfg.enabled,
            state: st.state.clone(),
            error: st.error.clone(),
            server_name: st.client.as_ref().and_then(|c| c.server_info["serverInfo"]["name"].as_str().map(String::from)),
            tools: st
                .tools
                .iter()
                .map(|t| McpToolStatus {
                    name: t.name.clone(),
                    exposed_name: exposed_name(&st.cfg.id, &t.name),
                    description: t.description.clone(),
                    input_schema: t.input_schema.clone(),
                    trusted: st.cfg.trusted_tools.contains(&t.name),
                })
                .collect(),
        }
    }

    pub fn status(&self, id: &str) -> AppResult<McpServerStatus> {
        let map = self.servers.lock().unwrap();
        map.get(id).map(Self::to_status).ok_or_else(|| AppError::NotFound(format!("MCP server {id}")))
    }

    pub fn statuses(&self) -> Vec<McpServerStatus> {
        let map = self.servers.lock().unwrap();
        let mut v: Vec<_> = map.values().map(Self::to_status).collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// Trusted tools from enabled+connected servers, wrapped as `Tool`s.
    pub fn ai_tools(&self) -> Vec<Arc<dyn Tool>> {
        let map = self.servers.lock().unwrap();
        let mut out: Vec<Arc<dyn Tool>> = Vec::new();
        for st in map.values() {
            let Some(client) = &st.client else { continue };
            if !st.cfg.enabled {
                continue;
            }
            for t in &st.tools {
                if st.cfg.trusted_tools.contains(&t.name) {
                    out.push(Arc::new(McpTool {
                        server_id: st.cfg.id.clone(),
                        server_name: st.cfg.name.clone(),
                        info: t.clone(),
                        client: client.clone(),
                    }));
                }
            }
        }
        out
    }

    pub async fn shutdown_all(&self) {
        let clients: Vec<_> = self.servers.lock().unwrap().values_mut().filter_map(|s| s.client.take()).collect();
        for c in clients {
            c.shutdown().await;
        }
    }
}

pub struct McpTool {
    server_id: String,
    server_name: String,
    info: McpToolInfo,
    client: Arc<McpClient>,
}

#[async_trait::async_trait]
impl Tool for McpTool {
    fn meta(&self) -> ToolMeta {
        let mut schema = self.info.input_schema.clone();
        if !schema.is_object() {
            schema = serde_json::json!({"type": "object"});
        }
        ToolMeta {
            name: exposed_name(&self.server_id, &self.info.name),
            description: format!("[MCP: {}] {}", self.server_name, self.info.description),
            input_schema: schema,
            access: Access::External,
            default_mode: PermissionMode::Ask,
            locked_ask: false,
            origin: format!("mcp:{}", self.server_id),
        }
    }

    fn describe(&self, input: &Value) -> String {
        let args = input.to_string();
        let args = if args.len() > 300 { format!("{}…", &args[..args.floor_char_boundary(300)]) } else { args };
        format!("Call MCP tool \"{}\" on server \"{}\" with {args}", self.info.name, self.server_name)
    }

    fn risk(&self, _input: &Value) -> &'static str {
        "medium"
    }

    async fn run(&self, _ctx: &ToolContext, input: Value) -> crate::error::AppResult<String> {
        let (text, is_error) = self.client.call_tool(&self.info.name, input, Duration::from_secs(120)).await?;
        if is_error {
            Err(AppError::Tool(text))
        } else {
            Ok(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as Map;

    #[test]
    fn exposed_names_are_api_safe() {
        assert_eq!(exposed_name("fs", "read file"), "mcp__fs__read_file");
        assert!(exposed_name("s", &"x".repeat(100)).len() <= 64);
    }

    /// End-to-end against a tiny fake MCP server written in Node (skipped if node is missing).
    #[tokio::test]
    async fn connects_discovers_and_calls_with_trust_gate() {
        if std::process::Command::new("node").arg("--version").output().is_err() {
            eprintln!("node not found; skipping");
            return;
        }
        let script = r#"
const rl = require('readline').createInterface({ input: process.stdin });
const send = (m) => process.stdout.write(JSON.stringify(m) + '\n');
rl.on('line', (l) => {
  const m = JSON.parse(l);
  if (m.method === 'initialize') send({ jsonrpc: '2.0', id: m.id, result: { protocolVersion: '2025-06-18', capabilities: { tools: {} }, serverInfo: { name: 'fake', version: '1' } } });
  else if (m.method === 'tools/list') send({ jsonrpc: '2.0', id: m.id, result: { tools: [{ name: 'echo', description: 'Echo text', inputSchema: { type: 'object', properties: { text: { type: 'string' } }, required: ['text'] } }, { name: 'danger', description: 'x', inputSchema: { type: 'object' } }] } });
  else if (m.method === 'tools/call') send({ jsonrpc: '2.0', id: m.id, result: { content: [{ type: 'text', text: 'echo:' + m.params.arguments.text }] } });
});"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.js");
        std::fs::write(&path, script).unwrap();
        let mut cfg = McpServerConfig {
            id: "fake".into(),
            name: "Fake".into(),
            command: "node".into(),
            args: vec![path.to_string_lossy().into()],
            env: Map::new(),
            cwd: None,
            enabled: true,
            trusted_tools: vec![],
        };
        let m = McpManager::new();
        m.sync_config(&[cfg.clone()]);
        let st = m.connect("fake").await.unwrap();
        assert_eq!(st.state, "connected", "{:?}", st.error);
        assert_eq!(st.tools.len(), 2);
        assert!(m.ai_tools().is_empty(), "untrusted tools must not be offered");

        cfg.trusted_tools = vec!["echo".into()];
        m.sync_config(&[cfg]);
        let tools = m.ai_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].meta().name, "mcp__fake__echo");
        let ctx = ToolContext {
            cfg: Default::default(),
            db: crate::db::Db::open_in_memory().unwrap(),
            notifier: Arc::new(crate::permissions::NullNotifier),
            cancel: Default::default(),
            conversation_id: None,
        };
        assert_eq!(tools[0].run(&ctx, serde_json::json!({"text": "hi"})).await.unwrap(), "echo:hi");
        m.shutdown_all().await;
    }
}
