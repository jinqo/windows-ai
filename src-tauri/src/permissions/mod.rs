//! Permission framework: per-tool modes, directory scopes and interactive approvals.

pub mod approvals;
pub mod scope;

pub use approvals::*;
pub use scope::*;

use crate::config::{AppConfig, PermissionMode};
use serde::Serialize;

/// What kind of effect a tool has. Drives defaults and UI labelling.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Read,
    Write,
    Execute,
    Network,
    System,
    Memory,
    External,
}

/// Effective mode for a tool, combining its default with user overrides.
/// `locked_ask` tools can be denied but never silently allowed.
pub fn effective_mode(cfg: &AppConfig, tool: &str, default: PermissionMode, locked_ask: bool) -> PermissionMode {
    let m = cfg.permissions.tool_modes.get(tool).copied().unwrap_or(default);
    if locked_ask && m == PermissionMode::Allow {
        PermissionMode::Ask
    } else {
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_tools_cannot_be_allowed() {
        let mut cfg = AppConfig::default();
        cfg.permissions.tool_modes.insert("run_command".into(), PermissionMode::Allow);
        cfg.permissions.tool_modes.insert("read_file".into(), PermissionMode::Deny);
        assert_eq!(effective_mode(&cfg, "run_command", PermissionMode::Ask, true), PermissionMode::Ask);
        assert_eq!(effective_mode(&cfg, "read_file", PermissionMode::Allow, false), PermissionMode::Deny);
        assert_eq!(effective_mode(&cfg, "system_info", PermissionMode::Allow, false), PermissionMode::Allow);
    }
}
