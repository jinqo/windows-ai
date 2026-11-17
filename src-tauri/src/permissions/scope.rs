//! Directory scope enforcement with read/write separation.
//!
//! Paths are canonicalized (resolving `..` and symlinks) before comparison so
//! a request cannot escape an approved directory.

use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathAccess {
    Read,
    Write,
}

fn normalize(p: &Path) -> PathBuf {
    // dunce-style: strip the \\?\ prefix std::fs::canonicalize adds on Windows.
    let s = p.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    PathBuf::from(s.to_string())
}

fn lower(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// Canonicalizes `p`. For paths that do not exist yet (new files), the
/// nearest existing ancestor is canonicalized and the rest re-appended,
/// rejecting any `..` in the non-existent tail.
pub fn canonical(p: &Path) -> AppResult<PathBuf> {
    if !p.is_absolute() {
        return Err(AppError::InvalidInput(format!("path must be absolute: {}", p.display())));
    }
    if let Ok(c) = std::fs::canonicalize(p) {
        return Ok(normalize(&c));
    }
    let mut tail = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = std::fs::canonicalize(&cur) {
            let mut out = normalize(&c);
            for t in tail.iter().rev() {
                out.push(t);
            }
            return Ok(out);
        }
        let name = cur.file_name().map(|n| n.to_os_string());
        match (name, cur.parent()) {
            (Some(n), Some(parent)) => {
                tail.push(n);
                cur = parent.to_path_buf();
            }
            _ => return Err(AppError::InvalidInput(format!("cannot resolve path {}", p.display()))),
        }
    }
}

pub fn is_within(child: &Path, root: &Path) -> bool {
    let c = lower(child);
    let r = lower(root);
    c == r || c.starts_with(&format!("{r}\\"))
}

/// Verifies `path` is inside an approved directory with the needed access.
pub fn check_path(cfg: &AppConfig, path: &str, access: PathAccess) -> AppResult<PathBuf> {
    let raw = Path::new(path);
    if raw.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(AppError::PermissionDenied("'..' is not allowed in paths".into()));
    }
    let target = canonical(raw)?;
    for scope in &cfg.permissions.directories {
        let allowed = match access {
            PathAccess::Read => scope.read || scope.write,
            PathAccess::Write => scope.write,
        };
        if !allowed {
            continue;
        }
        let Ok(root) = canonical(Path::new(&scope.path)) else { continue };
        if is_within(&target, &root) {
            return Ok(target);
        }
    }
    let what = if access == PathAccess::Write { "write" } else { "read" };
    Err(AppError::PermissionDenied(format!(
        "{} is outside the directories approved for {what} access. Add it in Settings → Permissions.",
        target.display()
    )))
}

/// Approved roots with the requested access (canonicalized, existing only).
pub fn approved_roots(cfg: &AppConfig, access: PathAccess) -> Vec<PathBuf> {
    cfg.permissions
        .directories
        .iter()
        .filter(|s| match access {
            PathAccess::Read => s.read || s.write,
            PathAccess::Write => s.write,
        })
        .filter_map(|s| canonical(Path::new(&s.path)).ok())
        .filter(|p| p.is_dir())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DirectoryScope;

    fn cfg_with(dir: &Path, read: bool, write: bool) -> AppConfig {
        let mut c = AppConfig::default();
        c.permissions.directories.push(DirectoryScope { path: dir.to_string_lossy().into(), read, write });
        c
    }

    #[test]
    fn read_write_separation() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.txt"), "x").unwrap();
        let cfg = cfg_with(d.path(), true, false);
        let file = d.path().join("a.txt");
        assert!(check_path(&cfg, file.to_str().unwrap(), PathAccess::Read).is_ok());
        let e = check_path(&cfg, file.to_str().unwrap(), PathAccess::Write).unwrap_err();
        assert_eq!(e.kind(), "permission_denied");
    }

    #[test]
    fn write_scope_allows_new_files() {
        let d = tempfile::tempdir().unwrap();
        let cfg = cfg_with(d.path(), true, true);
        let new = d.path().join("sub").join("new.txt");
        assert!(check_path(&cfg, new.to_str().unwrap(), PathAccess::Write).is_ok());
    }

    #[test]
    fn blocks_escape_and_outside_paths() {
        let d = tempfile::tempdir().unwrap();
        let inner = d.path().join("inner");
        std::fs::create_dir(&inner).unwrap();
        let cfg = cfg_with(&inner, true, true);
        let escape = format!("{}\\..\\secret.txt", inner.display());
        assert!(check_path(&cfg, &escape, PathAccess::Read).is_err());
        let sibling = d.path().join("inner2").join("x.txt");
        assert!(check_path(&cfg, sibling.to_str().unwrap(), PathAccess::Read).is_err(), "prefix match must not leak to siblings");
        assert!(check_path(&cfg, "relative.txt", PathAccess::Read).is_err());
    }

    #[test]
    fn no_scopes_means_no_access() {
        let d = tempfile::tempdir().unwrap();
        let cfg = AppConfig::default();
        assert!(check_path(&cfg, d.path().to_str().unwrap(), PathAccess::Read).is_err());
    }
}
