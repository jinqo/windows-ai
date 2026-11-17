use crate::ai::types::{ChatMessage, ContentBlock, Role};
use crate::db::{new_id, now, Db};
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub model: Option<String>,
    pub created_at: String,
}

fn row_to_conv(r: &rusqlite::Row) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: r.get(0)?,
        title: r.get(1)?,
        project_id: r.get(2)?,
        pinned: r.get::<_, i64>(3)? != 0,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
        message_count: r.get(6)?,
    })
}

const CONV_SELECT: &str = "SELECT c.id, c.title, c.project_id, c.pinned, c.created_at, c.updated_at,
    (SELECT count(*) FROM messages m WHERE m.conversation_id = c.id) FROM conversations c";

pub fn create_conversation(db: &Db, title: &str, project_id: Option<&str>) -> AppResult<Conversation> {
    let id = new_id();
    let ts = now();
    let title = if title.trim().is_empty() { "New conversation" } else { title.trim() };
    db.conn().execute(
        "INSERT INTO conversations (id, title, project_id, pinned, created_at, updated_at) VALUES (?1, ?2, ?3, 0, ?4, ?4)",
        params![id, title, project_id, ts],
    )?;
    get_conversation(db, &id)
}

pub fn get_conversation(db: &Db, id: &str) -> AppResult<Conversation> {
    db.conn()
        .query_row(&format!("{CONV_SELECT} WHERE c.id = ?1"), params![id], row_to_conv)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("conversation {id}")))
}

pub fn list_conversations(db: &Db, search: Option<&str>, limit: u32) -> AppResult<Vec<Conversation>> {
    let conn = db.conn();
    let mut out = Vec::new();
    match search.map(str::trim).filter(|s| !s.is_empty()) {
        Some(q) => {
            let like = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));
            let mut stmt = conn.prepare(&format!(
                "{CONV_SELECT} WHERE c.title LIKE ?1 ESCAPE '\\' OR EXISTS (SELECT 1 FROM messages m WHERE m.conversation_id = c.id AND m.content_json LIKE ?1 ESCAPE '\\')
                 ORDER BY c.pinned DESC, c.updated_at DESC LIMIT ?2"
            ))?;
            for r in stmt.query_map(params![like, limit], row_to_conv)? {
                out.push(r?);
            }
        }
        None => {
            let mut stmt = conn.prepare(&format!("{CONV_SELECT} ORDER BY c.pinned DESC, c.updated_at DESC LIMIT ?1"))?;
            for r in stmt.query_map(params![limit], row_to_conv)? {
                out.push(r?);
            }
        }
    }
    Ok(out)
}

pub fn rename_conversation(db: &Db, id: &str, title: &str) -> AppResult<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::InvalidInput("title cannot be empty".into()));
    }
    let n = db.conn().execute("UPDATE conversations SET title = ?2, updated_at = ?3 WHERE id = ?1", params![id, title, now()])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("conversation {id}")));
    }
    Ok(())
}

pub fn set_conversation_pinned(db: &Db, id: &str, pinned: bool) -> AppResult<()> {
    db.conn().execute("UPDATE conversations SET pinned = ?2 WHERE id = ?1", params![id, pinned as i64])?;
    Ok(())
}

pub fn set_conversation_project(db: &Db, id: &str, project_id: Option<&str>) -> AppResult<()> {
    db.conn().execute("UPDATE conversations SET project_id = ?2 WHERE id = ?1", params![id, project_id])?;
    Ok(())
}

pub fn get_cli_session(db: &Db, id: &str) -> AppResult<Option<String>> {
    Ok(db
        .conn()
        .query_row("SELECT cli_session_id FROM conversations WHERE id = ?1", params![id], |r| r.get::<_, Option<String>>(0))
        .optional()?
        .flatten())
}

pub fn set_cli_session(db: &Db, id: &str, session: Option<&str>) -> AppResult<()> {
    db.conn().execute("UPDATE conversations SET cli_session_id = ?2 WHERE id = ?1", params![id, session])?;
    Ok(())
}

pub fn delete_conversation(db: &Db, id: &str) -> AppResult<()> {
    db.conn().execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn append_message(db: &Db, conversation_id: &str, msg: &ChatMessage, model: Option<&str>) -> AppResult<StoredMessage> {
    let id = new_id();
    let ts = now();
    let json = serde_json::to_string(&msg.content)?;
    let conn = db.conn();
    let seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE conversation_id = ?1",
        params![conversation_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content_json, model, created_at, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, conversation_id, msg.role.as_str(), json, model, ts, seq],
    )?;
    conn.execute("UPDATE conversations SET updated_at = ?2 WHERE id = ?1", params![conversation_id, ts])?;
    Ok(StoredMessage {
        id,
        conversation_id: conversation_id.into(),
        role: msg.role,
        content: msg.content.clone(),
        model: model.map(String::from),
        created_at: ts,
    })
}

pub fn list_messages(db: &Db, conversation_id: &str) -> AppResult<Vec<StoredMessage>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content_json, model, created_at FROM messages WHERE conversation_id = ?1 ORDER BY seq",
    )?;
    let rows = stmt.query_map(params![conversation_id], |r| {
        let json: String = r.get(3)?;
        let role: String = r.get(2)?;
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, role, json, r.get::<_, Option<String>>(4)?, r.get::<_, String>(5)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, cid, role, json, model, created_at) = row?;
        let content: Vec<ContentBlock> = serde_json::from_str(&json).unwrap_or_default();
        out.push(StoredMessage { id, conversation_id: cid, role: Role::parse(&role), content, model, created_at });
    }
    Ok(out)
}

/// Builds the history sent to the provider: the last `window` messages,
/// trimmed so it never starts with an orphaned tool_result.
pub fn history_for_model(db: &Db, conversation_id: &str, window: usize) -> AppResult<Vec<ChatMessage>> {
    let all = list_messages(db, conversation_id)?;
    let start = all.len().saturating_sub(window.max(2));
    let mut msgs: Vec<ChatMessage> = all[start..]
        .iter()
        .map(|m| ChatMessage { role: m.role, content: m.content.clone() })
        .collect();
    // Drop leading messages until we start at a plain user message.
    while let Some(first) = msgs.first() {
        let is_plain_user = first.role == Role::User
            && first.content.iter().all(|b| !matches!(b, ContentBlock::ToolResult { .. }));
        if is_plain_user {
            break;
        }
        msgs.remove(0);
    }
    Ok(msgs)
}

pub fn delete_messages_after(db: &Db, conversation_id: &str, message_id: &str) -> AppResult<()> {
    let conn = db.conn();
    let seq: i64 = conn.query_row(
        "SELECT seq FROM messages WHERE id = ?1 AND conversation_id = ?2",
        params![message_id, conversation_id],
        |r| r.get(0),
    )?;
    conn.execute("DELETE FROM messages WHERE conversation_id = ?1 AND seq >= ?2", params![conversation_id, seq])?;
    // History was edited: a resumed CLI session would no longer match it.
    conn.execute("UPDATE conversations SET cli_session_id = NULL WHERE id = ?1", params![conversation_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(t: &str) -> ChatMessage {
        ChatMessage { role: Role::User, content: vec![ContentBlock::Text { text: t.into() }] }
    }

    #[test]
    fn conversation_lifecycle() {
        let db = Db::open_in_memory().unwrap();
        let c = create_conversation(&db, "", None).unwrap();
        assert_eq!(c.title, "New conversation");
        append_message(&db, &c.id, &user("hello world"), None).unwrap();
        append_message(
            &db,
            &c.id,
            &ChatMessage { role: Role::Assistant, content: vec![ContentBlock::Text { text: "hi".into() }] },
            Some("m"),
        )
        .unwrap();
        let msgs = list_messages(&db, &c.id).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].model.as_deref(), Some("m"));
        rename_conversation(&db, &c.id, "Greeting").unwrap();
        let found = list_conversations(&db, Some("hello"), 10).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Greeting");
        assert_eq!(found[0].message_count, 2);
        delete_conversation(&db, &c.id).unwrap();
        assert!(list_messages(&db, &c.id).unwrap().is_empty());
    }

    #[test]
    fn history_never_starts_with_tool_result() {
        let db = Db::open_in_memory().unwrap();
        let c = create_conversation(&db, "t", None).unwrap();
        append_message(&db, &c.id, &user("a"), None).unwrap();
        append_message(
            &db,
            &c.id,
            &ChatMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse { id: "t1".into(), name: "x".into(), input: serde_json::json!({}) }],
            },
            None,
        )
        .unwrap();
        append_message(
            &db,
            &c.id,
            &ChatMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult { tool_use_id: "t1".into(), content: "ok".into(), is_error: false }],
            },
            None,
        )
        .unwrap();
        append_message(&db, &c.id, &user("b"), None).unwrap();
        let h = history_for_model(&db, &c.id, 2).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h[0], user("b"));
        let h = history_for_model(&db, &c.id, 50).unwrap();
        assert_eq!(h.len(), 4);
    }
}
