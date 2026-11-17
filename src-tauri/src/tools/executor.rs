//! The single gateway for running any tool (built-in or MCP):
//! lookup → input validation → permission mode → approval → run → log.

use super::{schema, Tool, ToolContext, ToolMeta};
use crate::activity::{self, NewActivity};
use crate::config::PermissionMode;
use crate::error::AppError;
use crate::mcp::McpManager;
use crate::permissions::{effective_mode, ApprovalRequest, Approvals, Decision};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
pub struct ToolOutcome {
    pub content: String,
    pub is_error: bool,
    pub activity_id: Option<String>,
    /// success | error | denied | cancelled
    pub status: String,
}

impl ToolOutcome {
    fn err(msg: impl Into<String>, status: &str, activity_id: Option<String>) -> Self {
        Self { content: msg.into(), is_error: true, activity_id, status: status.into() }
    }
}

pub struct ToolHub {
    builtin: HashMap<String, Arc<dyn Tool>>,
    pub mcp: Arc<McpManager>,
    pub approvals: Arc<Approvals>,
}

impl ToolHub {
    pub fn new(mcp: Arc<McpManager>, approvals: Arc<Approvals>) -> Self {
        let builtin = super::builtin_tools().into_iter().map(|t| (t.meta().name, t)).collect();
        Self { builtin, mcp, approvals }
    }

    pub fn all_tools(&self) -> Vec<Arc<dyn Tool>> {
        let mut v: Vec<Arc<dyn Tool>> = self.builtin.values().cloned().collect();
        v.extend(self.mcp.ai_tools());
        v.sort_by_key(|t| t.meta().name);
        v
    }

    pub fn find(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.builtin.get(name).cloned().or_else(|| self.mcp.ai_tools().into_iter().find(|t| t.meta().name == name))
    }

    /// Tools offered to the model: everything not set to Deny.
    pub fn offered(&self, cfg: &crate::config::AppConfig) -> Vec<ToolMeta> {
        self.all_tools()
            .into_iter()
            .map(|t| t.meta())
            .filter(|m| effective_mode(cfg, &m.name, m.default_mode, m.locked_ask) != PermissionMode::Deny)
            .filter(|m| !(m.name == "remember" && !cfg.memory.allow_ai_save))
            .collect()
    }

    pub async fn execute(&self, ctx: &ToolContext, name: &str, input: Value, source: &str, task_id: Option<&str>) -> ToolOutcome {
        let Some(tool) = self.find(name) else {
            return ToolOutcome::err(format!("Unknown or unavailable tool '{name}'"), "error", None);
        };
        let meta = tool.meta();
        let description = tool.describe(&input);
        let log = |status: &str| {
            activity::start(
                &ctx.db,
                NewActivity {
                    source,
                    tool: name,
                    description: &description,
                    input: &input,
                    status,
                    conversation_id: ctx.conversation_id.as_deref(),
                    task_id,
                },
            )
            .ok()
        };
        let emit = |id: &Option<String>| {
            if let Some(id) = id {
                ctx.notifier.emit("activity", json!({"id": id}));
            }
        };

        if let Some(raw) = input.get("__invalid_json") {
            let id = log("error");
            if let Some(i) = &id {
                let _ = activity::finish(&ctx.db, i, "error", None, Some("invalid JSON input"), 0);
            }
            emit(&id);
            return ToolOutcome::err(format!("Tool input was not valid JSON: {raw}. Retry with valid JSON."), "error", id);
        }
        if let Err(e) = schema::validate(&meta.input_schema, &input) {
            let id = log("error");
            if let Some(i) = &id {
                let _ = activity::finish(&ctx.db, i, "error", None, Some(&e), 0);
            }
            emit(&id);
            return ToolOutcome::err(format!("Invalid input: {e}"), "error", id);
        }

        let mode = effective_mode(&ctx.cfg, name, meta.default_mode, meta.locked_ask);
        if mode == PermissionMode::Deny {
            let id = log("denied");
            emit(&id);
            return ToolOutcome::err(format!("The user's settings deny the '{name}' tool."), "denied", id);
        }

        let needs_approval = mode == PermissionMode::Ask && (meta.locked_ask || !self.approvals.is_session_allowed(name));
        let id = log(if needs_approval { "pending_approval" } else { "running" });
        emit(&id);
        let finish = |status: &str, result: Option<&str>, error: Option<&str>, ms: i64| {
            if let Some(i) = &id {
                let _ = activity::finish(&ctx.db, i, status, result, error, ms);
            }
            emit(&id);
        };

        if needs_approval {
            let req = ApprovalRequest {
                id: id.clone().unwrap_or_else(crate::db::new_id),
                tool: name.into(),
                title: format!("{} wants to use {}", if source == "task" { "A task" } else { "Windows AI" }, name),
                description: description.clone(),
                input: input.clone(),
                access: serde_json::to_value(meta.access).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
                risk: tool.risk(&input).into(),
                source: source.into(),
                allow_session: !meta.locked_ask,
            };
            let timeout = Duration::from_secs(ctx.cfg.permissions.approval_timeout_secs.max(10));
            match self.approvals.request(ctx.notifier.as_ref(), req, timeout, &ctx.cancel).await {
                Ok(Decision::Deny) => {
                    finish("denied", None, Some("denied by user"), 0);
                    return ToolOutcome::err("The user denied this action.", "denied", id);
                }
                Ok(_) => {
                    if let Some(i) = &id {
                        let _ = activity::set_status(&ctx.db, i, "running");
                    }
                    emit(&id);
                }
                Err(AppError::Cancelled) => {
                    finish("cancelled", None, Some("cancelled"), 0);
                    return ToolOutcome::err("Cancelled by the user.", "cancelled", id);
                }
                Err(e) => {
                    finish("error", None, Some(&e.to_string()), 0);
                    return ToolOutcome::err(e.to_string(), "error", id);
                }
            }
        }

        let started = Instant::now();
        let result = tokio::select! {
            r = tool.run(ctx, input.clone()) => r,
            _ = ctx.cancel.cancelled() => Err(AppError::Cancelled),
        };
        let ms = started.elapsed().as_millis() as i64;
        match result {
            Ok(out) => {
                finish("success", Some(&out), None, ms);
                ToolOutcome { content: out, is_error: false, activity_id: id, status: "success".into() }
            }
            Err(AppError::Cancelled) => {
                finish("cancelled", None, Some("cancelled"), ms);
                ToolOutcome::err("Cancelled by the user.", "cancelled", id)
            }
            Err(e) => {
                let msg = e.to_string();
                finish("error", None, Some(&msg), ms);
                ToolOutcome::err(msg, "error", id)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, DirectoryScope};
    use crate::db::Db;
    use crate::permissions::NullNotifier;
    use tokio_util::sync::CancellationToken;

    fn hub() -> ToolHub {
        ToolHub::new(Arc::new(McpManager::new()), Approvals::new())
    }

    fn ctx(cfg: AppConfig) -> ToolContext {
        ToolContext { cfg, db: Db::open_in_memory().unwrap(), notifier: Arc::new(NullNotifier), cancel: CancellationToken::new(), conversation_id: None }
    }

    #[tokio::test]
    async fn allowed_tool_runs_and_is_logged() {
        let h = hub();
        let c = ctx(AppConfig::default());
        let out = h.execute(&c, "system_info", json!({}), "chat", None).await;
        assert!(!out.is_error, "{}", out.content);
        let log = activity::list(&c.db, None, 10).unwrap();
        assert_eq!(log[0].status, "success");
    }

    #[tokio::test]
    async fn invalid_input_and_unknown_tools_fail_safely() {
        let h = hub();
        let c = ctx(AppConfig::default());
        assert!(h.execute(&c, "nope", json!({}), "chat", None).await.is_error);
        let o = h.execute(&c, "read_file", json!({"wrong": 1}), "chat", None).await;
        assert!(o.content.contains("Invalid input"));
        let o = h.execute(&c, "read_file", json!({"__invalid_json": "{"}), "chat", None).await;
        assert!(o.content.contains("not valid JSON"));
    }

    #[tokio::test]
    async fn deny_mode_blocks() {
        let h = hub();
        let mut cfg = AppConfig::default();
        cfg.permissions.tool_modes.insert("system_info".into(), PermissionMode::Deny);
        let c = ctx(cfg.clone());
        let o = h.execute(&c, "system_info", json!({}), "chat", None).await;
        assert_eq!(o.status, "denied");
        assert!(!h.offered(&cfg).iter().any(|m| m.name == "system_info"));
    }

    #[tokio::test]
    async fn ask_mode_waits_for_approval_then_runs() {
        let h = Arc::new(hub());
        let d = tempfile::tempdir().unwrap();
        let mut cfg = AppConfig::default();
        cfg.permissions.directories.push(DirectoryScope { path: d.path().to_string_lossy().into(), read: true, write: true });
        let c = Arc::new(ctx(cfg));
        let p = d.path().join("new.txt");
        let (h2, c2, p2) = (h.clone(), c.clone(), p.clone());
        let task = tokio::spawn(async move { h2.execute(&c2, "write_file", json!({"path": p2.to_str().unwrap(), "content": "hi"}), "chat", None).await });
        // wait for the approval to be registered
        let mut pending = vec![];
        for _ in 0..50 {
            pending = h.approvals.pending();
            if !pending.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(pending.len(), 1);
        assert!(!pending[0].allow_session, "write_file is locked to ask");
        h.approvals.resolve(&pending[0].id, Decision::Approve).unwrap();
        let out = task.await.unwrap();
        assert!(!out.is_error, "{}", out.content);
        assert!(p.exists());
    }

    #[tokio::test]
    async fn denial_is_reported_and_nothing_runs() {
        let h = Arc::new(hub());
        let d = tempfile::tempdir().unwrap();
        let mut cfg = AppConfig::default();
        cfg.permissions.directories.push(DirectoryScope { path: d.path().to_string_lossy().into(), read: true, write: true });
        let c = Arc::new(ctx(cfg));
        let p = d.path().join("never.txt");
        let (h2, c2, p2) = (h.clone(), c.clone(), p.clone());
        let task = tokio::spawn(async move { h2.execute(&c2, "write_file", json!({"path": p2.to_str().unwrap(), "content": "x"}), "chat", None).await });
        loop {
            if let Some(r) = h.approvals.pending().first() {
                h.approvals.resolve(&r.id, Decision::Deny).unwrap();
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let out = task.await.unwrap();
        assert_eq!(out.status, "denied");
        assert!(!p.exists());
    }
}
