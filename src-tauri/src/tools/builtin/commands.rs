//! Allow-listed command execution. The model can only pick a command by
//! name from the user's list; program and arguments are fixed in config.
//! There is no shell and no free-form command line.

use crate::config::{AllowedCommand, PermissionMode};
use crate::error::{AppError, AppResult};
use crate::permissions::Access;
use crate::tools::{str_arg, Tool, ToolContext, ToolMeta};
use serde_json::{json, Value};
use std::time::Duration;

pub struct RunCommand;

fn find<'a>(cmds: &'a [AllowedCommand], name: &str) -> AppResult<&'a AllowedCommand> {
    cmds.iter().find(|c| c.name == name).ok_or_else(|| {
        let names: Vec<_> = cmds.iter().map(|c| c.name.as_str()).collect();
        AppError::PermissionDenied(format!(
            "\"{name}\" is not an allow-listed command. Allowed: {}",
            if names.is_empty() { "(none configured)".into() } else { names.join(", ") }
        ))
    })
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut cut = max;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}\n[output truncated]", &s[..cut])
    }
}

#[async_trait::async_trait]
impl Tool for RunCommand {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "run_command".into(),
            description: "Run one of the user's pre-approved commands by name. Program and arguments are fixed by the user; no other commands can run. Always requires confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {"name": {"type": "string", "minLength": 1}},
                "required": ["name"],
                "additionalProperties": false
            }),
            access: Access::Execute,
            default_mode: PermissionMode::Ask,
            locked_ask: true,
            origin: "builtin".into(),
        }
    }

    fn describe(&self, input: &Value) -> String {
        format!("Run allow-listed command \"{}\"", str_arg(input, "name"))
    }

    fn risk(&self, _input: &Value) -> &'static str {
        "high"
    }

    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String> {
        let c = find(&ctx.cfg.permissions.commands, str_arg(&input, "name"))?.clone();
        let mut cmd = tokio::process::Command::new(&c.program);
        cmd.args(&c.args).kill_on_drop(true).stdin(std::process::Stdio::null());
        if let Some(cwd) = &c.cwd {
            cmd.current_dir(cwd);
        }
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        let child = cmd.output();
        let out = tokio::select! {
            r = tokio::time::timeout(Duration::from_secs(c.timeout_secs.max(1)), child) => match r {
                Ok(r) => r.map_err(|e| AppError::Tool(format!("failed to start {}: {e}", c.program)))?,
                Err(_) => return Err(AppError::Tool(format!("command timed out after {}s", c.timeout_secs))),
            },
            _ = ctx.cancel.cancelled() => return Err(AppError::Cancelled),
        };
        let stdout = truncate(&String::from_utf8_lossy(&out.stdout), 20_000);
        let stderr = truncate(&String::from_utf8_lossy(&out.stderr), 8_000);
        let code = out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "?".into());
        let text = format!("exit code: {code}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}");
        if out.status.success() {
            Ok(text)
        } else {
            Err(AppError::Tool(text))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::db::Db;
    use crate::permissions::NullNotifier;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    fn ctx(cmds: Vec<AllowedCommand>) -> ToolContext {
        let mut cfg = AppConfig::default();
        cfg.permissions.commands = cmds;
        ToolContext { cfg, db: Db::open_in_memory().unwrap(), notifier: Arc::new(NullNotifier), cancel: CancellationToken::new(), conversation_id: None }
    }

    #[tokio::test]
    async fn only_allowlisted_commands_run() {
        let c = ctx(vec![AllowedCommand {
            name: "echo".into(),
            description: "".into(),
            program: "cmd".into(),
            args: vec!["/C".into(), "echo".into(), "hello".into()],
            cwd: None,
            timeout_secs: 10,
        }]);
        let out = RunCommand.run(&c, json!({"name": "echo"})).await.unwrap();
        assert!(out.contains("hello"));
        let e = RunCommand.run(&c, json!({"name": "del /s C:\\"})).await.unwrap_err();
        assert_eq!(e.kind(), "permission_denied");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let s = "é".repeat(10);
        assert!(truncate(&s, 5).starts_with("éé"));
    }
}
