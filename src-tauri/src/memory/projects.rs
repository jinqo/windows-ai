use crate::db::{new_id, now, Db};
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub root_path: Option<String>,
    /// Free-form context injected into the system prompt for this project.
    pub context: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub root_path: Option<String>,
    #[serde(default)]
    pub context: String,
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        root_path: r.get(3)?,
        context: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

const SELECT: &str = "SELECT id, name, description, root_path, context, created_at, updated_at FROM projects";

pub fn create_project(db: &Db, input: &ProjectInput) -> AppResult<Project> {
    if input.name.trim().is_empty() {
        return Err(AppError::InvalidInput("project name is required".into()));
    }
    let id = new_id();
    let ts = now();
    db.conn().execute(
        "INSERT INTO projects (id, name, description, root_path, context, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, input.name.trim(), input.description, input.root_path, input.context, ts],
    )?;
    get_project(db, &id)
}

pub fn get_project(db: &Db, id: &str) -> AppResult<Project> {
    db.conn()
        .query_row(&format!("{SELECT} WHERE id = ?1"), params![id], row)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("project {id}")))
}

pub fn list_projects(db: &Db) -> AppResult<Vec<Project>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY name"))?;
    let rows = stmt.query_map([], row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn update_project(db: &Db, id: &str, input: &ProjectInput) -> AppResult<Project> {
    let n = db.conn().execute(
        "UPDATE projects SET name=?2, description=?3, root_path=?4, context=?5, updated_at=?6 WHERE id=?1",
        params![id, input.name.trim(), input.description, input.root_path, input.context, now()],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("project {id}")));
    }
    get_project(db, id)
}

pub fn delete_project(db: &Db, id: &str) -> AppResult<()> {
    db.conn().execute("DELETE FROM projects WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_crud() {
        let db = Db::open_in_memory().unwrap();
        let p = create_project(&db, &ProjectInput { name: "Atlas".into(), description: String::new(), root_path: None, context: "Uses Rust".into() }).unwrap();
        assert_eq!(list_projects(&db).unwrap().len(), 1);
        let u = update_project(&db, &p.id, &ProjectInput { name: "Atlas 2".into(), description: "d".into(), root_path: None, context: String::new() }).unwrap();
        assert_eq!(u.name, "Atlas 2");
        delete_project(&db, &p.id).unwrap();
        assert!(list_projects(&db).unwrap().is_empty());
    }
}
