use crate::db::{new_id, now, Db};
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct TaskStep {
    pub id: String,
    pub idx: i64,
    pub description: String,
    pub tool: String,
    pub input: Value,
    /// pending | running | success | error | skipped | denied | cancelled
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub attempts: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    /// awaiting_review | running | failed | completed | cancelled
    pub status: String,
    pub conversation_id: Option<String>,
    pub current_step: i64,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub steps: Vec<TaskStep>,
}

pub struct NewStep {
    pub description: String,
    pub tool: String,
    pub input: Value,
}

pub fn create_task(db: &Db, title: &str, conversation_id: Option<&str>, steps: &[NewStep]) -> AppResult<Task> {
    let id = new_id();
    let ts = now();
    {
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO tasks (id, title, status, conversation_id, current_step, created_at, updated_at) VALUES (?1, ?2, 'awaiting_review', ?3, 0, ?4, ?4)",
            params![id, title.trim(), conversation_id, ts],
        )?;
        for (i, s) in steps.iter().enumerate() {
            tx.execute(
                "INSERT INTO task_steps (id, task_id, idx, description, tool, input_json, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending')",
                params![new_id(), id, i as i64, s.description, s.tool, s.input.to_string()],
            )?;
        }
        tx.commit()?;
    }
    get_task(db, &id)
}

pub fn get_task(db: &Db, id: &str) -> AppResult<Task> {
    let conn = db.conn();
    let mut task = conn
        .query_row(
            "SELECT id, title, status, conversation_id, current_step, error, created_at, updated_at FROM tasks WHERE id = ?1",
            params![id],
            |r| {
                Ok(Task {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    status: r.get(2)?,
                    conversation_id: r.get(3)?,
                    current_step: r.get(4)?,
                    error: r.get(5)?,
                    created_at: r.get(6)?,
                    updated_at: r.get(7)?,
                    steps: vec![],
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("task {id}")))?;
    let mut stmt = conn.prepare(
        "SELECT id, idx, description, tool, input_json, status, result, error, attempts FROM task_steps WHERE task_id = ?1 ORDER BY idx",
    )?;
    task.steps = stmt
        .query_map(params![id], |r| {
            let input: String = r.get(4)?;
            Ok(TaskStep {
                id: r.get(0)?,
                idx: r.get(1)?,
                description: r.get(2)?,
                tool: r.get(3)?,
                input: serde_json::from_str(&input).unwrap_or(Value::Null),
                status: r.get(5)?,
                result: r.get(6)?,
                error: r.get(7)?,
                attempts: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(task)
}

pub fn list_tasks(db: &Db, limit: u32) -> AppResult<Vec<Task>> {
    let ids: Vec<String> = {
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT id FROM tasks ORDER BY created_at DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit], |r| r.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    ids.iter().map(|id| get_task(db, id)).collect()
}

pub fn set_task_status(db: &Db, id: &str, status: &str, current_step: Option<i64>, error: Option<&str>) -> AppResult<()> {
    db.conn().execute(
        "UPDATE tasks SET status = ?2, current_step = COALESCE(?3, current_step), error = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, status, current_step, error, now()],
    )?;
    Ok(())
}

pub fn set_step(db: &Db, step_id: &str, status: &str, result: Option<&str>, error: Option<&str>, bump_attempt: bool) -> AppResult<()> {
    db.conn().execute(
        "UPDATE task_steps SET status = ?2, result = ?3, error = ?4, attempts = attempts + ?5 WHERE id = ?1",
        params![step_id, status, result, error, bump_attempt as i64],
    )?;
    Ok(())
}

pub fn delete_task(db: &Db, id: &str) -> AppResult<()> {
    db.conn().execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
    Ok(())
}
