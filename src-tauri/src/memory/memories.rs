use crate::db::{new_id, now, Db};
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const MEMORY_KINDS: &[&str] = &["preference", "fact", "project", "note"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Memory {
    pub id: String,
    pub kind: String,
    pub content: String,
    pub tags: String,
    pub project_id: Option<String>,
    pub source: String,
    pub pinned: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryInput {
    pub kind: String,
    pub content: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub pinned: bool,
}

fn row_to_memory(r: &rusqlite::Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        kind: r.get(1)?,
        content: r.get(2)?,
        tags: r.get(3)?,
        project_id: r.get(4)?,
        source: r.get(5)?,
        pinned: r.get::<_, i64>(6)? != 0,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

const SELECT: &str = "SELECT m.id, m.kind, m.content, m.tags, m.project_id, m.source, m.pinned, m.created_at, m.updated_at FROM memories m";

fn validate(input: &MemoryInput) -> AppResult<()> {
    if !MEMORY_KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::InvalidInput(format!("memory kind must be one of {MEMORY_KINDS:?}")));
    }
    let c = input.content.trim();
    if c.is_empty() {
        return Err(AppError::InvalidInput("memory content is empty".into()));
    }
    if c.len() > 4000 {
        return Err(AppError::InvalidInput("memory content exceeds 4000 characters".into()));
    }
    Ok(())
}

/// Creates a memory. `allow_sensitive` must be true to store content that
/// looks like a secret; the UI only sets it after an explicit user confirm.
pub fn create_memory(db: &Db, input: &MemoryInput, source: &str, allow_sensitive: bool) -> AppResult<Memory> {
    validate(input)?;
    let found = super::sensitive::detect(&input.content);
    if !found.is_empty() && !allow_sensitive {
        return Err(AppError::PermissionDenied(format!(
            "content appears to contain sensitive data ({}); not saved",
            found.join(", ")
        )));
    }
    let id = new_id();
    let ts = now();
    db.conn().execute(
        "INSERT INTO memories (id, kind, content, tags, project_id, source, pinned, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![id, input.kind, input.content.trim(), input.tags.trim(), input.project_id, source, input.pinned as i64, ts],
    )?;
    get_memory(db, &id)
}

pub fn get_memory(db: &Db, id: &str) -> AppResult<Memory> {
    db.conn()
        .query_row(&format!("{SELECT} WHERE m.id = ?1"), params![id], row_to_memory)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("memory {id}")))
}

pub fn update_memory(db: &Db, id: &str, input: &MemoryInput, allow_sensitive: bool) -> AppResult<Memory> {
    validate(input)?;
    let found = super::sensitive::detect(&input.content);
    if !found.is_empty() && !allow_sensitive {
        return Err(AppError::PermissionDenied(format!("content appears to contain sensitive data ({})", found.join(", "))));
    }
    let n = db.conn().execute(
        "UPDATE memories SET kind=?2, content=?3, tags=?4, project_id=?5, pinned=?6, updated_at=?7 WHERE id=?1",
        params![id, input.kind, input.content.trim(), input.tags.trim(), input.project_id, input.pinned as i64, now()],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("memory {id}")));
    }
    get_memory(db, id)
}

pub fn delete_memory(db: &Db, id: &str) -> AppResult<()> {
    db.conn().execute("DELETE FROM memories WHERE id = ?1", params![id])?;
    Ok(())
}

/// Turns free text into a safe FTS5 query: quoted terms OR-ed together.
pub fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 3)
        .take(16)
        .map(|t| format!("\"{}\"", t.to_lowercase()))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

pub fn search_memories(
    db: &Db,
    query: Option<&str>,
    kind: Option<&str>,
    project_id: Option<&str>,
    limit: u32,
) -> AppResult<Vec<Memory>> {
    let conn = db.conn();
    let mut out = Vec::new();
    let kind = kind.filter(|k| !k.is_empty());
    match query.and_then(fts_query) {
        Some(q) => {
            let mut stmt = conn.prepare(&format!(
                "{SELECT} JOIN memories_fts f ON f.rowid = m.rowid
                 WHERE memories_fts MATCH ?1 AND (?2 IS NULL OR m.kind = ?2) AND (?3 IS NULL OR m.project_id = ?3 OR m.project_id IS NULL)
                 ORDER BY m.pinned DESC, bm25(memories_fts) LIMIT ?4"
            ))?;
            for r in stmt.query_map(params![q, kind, project_id, limit], row_to_memory)? {
                out.push(r?);
            }
        }
        None => {
            let mut stmt = conn.prepare(&format!(
                "{SELECT} WHERE (?1 IS NULL OR m.kind = ?1) AND (?2 IS NULL OR m.project_id = ?2 OR m.project_id IS NULL)
                 ORDER BY m.pinned DESC, m.updated_at DESC LIMIT ?3"
            ))?;
            for r in stmt.query_map(params![kind, project_id, limit], row_to_memory)? {
                out.push(r?);
            }
        }
    }
    Ok(out)
}

/// Memories to inject as context for a prompt: pinned + preferences first,
/// then the best full-text matches, de-duplicated.
pub fn context_for(db: &Db, prompt: &str, project_id: Option<&str>, limit: usize) -> AppResult<Vec<Memory>> {
    let mut out: Vec<Memory> = Vec::new();
    let conn_pinned = {
        let conn = db.conn();
        let mut stmt = conn.prepare(&format!(
            "{SELECT} WHERE (m.pinned = 1 OR m.kind = 'preference') AND (m.project_id IS NULL OR m.project_id = ?1)
             ORDER BY m.pinned DESC, m.updated_at DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![project_id, limit as i64], row_to_memory)?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    out.extend(conn_pinned);
    if out.len() < limit {
        for m in search_memories(db, Some(prompt), None, project_id, limit as u32)? {
            if out.len() >= limit {
                break;
            }
            if !out.iter().any(|x| x.id == m.id) {
                out.push(m);
            }
        }
    }
    out.truncate(limit);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: &str, content: &str) -> MemoryInput {
        MemoryInput { kind: kind.into(), content: content.into(), tags: String::new(), project_id: None, pinned: false }
    }

    #[test]
    fn crud_and_search() {
        let db = Db::open_in_memory().unwrap();
        let a = create_memory(&db, &input("fact", "The build server is called atlas"), "user", false).unwrap();
        create_memory(&db, &input("preference", "Prefers concise answers"), "user", false).unwrap();
        let hits = search_memories(&db, Some("which build server?"), None, None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, a.id);

        let mut upd = input("fact", "The build server is called hermes");
        upd.pinned = true;
        let u = update_memory(&db, &a.id, &upd, false).unwrap();
        assert!(u.pinned);
        assert!(search_memories(&db, Some("atlas"), None, None, 10).unwrap().is_empty());
        assert_eq!(search_memories(&db, Some("hermes"), None, None, 10).unwrap().len(), 1);

        let ctx = context_for(&db, "unrelated question", None, 5).unwrap();
        assert_eq!(ctx.len(), 2, "pinned + preference always included");

        delete_memory(&db, &a.id).unwrap();
        assert!(get_memory(&db, &a.id).is_err());
    }

    #[test]
    fn refuses_sensitive_without_confirmation() {
        let db = Db::open_in_memory().unwrap();
        let err = create_memory(&db, &input("note", "password: hunter22"), "assistant", false).unwrap_err();
        assert_eq!(err.kind(), "permission_denied");
        assert!(create_memory(&db, &input("note", "password: hunter22"), "user", true).is_ok());
    }

    #[test]
    fn rejects_invalid_kind() {
        let db = Db::open_in_memory().unwrap();
        assert!(create_memory(&db, &input("weird", "x"), "user", false).is_err());
    }

    #[test]
    fn fts_query_is_sanitized() {
        assert_eq!(fts_query("a\" OR 1=1 --"), None);
        assert_eq!(fts_query("Rust NEAR(tauri)").unwrap(), "\"rust\" OR \"near\" OR \"tauri\"");
    }
}
