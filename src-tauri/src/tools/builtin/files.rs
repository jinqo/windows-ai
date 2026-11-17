use crate::config::PermissionMode;
use crate::error::{AppError, AppResult};
use crate::permissions::{approved_roots, check_path, Access, PathAccess};
use crate::tools::{str_arg, Tool, ToolContext, ToolMeta};
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;

fn meta(name: &str, description: &str, schema: Value, access: Access, mode: PermissionMode, locked: bool) -> ToolMeta {
    ToolMeta {
        name: name.into(),
        description: description.into(),
        input_schema: schema,
        access,
        default_mode: mode,
        locked_ask: locked,
        origin: "builtin".into(),
    }
}

pub struct SearchFiles;

#[async_trait::async_trait]
impl Tool for SearchFiles {
    fn meta(&self) -> ToolMeta {
        meta(
            "search_files",
            "Search file and folder names inside the user's approved directories. Returns matching absolute paths.",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "minLength": 1, "description": "Case-insensitive substring of the file name"},
                    "directory": {"type": "string", "description": "Optional absolute path of an approved directory to limit the search"},
                    "extensions": {"type": "array", "items": {"type": "string"}, "description": "Optional extensions like [\"md\", \"txt\"]"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 200}
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            Access::Read,
            PermissionMode::Allow,
            false,
        )
    }

    fn describe(&self, input: &Value) -> String {
        format!("Search approved folders for files named like \"{}\"", str_arg(input, "query"))
    }

    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String> {
        let query = str_arg(&input, "query").to_lowercase();
        let max = input.get("max_results").and_then(Value::as_u64).unwrap_or(50) as usize;
        let exts: Vec<String> = input
            .get("extensions")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(|s| s.trim_start_matches('.').to_lowercase()).collect())
            .unwrap_or_default();
        let roots = match input.get("directory").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            Some(d) => vec![check_path(&ctx.cfg, d, PathAccess::Read)?],
            None => approved_roots(&ctx.cfg, PathAccess::Read),
        };
        if roots.is_empty() {
            return Err(AppError::PermissionDenied("No directories are approved for reading. Add one in Settings → Permissions.".into()));
        }
        let cancel = ctx.cancel.clone();
        let results = tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            let mut scanned = 0usize;
            for root in roots {
                for entry in walkdir::WalkDir::new(&root).max_depth(10).follow_links(false).into_iter().filter_map(Result::ok) {
                    scanned += 1;
                    if scanned > 200_000 || out.len() >= max || cancel.is_cancelled() {
                        break;
                    }
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if !name.contains(&query) {
                        continue;
                    }
                    if !exts.is_empty() {
                        let ext = entry.path().extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                        if !exts.contains(&ext) {
                            continue;
                        }
                    }
                    let kind = if entry.file_type().is_dir() { "dir " } else { "file" };
                    out.push(format!("{kind} {}", entry.path().display()));
                }
            }
            out
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        if results.is_empty() {
            Ok("No matching files found.".into())
        } else {
            Ok(results.join("\n"))
        }
    }
}

pub struct ListDirectory;

#[async_trait::async_trait]
impl Tool for ListDirectory {
    fn meta(&self) -> ToolMeta {
        meta(
            "list_directory",
            "List the entries of a directory inside an approved folder (names, type, size).",
            json!({
                "type": "object",
                "properties": {"path": {"type": "string", "minLength": 1}},
                "required": ["path"],
                "additionalProperties": false
            }),
            Access::Read,
            PermissionMode::Allow,
            false,
        )
    }

    fn describe(&self, input: &Value) -> String {
        format!("List folder {}", str_arg(input, "path"))
    }

    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String> {
        let p = check_path(&ctx.cfg, str_arg(&input, "path"), PathAccess::Read)?;
        let mut lines = Vec::new();
        for e in std::fs::read_dir(&p)?.filter_map(Result::ok).take(500) {
            let md = e.metadata().ok();
            let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = md.map(|m| m.len()).unwrap_or(0);
            lines.push(if is_dir { format!("[dir]  {}", e.file_name().to_string_lossy()) } else { format!("[file] {} ({size} bytes)", e.file_name().to_string_lossy()) });
        }
        lines.sort();
        Ok(if lines.is_empty() { "(empty directory)".into() } else { lines.join("\n") })
    }
}

pub struct ReadFile;

/// Reads a text file with a byte cap. Rejects binary content.
pub fn read_text_file(path: &Path, max_bytes: u64) -> AppResult<(String, bool)> {
    let md = std::fs::metadata(path)?;
    if md.is_dir() {
        return Err(AppError::InvalidInput(format!("{} is a directory", path.display())));
    }
    let mut f = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    f.by_ref().take(max_bytes).read_to_end(&mut buf)?;
    if buf.iter().take(8192).any(|&b| b == 0) {
        return Err(AppError::InvalidInput(format!("{} looks like a binary file; only text files can be read", path.display())));
    }
    let truncated = md.len() > max_bytes;
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

#[async_trait::async_trait]
impl Tool for ReadFile {
    fn meta(&self) -> ToolMeta {
        meta(
            "read_file",
            "Read a UTF-8 text file inside an approved folder. Large files are truncated.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "minLength": 1, "description": "Absolute path"},
                    "max_bytes": {"type": "integer", "minimum": 1}
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            Access::Read,
            PermissionMode::Allow,
            false,
        )
    }

    fn describe(&self, input: &Value) -> String {
        format!("Read file {}", str_arg(input, "path"))
    }

    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String> {
        let p = check_path(&ctx.cfg, str_arg(&input, "path"), PathAccess::Read)?;
        let cap = input.get("max_bytes").and_then(Value::as_u64).unwrap_or(u64::MAX).min(ctx.cfg.permissions.max_read_bytes);
        let (text, truncated) = read_text_file(&p, cap)?;
        Ok(if truncated { format!("{text}\n\n[truncated at {cap} bytes]") } else { text })
    }
}

pub struct WriteFile;

#[async_trait::async_trait]
impl Tool for WriteFile {
    fn meta(&self) -> ToolMeta {
        meta(
            "write_file",
            "Create a text file inside a folder approved for writing. Fails if the file exists unless overwrite is true (overwriting always needs explicit user confirmation).",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "minLength": 1, "description": "Absolute path of the file to create"},
                    "content": {"type": "string"},
                    "overwrite": {"type": "boolean"}
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
            Access::Write,
            PermissionMode::Ask,
            true,
        )
    }

    fn describe(&self, input: &Value) -> String {
        let n = str_arg(input, "content").chars().count();
        let path = str_arg(input, "path");
        if input.get("overwrite").and_then(Value::as_bool).unwrap_or(false) && Path::new(path).exists() {
            format!("OVERWRITE existing file {path} with {n} characters (previous contents will be lost)")
        } else {
            format!("Create file {path} ({n} characters)")
        }
    }

    fn risk(&self, input: &Value) -> &'static str {
        if input.get("overwrite").and_then(Value::as_bool).unwrap_or(false) { "high" } else { "medium" }
    }

    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String> {
        let p = check_path(&ctx.cfg, str_arg(&input, "path"), PathAccess::Write)?;
        let overwrite = input.get("overwrite").and_then(Value::as_bool).unwrap_or(false);
        if p.exists() && !overwrite {
            return Err(AppError::Tool(format!("{} already exists (set overwrite to replace it)", p.display())));
        }
        if p.is_dir() {
            return Err(AppError::Tool(format!("{} is a directory", p.display())));
        }
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = str_arg(&input, "content");
        std::fs::write(&p, content)?;
        Ok(format!("Wrote {} bytes to {}", content.len(), p.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, DirectoryScope};
    use crate::db::Db;
    use crate::permissions::NullNotifier;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    fn ctx(dir: &Path, write: bool) -> ToolContext {
        let mut cfg = AppConfig::default();
        cfg.permissions.directories.push(DirectoryScope { path: dir.to_string_lossy().into(), read: true, write });
        ToolContext { cfg, db: Db::open_in_memory().unwrap(), notifier: Arc::new(NullNotifier), cancel: CancellationToken::new(), conversation_id: None }
    }

    #[tokio::test]
    async fn search_read_write() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("notes")).unwrap();
        std::fs::write(d.path().join("notes").join("Meeting.md"), "agenda").unwrap();
        let c = ctx(d.path(), true);
        let r = SearchFiles.run(&c, json!({"query": "meeting", "extensions": ["md"]})).await.unwrap();
        assert!(r.contains("Meeting.md"));
        let path = d.path().join("notes").join("Meeting.md");
        assert_eq!(ReadFile.run(&c, json!({"path": path.to_str().unwrap()})).await.unwrap(), "agenda");
        let newp = d.path().join("out.txt");
        WriteFile.run(&c, json!({"path": newp.to_str().unwrap(), "content": "hi"})).await.unwrap();
        assert!(WriteFile.run(&c, json!({"path": newp.to_str().unwrap(), "content": "x"})).await.is_err());
        WriteFile.run(&c, json!({"path": newp.to_str().unwrap(), "content": "x", "overwrite": true})).await.unwrap();
        assert_eq!(std::fs::read_to_string(&newp).unwrap(), "x");
        let listing = ListDirectory.run(&c, json!({"path": d.path().to_str().unwrap()})).await.unwrap();
        assert!(listing.contains("[dir]  notes"));
    }

    #[tokio::test]
    async fn read_only_scope_blocks_write_and_binary() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("b.bin"), [0u8, 1, 2]).unwrap();
        let c = ctx(d.path(), false);
        let p = d.path().join("x.txt");
        assert_eq!(WriteFile.run(&c, json!({"path": p.to_str().unwrap(), "content": "a"})).await.unwrap_err().kind(), "permission_denied");
        let b = d.path().join("b.bin");
        assert!(ReadFile.run(&c, json!({"path": b.to_str().unwrap()})).await.is_err());
    }
}
