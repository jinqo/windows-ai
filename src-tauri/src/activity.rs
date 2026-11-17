//! Execution history: every tool call (approved, denied, failed) is logged.

use crate::db::{new_id, now, Db};
use crate::error::AppResult;
use rusqlite::params;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct ActivityEntry {
    pub id: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub source: String,
    pub tool: String,
    pub description: String,
    pub input: Value,
    /// pending_approval | running | success | error | denied | cancelled
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
    pub conversation_id: Option<String>,
    pub task_id: Option<String>,
}

pub struct NewActivity<'a> {
    pub source: &'a str,
    pub tool: &'a str,
    pub description: &'a str,
    pub input: &'a Value,
    pub status: &'a str,
    pub conversation_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
}

/// Redacts long or sensitive-looking input fields before persisting.
fn redact(input: &Value) -> Value {
    match input {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| {
                    let kl = k.to_lowercase();
                    if kl.contains("password") || kl.contains("token") || kl.contains("secret") || kl.contains("api_key") {
                        (k.clone(), Value::String("[redacted]".into()))
                    } else if let Value::String(s) = v {
                        if s.chars().count() > 2000 {
                            (k.clone(), Value::String(format!("{}… [{} chars]", s.chars().take(2000).collect::<String>(), s.chars().count())))
                        } else {
                            (k.clone(), v.clone())
                        }
                    } else {
                        (k.clone(), redact(v))
                    }
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

pub fn start(db: &Db, a: NewActivity) -> AppResult<String> {
    let id = new_id();
    db.conn().execute(
        "INSERT INTO activity (id, created_at, source, tool, description, input_json, status, conversation_id, task_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, now(), a.source, a.tool, a.description, redact(a.input).to_string(), a.status, a.conversation_id, a.task_id],
    )?;
    Ok(id)
}

pub fn set_status(db: &Db, id: &str, status: &str) -> AppResult<()> {
    db.conn().execute("UPDATE activity SET status = ?2 WHERE id = ?1", params![id, status])?;
    Ok(())
}

pub fn finish(db: &Db, id: &str, status: &str, result: Option<&str>, error: Option<&str>, duration_ms: i64) -> AppResult<()> {
    let result = result.map(|r| if r.len() > 4000 { format!("{}…", &r[..r.floor_char_boundary(4000)]) } else { r.to_string() });
    db.conn().execute(
        "UPDATE activity SET status=?2, result=?3, error=?4, duration_ms=?5, finished_at=?6 WHERE id=?1",
        params![id, status, result, error, duration_ms, now()],
    )?;
    Ok(())
}

pub fn get(db: &Db, id: &str) -> AppResult<ActivityEntry> {
    let conn = db.conn();
    Ok(conn.query_row(&format!("{SELECT} WHERE id = ?1"), params![id], row)?)
}

const SELECT: &str = "SELECT id, created_at, finished_at, source, tool, description, input_json, status, result, error, duration_ms, conversation_id, task_id FROM activity";

fn row(r: &rusqlite::Row) -> rusqlite::Result<ActivityEntry> {
    let input: String = r.get(6)?;
    Ok(ActivityEntry {
        id: r.get(0)?,
        created_at: r.get(1)?,
        finished_at: r.get(2)?,
        source: r.get(3)?,
        tool: r.get(4)?,
        description: r.get(5)?,
        input: serde_json::from_str(&input).unwrap_or(Value::Null),
        status: r.get(7)?,
        result: r.get(8)?,
        error: r.get(9)?,
        duration_ms: r.get(10)?,
        conversation_id: r.get(11)?,
        task_id: r.get(12)?,
    })
}

pub fn list(db: &Db, status: Option<&str>, limit: u32) -> AppResult<Vec<ActivityEntry>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(&format!("{SELECT} WHERE (?1 IS NULL OR status = ?1) ORDER BY created_at DESC LIMIT ?2"))?;
    let rows = stmt.query_map(params![status, limit], row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn clear(db: &Db) -> AppResult<()> {
    db.conn().execute("DELETE FROM activity", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_and_redacts() {
        let db = Db::open_in_memory().unwrap();
        let input = serde_json::json!({"path": "C:/x", "api_key": "sk-123"});
        let id = start(&db, NewActivity { source: "chat", tool: "read_file", description: "Read C:/x", input: &input, status: "running", conversation_id: None, task_id: None }).unwrap();
        finish(&db, &id, "success", Some("ok"), None, 5).unwrap();
        let e = get(&db, &id).unwrap();
        assert_eq!(e.status, "success");
        assert_eq!(e.input["api_key"], "[redacted]");
        assert_eq!(list(&db, Some("success"), 10).unwrap().len(), 1);
        assert!(list(&db, Some("error"), 10).unwrap().is_empty());
    }
}
