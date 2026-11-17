//! Interactive approval broker. A tool call that needs consent emits an
//! `approval-request` event and waits for the user's decision, a timeout,
//! or cancellation of the run.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

/// Abstraction over event emission so the broker is testable without Tauri.
pub trait Notifier: Send + Sync {
    fn emit(&self, event: &str, payload: Value);
}

pub struct NullNotifier;
impl Notifier for NullNotifier {
    fn emit(&self, _event: &str, _payload: Value) {}
}

#[derive(Debug, Clone, Serialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub tool: String,
    pub title: String,
    /// Plain-language description of exactly what will happen.
    pub description: String,
    pub input: Value,
    pub access: String,
    /// "low" | "medium" | "high"
    pub risk: String,
    pub source: String,
    /// Whether "allow for this session" may be offered.
    pub allow_session: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approve,
    ApproveSession,
    Deny,
}

#[derive(Default)]
pub struct Approvals {
    pending: Mutex<HashMap<String, (ApprovalRequest, oneshot::Sender<Decision>)>>,
    session_allowed: Mutex<HashSet<String>>,
}

impl Approvals {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn is_session_allowed(&self, tool: &str) -> bool {
        self.session_allowed.lock().unwrap().contains(tool)
    }

    pub fn clear_session(&self) {
        self.session_allowed.lock().unwrap().clear();
    }

    pub fn pending(&self) -> Vec<ApprovalRequest> {
        self.pending.lock().unwrap().values().map(|(r, _)| r.clone()).collect()
    }

    pub async fn request(
        &self,
        notifier: &dyn Notifier,
        req: ApprovalRequest,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> AppResult<Decision> {
        let (tx, rx) = oneshot::channel();
        let id = req.id.clone();
        let tool = req.tool.clone();
        let allow_session = req.allow_session;
        self.pending.lock().unwrap().insert(id.clone(), (req.clone(), tx));
        notifier.emit("approval-request", serde_json::to_value(&req).unwrap_or(Value::Null));
        let result = tokio::select! {
            r = rx => r.map_err(|_| AppError::Internal("approval channel closed".into())),
            _ = tokio::time::sleep(timeout) => Ok(Decision::Deny),
            _ = cancel.cancelled() => Err(AppError::Cancelled),
        };
        self.pending.lock().unwrap().remove(&id);
        notifier.emit("approval-resolved", serde_json::json!({ "id": id }));
        let d = result?;
        if d == Decision::ApproveSession && allow_session {
            self.session_allowed.lock().unwrap().insert(tool);
        }
        Ok(if d == Decision::ApproveSession && !allow_session { Decision::Approve } else { d })
    }

    pub fn resolve(&self, id: &str, decision: Decision) -> AppResult<()> {
        let entry = self.pending.lock().unwrap().remove(id);
        match entry {
            Some((_, tx)) => {
                let _ = tx.send(decision);
                Ok(())
            }
            None => Err(AppError::NotFound(format!("approval {id} (it may have timed out)"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: &str, allow_session: bool) -> ApprovalRequest {
        ApprovalRequest {
            id: id.into(),
            tool: "write_file".into(),
            title: "t".into(),
            description: "d".into(),
            input: Value::Null,
            access: "write".into(),
            risk: "high".into(),
            source: "chat".into(),
            allow_session,
        }
    }

    #[tokio::test]
    async fn approve_flow() {
        let a = Approvals::new();
        let a2 = a.clone();
        let h = tokio::spawn(async move { a2.request(&NullNotifier, req("1", true), Duration::from_secs(5), &CancellationToken::new()).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(a.pending().len(), 1);
        a.resolve("1", Decision::ApproveSession).unwrap();
        assert_eq!(h.await.unwrap().unwrap(), Decision::ApproveSession);
        assert!(a.is_session_allowed("write_file"));
    }

    #[tokio::test]
    async fn session_approval_downgraded_when_not_allowed() {
        let a = Approvals::new();
        let a2 = a.clone();
        let h = tokio::spawn(async move { a2.request(&NullNotifier, req("2", false), Duration::from_secs(5), &CancellationToken::new()).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        a.resolve("2", Decision::ApproveSession).unwrap();
        assert_eq!(h.await.unwrap().unwrap(), Decision::Approve);
        assert!(!a.is_session_allowed("write_file"));
    }

    #[tokio::test]
    async fn timeout_denies_and_cancel_errors() {
        let a = Approvals::new();
        let d = a.request(&NullNotifier, req("3", true), Duration::from_millis(30), &CancellationToken::new()).await.unwrap();
        assert_eq!(d, Decision::Deny);
        let c = CancellationToken::new();
        c.cancel();
        let e = a.request(&NullNotifier, req("4", true), Duration::from_secs(5), &c).await.unwrap_err();
        assert_eq!(e.kind(), "cancelled");
        assert!(a.pending().is_empty());
    }
}
