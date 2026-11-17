//! Multi-step task engine. Plans are drafted (by the assistant or user),
//! reviewed, then run step-by-step through the permission executor.
//! Failed steps pause the task so the user can retry, skip or cancel.

pub mod store;

use crate::error::{AppError, AppResult};
use crate::tools::executor::ToolHub;
use crate::tools::ToolContext;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use store::*;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct TaskRunner {
    running: Mutex<HashMap<String, CancellationToken>>,
}

impl TaskRunner {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.running.lock().unwrap().contains_key(id)
    }

    pub fn cancel(&self, db: &crate::db::Db, id: &str) -> AppResult<()> {
        if let Some(t) = self.running.lock().unwrap().get(id) {
            t.cancel();
            return Ok(());
        }
        // Not running: cancel a reviewed/failed task directly.
        let task = get_task(db, id)?;
        if task.status != "completed" {
            set_task_status(db, id, "cancelled", None, None)?;
        }
        Ok(())
    }

    /// Marks the failed/pending step at `current_step` as skipped.
    pub fn skip_current(&self, db: &crate::db::Db, id: &str) -> AppResult<()> {
        if self.is_running(id) {
            return Err(AppError::InvalidInput("task is running".into()));
        }
        let task = get_task(db, id)?;
        if let Some(step) = task.steps.iter().find(|s| s.idx == task.current_step) {
            set_step(db, &step.id, "skipped", None, step.error.as_deref(), false)?;
        }
        set_task_status(db, id, "failed", Some(task.current_step + 1), Some("step skipped; resume to continue"))?;
        Ok(())
    }

    /// Runs (or resumes) a task from its current step to completion.
    /// `make_ctx` builds a ToolContext bound to the given cancel token.
    pub async fn run(&self, hub: &ToolHub, ctx: ToolContext, id: &str) -> AppResult<Task> {
        let task = get_task(&ctx.db, id)?;
        if matches!(task.status.as_str(), "completed" | "cancelled") {
            return Err(AppError::InvalidInput(format!("task is {}", task.status)));
        }
        {
            let mut r = self.running.lock().unwrap();
            if r.contains_key(id) {
                return Err(AppError::InvalidInput("task is already running".into()));
            }
            r.insert(id.to_string(), ctx.cancel.clone());
        }
        let result = self.run_inner(hub, &ctx, task).await;
        self.running.lock().unwrap().remove(id);
        ctx.notifier.emit("task-updated", json!({"task_id": id}));
        result
    }

    async fn run_inner(&self, hub: &ToolHub, ctx: &ToolContext, task: Task) -> AppResult<Task> {
        let db = &ctx.db;
        set_task_status(db, &task.id, "running", None, None)?;
        ctx.notifier.emit("task-updated", json!({"task_id": task.id}));
        for step in task.steps.iter().filter(|s| s.idx >= task.current_step) {
            if matches!(step.status.as_str(), "success" | "skipped") {
                continue;
            }
            if ctx.cancel.is_cancelled() {
                set_step(db, &step.id, "cancelled", None, None, false)?;
                set_task_status(db, &task.id, "cancelled", Some(step.idx), None)?;
                return get_task(db, &task.id);
            }
            set_task_status(db, &task.id, "running", Some(step.idx), None)?;
            set_step(db, &step.id, "running", None, None, true)?;
            ctx.notifier.emit("task-updated", json!({"task_id": task.id}));
            let out = hub.execute(ctx, &step.tool, step.input.clone(), "task", Some(&task.id)).await;
            match out.status.as_str() {
                "success" => set_step(db, &step.id, "success", Some(&out.content), None, false)?,
                "cancelled" => {
                    set_step(db, &step.id, "cancelled", None, Some(&out.content), false)?;
                    set_task_status(db, &task.id, "cancelled", Some(step.idx), None)?;
                    return get_task(db, &task.id);
                }
                status => {
                    let st = if status == "denied" { "denied" } else { "error" };
                    set_step(db, &step.id, st, None, Some(&out.content), false)?;
                    set_task_status(db, &task.id, "failed", Some(step.idx), Some(&format!("Step {} failed: {}", step.idx + 1, out.content)))?;
                    return get_task(db, &task.id);
                }
            }
            ctx.notifier.emit("task-updated", json!({"task_id": task.id}));
        }
        let n = task.steps.len() as i64;
        set_task_status(db, &task.id, "completed", Some(n), None)?;
        get_task(db, &task.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::db::Db;
    use crate::mcp::McpManager;
    use crate::permissions::{Approvals, NullNotifier};

    fn ctx(db: &Db) -> ToolContext {
        ToolContext { cfg: AppConfig::default(), db: db.clone(), notifier: Arc::new(NullNotifier), cancel: CancellationToken::new(), conversation_id: None }
    }

    #[tokio::test]
    async fn runs_fails_skips_and_resumes() {
        let db = Db::open_in_memory().unwrap();
        let hub = ToolHub::new(Arc::new(McpManager::new()), Approvals::new());
        let runner = TaskRunner::new();
        let task = create_task(
            &db,
            "demo",
            None,
            &[
                NewStep { description: "info".into(), tool: "system_info".into(), input: json!({}) },
                NewStep { description: "bad".into(), tool: "read_file".into(), input: json!({"path": "C:\\definitely\\not\\approved.txt"}) },
                NewStep { description: "info again".into(), tool: "system_info".into(), input: json!({}) },
            ],
        )
        .unwrap();
        assert_eq!(task.status, "awaiting_review");

        let t = runner.run(&hub, ctx(&db), &task.id).await.unwrap();
        assert_eq!(t.status, "failed");
        assert_eq!(t.current_step, 1);
        assert_eq!(t.steps[0].status, "success");
        assert_eq!(t.steps[1].status, "error");

        runner.skip_current(&db, &task.id).unwrap();
        let t = runner.run(&hub, ctx(&db), &task.id).await.unwrap();
        assert_eq!(t.status, "completed");
        assert_eq!(t.steps[1].status, "skipped");
        assert_eq!(t.steps[2].status, "success");
        assert!(runner.run(&hub, ctx(&db), &task.id).await.is_err(), "completed tasks cannot rerun");
    }

    #[tokio::test]
    async fn cancellation_stops_task() {
        let db = Db::open_in_memory().unwrap();
        let hub = ToolHub::new(Arc::new(McpManager::new()), Approvals::new());
        let runner = TaskRunner::new();
        let task = create_task(&db, "c", None, &[NewStep { description: "a".into(), tool: "system_info".into(), input: json!({}) }]).unwrap();
        let c = ctx(&db);
        c.cancel.cancel();
        let t = runner.run(&hub, c, &task.id).await.unwrap();
        assert_eq!(t.status, "cancelled");
    }
}
