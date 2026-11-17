//! Claude subscription mode: the official, unmodified Claude Code CLI runs
//! as a local child process (`claude -p --output-format stream-json`).
//!
//! Compliance boundaries (enforced here):
//! - Authentication is done only by the CLI's own `claude auth login`.
//!   Windows AI reads `claude auth status --json` for the signed-in state and
//!   billing method, and never reads, copies or forwards tokens or cookies.
//! - All ANTHROPIC_* / CLAUDE* environment variables are removed from the
//!   child so an API key in the environment can never silently switch the
//!   CLI to paid API billing. If the CLI itself is signed in with Console
//!   (API) billing, runs are refused unless the user explicitly opts in.
//! - Claude Code's built-in tools are disabled (`--tools ""`). Instead,
//!   Windows AI's own tools are offered over MCP and every call goes through
//!   Windows AI's permission executor (approval dialogs, scopes, logging).

use super::agent::{build_system_prompt, RunParams};
use super::types::*;
use crate::config::ClaudeCodeConfig;
use crate::error::{AppError, AppResult};
use crate::memory;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

pub const MCP_SERVER_NAME: &str = "windows_ai";

/// How to launch the CLI. `prefix` supports `node cli.js` installs and tests.
#[derive(Debug, Clone, PartialEq)]
pub struct Cli {
    pub program: PathBuf,
    pub prefix: Vec<String>,
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

/// Finds the Claude Code CLI: configured path, PATH, then the default
/// locations of the native installer and npm.
pub fn locate(configured: &str) -> Option<Cli> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        return p.is_file().then(|| cli_for(p));
    }
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let candidates = [
        which("claude.exe"),
        home.as_ref().map(|h| h.join(".local").join("bin").join("claude.exe")),
        which("claude.cmd"),
        appdata.as_ref().map(|a| a.join("npm").join("claude.cmd")),
    ];
    candidates.into_iter().flatten().find(|p| p.is_file()).map(cli_for)
}

/// npm installs ship a `claude.cmd` shim; run its JS entry with node so
/// arguments never pass through cmd.exe parsing.
fn cli_for(p: PathBuf) -> Cli {
    let is_cmd = p.extension().map(|e| e.eq_ignore_ascii_case("cmd")).unwrap_or(false);
    if is_cmd {
        if let Some(dir) = p.parent() {
            let js = dir.join("node_modules").join("@anthropic-ai").join("claude-code").join("cli.js");
            if js.is_file() {
                if let Some(node) = which("node.exe").or_else(|| Some(dir.join("node.exe")).filter(|n| n.is_file())) {
                    return Cli { program: node, prefix: vec![js.to_string_lossy().into()] };
                }
            }
        }
    }
    Cli { program: p, prefix: vec![] }
}

/// Environment variables removed from the child process.
pub fn is_stripped_env(key: &str) -> bool {
    let k = key.to_ascii_uppercase();
    if k == "CLAUDE_CONFIG_DIR" {
        return false; // where the user's own CLI login lives
    }
    k.starts_with("ANTHROPIC_") || k.starts_with("CLAUDE_") || k == "CLAUDECODE" || k.starts_with("AWS_BEARER_TOKEN_BEDROCK")
}

impl Cli {
    pub fn command(&self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.program);
        cmd.args(&self.prefix);
        for (k, _) in std::env::vars_os() {
            if let Some(ks) = k.to_str() {
                if is_stripped_env(ks) {
                    cmd.env_remove(&k);
                }
            }
        }
        cmd.kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        cmd
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CliStatus {
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub logged_in: bool,
    pub auth_method: Option<String>,
    pub subscription_type: Option<String>,
    /// "subscription" | "api" | "unknown"
    pub billing: String,
    pub login_expires_at: Option<String>,
    pub error: Option<String>,
}

/// Interprets `claude auth status --json`. Identity fields (email, org)
/// are deliberately ignored and never stored.
pub fn parse_auth_status(v: &Value) -> (bool, Option<String>, Option<String>, String, Option<String>) {
    let logged_in = v["loggedIn"].as_bool().unwrap_or(false);
    let method = v["authMethod"].as_str().map(String::from);
    let sub = v["subscriptionType"].as_str().or(v["planTier"].as_str()).map(String::from);
    let provider = v["apiProvider"].as_str().unwrap_or("firstParty");
    let billing = match (logged_in, method.as_deref(), provider) {
        (false, _, _) => "unknown",
        (true, _, p) if p != "firstParty" => "api",
        (true, Some("claude.ai"), _) => "subscription",
        (true, Some(_), _) => "api",
        _ => "unknown",
    };
    (logged_in, method, sub, billing.into(), v["loginExpiresAt"].as_str().map(String::from))
}

async fn run_simple(cli: &Cli, args: &[&str], timeout: Duration) -> AppResult<(bool, String, String)> {
    let mut cmd = cli.command();
    cmd.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let out = tokio::time::timeout(timeout, cmd.output())
        .await
        .map_err(|_| AppError::Provider("Claude Code did not respond in time".into()))?
        .map_err(|e| AppError::Provider(format!("could not start Claude Code: {e}")))?;
    Ok((out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned()))
}

pub async fn status(cfg: &ClaudeCodeConfig) -> CliStatus {
    let Some(cli) = locate(&cfg.cli_path) else {
        return CliStatus {
            billing: "unknown".into(),
            error: Some(if cfg.cli_path.trim().is_empty() {
                "Claude Code is not installed. Install it, then sign in: see Settings → AI Providers.".into()
            } else {
                format!("Claude Code not found at {}", cfg.cli_path)
            }),
            ..Default::default()
        };
    };
    let mut st = CliStatus { installed: true, path: Some(cli.program.to_string_lossy().into()), billing: "unknown".into(), ..Default::default() };
    if let Ok((true, out, _)) = run_simple(&cli, &["--version"], Duration::from_secs(20)).await {
        st.version = Some(out.trim().to_string());
    }
    match run_simple(&cli, &["auth", "status", "--json"], Duration::from_secs(20)).await {
        Ok((_, out, err)) => match serde_json::from_str::<Value>(out.trim()) {
            Ok(v) => {
                let (li, m, s, b, exp) = parse_auth_status(&v);
                st.logged_in = li;
                st.auth_method = m;
                st.subscription_type = s;
                st.billing = b;
                st.login_expires_at = exp;
                if !li {
                    st.error = Some("Claude Code is not signed in.".into());
                }
            }
            Err(_) => st.error = Some(format!("Could not read sign-in status: {}", err.trim().chars().take(200).collect::<String>())),
        },
        Err(e) => st.error = Some(e.to_string()),
    }
    st
}

/// Opens the CLI's own sign-in flow in a visible console window.
/// The user completes it in their browser; Windows AI never sees credentials.
pub fn launch_login(cfg: &ClaudeCodeConfig) -> AppResult<()> {
    let cli = locate(&cfg.cli_path).ok_or_else(|| AppError::NotFound("Claude Code is not installed".into()))?;
    let mut cmd = std::process::Command::new(&cli.program);
    cmd.args(&cli.prefix).args(["auth", "login", "--claudeai"]);
    for (k, _) in std::env::vars_os() {
        if k.to_str().map(is_stripped_env).unwrap_or(false) {
            cmd.env_remove(&k);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0010); // CREATE_NEW_CONSOLE
    }
    cmd.spawn().map_err(|e| AppError::Provider(format!("could not start Claude Code sign-in: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------- stream parsing

#[derive(Debug, Clone, PartialEq)]
pub enum CliEvent {
    Init { session_id: String, model: String },
    TextDelta(String),
    ThinkingDelta(String),
    ToolStart { id: String, name: String },
    AssistantText(String),
    ToolResult { id: String, is_error: bool, content: String },
    RateLimit { status: String, kind: String, resets_at: Option<i64>, utilization: Option<f64> },
    Result { is_error: bool, subtype: String, text: String, session_id: String, terminal_reason: String },
}

fn tool_result_text(c: &Value) -> String {
    match c {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(|x| x["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// Parses one stream-json line. Subagent (nested) events are ignored.
pub fn parse_line(line: &str) -> Vec<CliEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return vec![] };
    if !v["parent_tool_use_id"].is_null() {
        return vec![];
    }
    let mut out = Vec::new();
    match v["type"].as_str() {
        Some("system") if v["subtype"] == "init" => out.push(CliEvent::Init {
            session_id: v["session_id"].as_str().unwrap_or_default().into(),
            model: v["model"].as_str().unwrap_or_default().into(),
        }),
        Some("stream_event") => {
            let e = &v["event"];
            match e["type"].as_str() {
                Some("content_block_delta") => match e["delta"]["type"].as_str() {
                    Some("text_delta") => out.push(CliEvent::TextDelta(e["delta"]["text"].as_str().unwrap_or_default().into())),
                    Some("thinking_delta") => out.push(CliEvent::ThinkingDelta(e["delta"]["thinking"].as_str().unwrap_or_default().into())),
                    _ => {}
                },
                Some("content_block_start") if e["content_block"]["type"] == "tool_use" => out.push(CliEvent::ToolStart {
                    id: e["content_block"]["id"].as_str().unwrap_or_default().into(),
                    name: e["content_block"]["name"].as_str().unwrap_or_default().into(),
                }),
                _ => {}
            }
        }
        Some("assistant") => {
            let text: Vec<&str> = v["message"]["content"].as_array().map(|a| a.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect()).unwrap_or_default();
            if !text.is_empty() {
                out.push(CliEvent::AssistantText(text.join("")));
            }
        }
        Some("user") => {
            for b in v["message"]["content"].as_array().cloned().unwrap_or_default() {
                if b["type"] == "tool_result" {
                    out.push(CliEvent::ToolResult {
                        id: b["tool_use_id"].as_str().unwrap_or_default().into(),
                        is_error: b["is_error"].as_bool().unwrap_or(false),
                        content: tool_result_text(&b["content"]),
                    });
                }
            }
        }
        Some("rate_limit_event") => {
            let r = &v["rate_limit_info"];
            out.push(CliEvent::RateLimit {
                status: r["status"].as_str().unwrap_or_default().into(),
                kind: r["rateLimitType"].as_str().unwrap_or_default().into(),
                resets_at: r["resetsAt"].as_i64(),
                utilization: r["utilization"].as_f64(),
            });
        }
        Some("result") => {
            let text = v["result"].as_str().map(String::from).unwrap_or_else(|| v["errors"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")).unwrap_or_default());
            out.push(CliEvent::Result {
                is_error: v["is_error"].as_bool().unwrap_or(false) || v["subtype"].as_str().map(|s| s != "success").unwrap_or(false),
                subtype: v["subtype"].as_str().unwrap_or_default().into(),
                text,
                session_id: v["session_id"].as_str().unwrap_or_default().into(),
                terminal_reason: v["terminal_reason"].as_str().unwrap_or_default().into(),
            });
        }
        _ => {}
    }
    out
}

/// Maps CLI failure output to a structured, user-actionable error.
pub fn classify_error(text: &str) -> AppError {
    let t = text.to_lowercase();
    let short: String = text.trim().chars().take(400).collect();
    if t.contains("usage limit") || t.contains("hit your limit") || t.contains("limit reached") || t.contains("rate_limit") || t.contains("rate limit") {
        AppError::Provider(format!("Claude usage limit reached for your subscription. {short}"))
    } else if t.contains("/login") || t.contains("not logged in") || t.contains("invalid api key") || t.contains("oauth token") || t.contains("authentication") || t.contains("expired") || t.contains("401") {
        AppError::MissingCredential(format!("Claude Code needs you to sign in again (Settings → AI Providers → Sign in). {short}"))
    } else if t.contains("no conversation found") || t.contains("session not found") {
        AppError::NotFound(format!("claude session: {short}"))
    } else {
        AppError::Provider(format!("Claude Code: {short}"))
    }
}

/// Human-readable usage-limit status from a rate_limit_event.
pub fn rate_limit_message(status: &str, kind: &str, resets_at: Option<i64>) -> Option<String> {
    let when = resets_at
        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
        .map(|d| d.with_timezone(&chrono::Local).format("%a %H:%M").to_string())
        .unwrap_or_else(|| "later".into());
    let window = match kind {
        "five_hour" => "5-hour",
        "seven_day" => "weekly",
        "seven_day_opus" => "weekly Opus",
        other => other,
    };
    match status {
        "rejected" => Some(format!("You've reached your Claude {window} usage limit. It resets {when}.")),
        "allowed_warning" => Some(format!("You're close to your Claude {window} usage limit (resets {when}).")),
        _ => None,
    }
}

// ---------------------------------------------------------------- running

pub enum SessionMode {
    New(String),
    Resume(String),
}

pub struct TurnOptions {
    pub session: SessionMode,
    pub model: String,
    pub effort: String,
    pub system_prompt: String,
    pub mcp_config: Option<PathBuf>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub idle_timeout: Duration,
}

#[derive(Debug, Default)]
pub struct TurnResult {
    pub text: String,
    pub session_id: String,
    pub model: String,
}

pub fn build_args(o: &TurnOptions) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--input-format",
        "text",
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--disable-slash-commands",
        "--no-chrome",
        "--permission-prompts",
        "none",
        "--system-prompt-snapshot",
        "off",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    match &o.session {
        SessionMode::New(id) => a.extend(["--session-id".into(), id.clone()]),
        SessionMode::Resume(id) => a.extend(["--resume".into(), id.clone()]),
    }
    if !o.model.trim().is_empty() {
        a.extend(["--model".into(), o.model.trim().into()]);
    }
    if !o.effort.trim().is_empty() {
        a.extend(["--effort".into(), o.effort.trim().into()]);
    }
    a.extend(["--system-prompt".into(), o.system_prompt.clone()]);
    if let Some(p) = &o.mcp_config {
        a.extend(["--mcp-config".into(), p.to_string_lossy().into(), "--allowedTools".into(), format!("mcp__{MCP_SERVER_NAME}")]);
    }
    a
}

/// Runs one prompt through the CLI, streaming events to `sink`.
pub async fn run_turn(cli: &Cli, o: &TurnOptions, prompt: &str, cancel: &CancellationToken, sink: &mut (dyn FnMut(CliEvent) + Send)) -> AppResult<TurnResult> {
    std::fs::create_dir_all(&o.cwd)?;
    let mut cmd = cli.command();
    cmd.args(build_args(o))
        .current_dir(&o.cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| AppError::Provider(format!("could not start Claude Code: {e}")))?;
    {
        let mut stdin = child.stdin.take().ok_or_else(|| AppError::Internal("no stdin".into()))?;
        stdin.write_all(prompt.as_bytes()).await?;
        stdin.shutdown().await?;
    }
    let stdout = child.stdout.take().ok_or_else(|| AppError::Internal("no stdout".into()))?;
    let mut stderr = child.stderr.take().ok_or_else(|| AppError::Internal("no stderr".into()))?;
    let stderr_task = tokio::spawn(async move {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s).await;
        s
    });
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let started = Instant::now();
    let mut result: Option<CliEvent> = None;
    let mut texts: Vec<String> = Vec::new();
    let mut session_id = String::new();
    let mut model = String::new();
    loop {
        let remaining = o.timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            let _ = child.kill().await;
            return Err(AppError::Provider(format!("Claude Code timed out after {}s", o.timeout.as_secs())));
        }
        let next = tokio::select! {
            r = tokio::time::timeout(o.idle_timeout.min(remaining), lines.next_line()) => r,
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                return Err(AppError::Cancelled);
            }
        };
        let line = match next {
            Ok(Ok(Some(l))) => l,
            Ok(Ok(None)) => break,
            Ok(Err(e)) => return Err(AppError::Provider(format!("reading Claude Code output: {e}"))),
            Err(_) => {
                let _ = child.kill().await;
                return Err(AppError::Provider("Claude Code stopped responding (idle timeout)".into()));
            }
        };
        for ev in parse_line(&line) {
            match &ev {
                CliEvent::Init { session_id: s, model: m } => {
                    session_id = s.clone();
                    model = m.clone();
                }
                CliEvent::AssistantText(t) => texts.push(t.clone()),
                CliEvent::Result { session_id: s, .. } => {
                    if !s.is_empty() {
                        session_id = s.clone();
                    }
                    result = Some(ev.clone());
                }
                _ => {}
            }
            sink(ev);
        }
    }
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
    let stderr_text = stderr_task.await.unwrap_or_default();
    match result {
        Some(CliEvent::Result { is_error: false, text, .. }) => {
            let joined = texts.join("\n\n");
            Ok(TurnResult { text: if joined.trim().is_empty() { text } else { joined }, session_id, model })
        }
        Some(CliEvent::Result { text, subtype, terminal_reason, .. }) => {
            let detail = if text.trim().is_empty() { format!("{subtype} {terminal_reason} {stderr_text}") } else { text };
            Err(classify_error(&detail))
        }
        _ => {
            let code = status.ok().and_then(|s| s.ok()).and_then(|s| s.code());
            let msg = if stderr_text.trim().is_empty() { format!("Claude Code exited (code {code:?}) without a result") } else { stderr_text };
            Err(classify_error(&msg))
        }
    }
}

// ---------------------------------------------------------------- MCP run registry

pub struct RunHandle {
    pub conversation_id: String,
    pub cancel: CancellationToken,
}

/// Per-run tokens that authorize the Windows AI MCP server process (spawned
/// by Claude Code) to call tools for exactly one active run.
#[derive(Default)]
pub struct CliRunRegistry {
    runs: Mutex<HashMap<String, RunHandle>>,
}

impl CliRunRegistry {
    pub fn register(&self, conversation_id: &str, cancel: CancellationToken) -> String {
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        self.runs.lock().unwrap().insert(token.clone(), RunHandle { conversation_id: conversation_id.into(), cancel });
        token
    }
    pub fn unregister(&self, token: &str) {
        self.runs.lock().unwrap().remove(token);
    }
    pub fn lookup(&self, token: &str) -> Option<(String, CancellationToken)> {
        self.runs.lock().unwrap().get(token).map(|h| (h.conversation_id.clone(), h.cancel.clone()))
    }
}

struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_mcp_config(run_id: &str, token: &str) -> AppResult<TempFile> {
    let exe = std::env::current_exe()?;
    let dir = crate::integrations::bridge::bridge_dir().join("runs");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{run_id}.json"));
    let cfg = json!({
        "mcpServers": {
            MCP_SERVER_NAME: {
                "type": "stdio",
                "command": exe.to_string_lossy(),
                "args": ["--mcp-stdio"],
                "env": { "WINDOWS_AI_RUN_TOKEN": token }
            }
        }
    });
    std::fs::write(&path, serde_json::to_string_pretty(&cfg)?)?;
    Ok(TempFile(path))
}

fn transcript(history: &[ChatMessage]) -> String {
    let mut s = String::new();
    for m in history {
        let t = text_of(&m.content);
        if t.trim().is_empty() {
            continue;
        }
        let who = if m.role == Role::User { "User" } else { "Assistant" };
        s.push_str(&format!("{who}: {}\n\n", t.trim()));
    }
    s
}

fn emit(p: &RunParams, payload: Value) {
    let mut v = payload;
    v["run_id"] = json!(p.run_id);
    v["conversation_id"] = json!(p.conversation_id);
    p.notifier.emit("chat-stream", v);
}

// Cached auth check so each message does not spawn an extra process.
static AUTH_CACHE: once_cell::sync::Lazy<Mutex<Option<(Instant, String, CliStatus)>>> = once_cell::sync::Lazy::new(|| Mutex::new(None));

async fn ensure_ready(cfg: &ClaudeCodeConfig) -> AppResult<Cli> {
    let cli = locate(&cfg.cli_path).ok_or_else(|| {
        AppError::NotFound("Claude Code is not installed. Install it from https://claude.com/claude-code (PowerShell: irm https://claude.ai/install.ps1 | iex), then sign in from Settings → AI Providers.".into())
    })?;
    let key = cli.program.to_string_lossy().to_string();
    let cached = AUTH_CACHE.lock().unwrap().clone().filter(|(t, k, _)| t.elapsed() < Duration::from_secs(120) && *k == key).map(|(_, _, s)| s);
    let st = match cached {
        Some(s) => s,
        None => {
            let s = status(cfg).await;
            *AUTH_CACHE.lock().unwrap() = Some((Instant::now(), key, s.clone()));
            s
        }
    };
    if !st.logged_in {
        return Err(AppError::MissingCredential("Claude Code is not signed in. Open Settings → AI Providers and click “Sign in with Claude”.".into()));
    }
    if st.billing != "subscription" && !cfg.allow_api_billing {
        return Err(AppError::PermissionDenied(format!(
            "Claude Code is signed in with {} (API billing), not a Claude subscription. Windows AI will not use paid API billing in subscription mode. Sign in with your Claude subscription, or explicitly allow API billing in settings.",
            st.auth_method.unwrap_or_else(|| "an API account".into())
        )));
    }
    Ok(cli)
}

pub fn invalidate_auth_cache() {
    *AUTH_CACHE.lock().unwrap() = None;
}

/// Conversation backend for subscription mode.
pub async fn run_conversation(p: &RunParams, cli_override: Option<Cli>) -> AppResult<()> {
    let cfg = &p.cfg.ai.claude_code;
    let cli = match cli_override {
        Some(c) => c,
        None => ensure_ready(cfg).await?,
    };
    let conv = memory::get_conversation(&p.db, &p.conversation_id)?;
    let all = memory::history_for_model(&p.db, &p.conversation_id, 400)?;
    let (last, earlier) = all.split_last().ok_or_else(|| AppError::InvalidInput("no message to send".into()))?;
    if last.role != Role::User {
        return Err(AppError::InvalidInput("last message is not from the user".into()));
    }
    let user_text = text_of(&last.content);
    let project = conv.project_id.as_deref().and_then(|id| memory::get_project(&p.db, id).ok());
    let memories = if p.cfg.memory.auto_context {
        memory::context_for(&p.db, &user_text, conv.project_id.as_deref(), p.cfg.memory.max_context_items as usize)?
    } else {
        vec![]
    };
    let mut system = build_system_prompt(&p.cfg, &memories, project.as_ref());
    let tools_on = cfg.enable_tools && p.cli_runs.is_some();
    if tools_on {
        system.push_str(&format!("\n\nYour Windows tools are provided by the `{MCP_SERVER_NAME}` MCP server (tool names start with mcp__{MCP_SERVER_NAME}__)."));
    } else {
        system.push_str("\n\nNo tools are available in this session; answer from the conversation and captured context only.");
    }

    // MCP access token for this run only.
    let token = p.cli_runs.as_ref().filter(|_| tools_on).map(|r| r.register(&p.conversation_id, p.cancel.clone()));
    let _mcp_file = match &token {
        Some(t) => Some(write_mcp_config(&p.run_id, t)?),
        None => None,
    };
    let result = async {
        let existing = memory::get_cli_session(&p.db, &p.conversation_id)?;
        let attempt = |session: SessionMode, prompt: String| {
            let opts = TurnOptions {
                session,
                model: cfg.model.clone(),
                effort: cfg.effort.clone(),
                system_prompt: system.clone(),
                mcp_config: _mcp_file.as_ref().map(|f| f.0.clone()),
                cwd: p.data_dir.join("claude-workdir"),
                timeout: Duration::from_secs(cfg.timeout_secs.max(30)),
                idle_timeout: Duration::from_secs(p.cfg.permissions.approval_timeout_secs.max(60) + 120),
            };
            (opts, prompt)
        };
        let fresh_prompt = || {
            let t = transcript(earlier);
            if t.is_empty() {
                user_text.clone()
            } else {
                format!("<previous_conversation>\n{t}</previous_conversation>\n\n{user_text}")
            }
        };
        let mut names: HashMap<String, String> = HashMap::new();
        let mut sink = |ev: CliEvent| match ev {
            CliEvent::TextDelta(t) => emit(p, json!({"type": "delta", "event": {"type": "text_delta", "text": t}})),
            CliEvent::ThinkingDelta(t) => emit(p, json!({"type": "delta", "event": {"type": "thinking_delta", "text": t}})),
            CliEvent::ToolStart { id, name } => {
                let short = name.strip_prefix(&format!("mcp__{MCP_SERVER_NAME}__")).unwrap_or(&name).to_string();
                names.insert(id.clone(), short.clone());
                emit(p, json!({"type": "tool_running", "tool_use_id": id, "name": short}));
            }
            CliEvent::ToolResult { id, is_error, content } => {
                let name = names.get(&id).cloned().unwrap_or_default();
                emit(p, json!({"type": "tool_result", "tool_use_id": id, "name": name, "status": if is_error { "error" } else { "success" }, "is_error": is_error, "content": content}));
            }
            CliEvent::RateLimit { status, kind, resets_at, utilization } => {
                if let Some(msg) = rate_limit_message(&status, &kind, resets_at) {
                    emit(p, json!({"type": "usage", "status": status, "message": msg, "utilization": utilization}));
                }
            }
            _ => {}
        };
        let (opts, prompt) = match &existing {
            Some(id) => attempt(SessionMode::Resume(id.clone()), user_text.clone()),
            None => attempt(SessionMode::New(uuid::Uuid::new_v4().to_string()), fresh_prompt()),
        };
        emit(p, json!({"type": "turn_start", "iteration": 0}));
        let r = run_turn(&cli, &opts, &prompt, &p.cancel, &mut sink).await;
        match r {
            Err(AppError::NotFound(_)) if existing.is_some() => {
                // The CLI no longer has that session: start a new one with the transcript.
                memory::set_cli_session(&p.db, &p.conversation_id, None)?;
                let (opts, prompt) = attempt(SessionMode::New(uuid::Uuid::new_v4().to_string()), fresh_prompt());
                run_turn(&cli, &opts, &prompt, &p.cancel, &mut sink).await
            }
            other => other,
        }
    }
    .await;
    if let (Some(t), Some(r)) = (&token, &p.cli_runs) {
        r.unregister(t);
    }
    if matches!(result, Err(AppError::MissingCredential(_))) {
        invalidate_auth_cache();
    }
    let turn = result?;
    if !turn.session_id.is_empty() {
        memory::set_cli_session(&p.db, &p.conversation_id, Some(&turn.session_id))?;
    }
    let msg = ChatMessage { role: Role::Assistant, content: vec![ContentBlock::Text { text: turn.text }] };
    let label = if turn.model.is_empty() { "claude-code".to_string() } else { format!("claude-code:{}", turn.model) };
    let stored = memory::append_message(&p.db, &p.conversation_id, &msg, Some(&label))?;
    emit(p, json!({"type": "message", "message": stored}));
    Ok(())
}

pub fn model_choices() -> Vec<super::provider::ModelInfo> {
    [("", "Claude Code default"), ("opus", "Opus (latest)"), ("sonnet", "Sonnet (latest)"), ("haiku", "Haiku (latest)"), ("fable", "Fable (latest)")]
        .iter()
        .map(|(id, l)| super::provider::ModelInfo { id: (*id).into(), label: (*l).into() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_billing_env_vars() {
        assert!(is_stripped_env("ANTHROPIC_API_KEY"));
        assert!(is_stripped_env("anthropic_base_url"));
        assert!(is_stripped_env("CLAUDE_CODE_USE_BEDROCK"));
        assert!(is_stripped_env("CLAUDECODE"));
        assert!(!is_stripped_env("CLAUDE_CONFIG_DIR"));
        assert!(!is_stripped_env("PATH"));
    }

    #[test]
    fn auth_status_billing() {
        let sub = json!({"loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty", "subscriptionType": "max", "email": "x@y.z"});
        let (li, m, s, b, _) = parse_auth_status(&sub);
        assert!(li);
        assert_eq!(m.as_deref(), Some("claude.ai"));
        assert_eq!(s.as_deref(), Some("max"));
        assert_eq!(b, "subscription");
        assert_eq!(parse_auth_status(&json!({"loggedIn": true, "authMethod": "console"})).3, "api");
        assert_eq!(parse_auth_status(&json!({"loggedIn": true, "authMethod": "claude.ai", "apiProvider": "bedrock"})).3, "api");
        assert_eq!(parse_auth_status(&json!({"loggedIn": false})).3, "unknown");
    }

    #[test]
    fn parses_stream_lines() {
        let init = r#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-opus-5-5","tools":[]}"#;
        assert_eq!(parse_line(init), vec![CliEvent::Init { session_id: "s1".into(), model: "claude-opus-5-5".into() }]);
        let d = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}},"parent_tool_use_id":null}"#;
        assert_eq!(parse_line(d), vec![CliEvent::TextDelta("hi".into())]);
        let sub = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"x"}},"parent_tool_use_id":"toolu_1"}"#;
        assert!(parse_line(sub).is_empty(), "subagent text is ignored");
        let tool = r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t1","name":"mcp__windows_ai__read_file","input":{}}}}"#;
        assert_eq!(parse_line(tool), vec![CliEvent::ToolStart { id: "t1".into(), name: "mcp__windows_ai__read_file".into() }]);
        let tr = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":[{"type":"text","text":"denied"}]}]}}"#;
        assert_eq!(parse_line(tr), vec![CliEvent::ToolResult { id: "t1".into(), is_error: true, content: "denied".into() }]);
        let res = r#"{"type":"result","subtype":"success","is_error":false,"result":"done","session_id":"s1","terminal_reason":"completed"}"#;
        assert!(matches!(&parse_line(res)[0], CliEvent::Result { is_error: false, text, .. } if text == "done"));
        let rl = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1794938400,"rateLimitType":"five_hour"}}"#;
        assert!(matches!(&parse_line(rl)[0], CliEvent::RateLimit { status, .. } if status == "rejected"));
        assert!(parse_line("not json").is_empty());
    }

    #[test]
    fn classifies_errors() {
        assert!(classify_error("Claude AI usage limit reached|1794938400").to_string().contains("usage limit"));
        assert_eq!(classify_error("Invalid API key · Please run /login").kind(), "missing_credential");
        assert_eq!(classify_error("OAuth token has expired. Please obtain a new token").kind(), "missing_credential");
        assert_eq!(classify_error("No conversation found with session ID: abc").kind(), "not_found");
        assert_eq!(classify_error("something else").kind(), "provider");
        assert!(rate_limit_message("rejected", "five_hour", Some(1794938400)).unwrap().contains("5-hour"));
        assert!(rate_limit_message("allowed", "five_hour", None).is_none());
    }

    #[test]
    fn args_disable_builtin_tools_and_isolate_settings() {
        let o = TurnOptions {
            session: SessionMode::Resume("abc".into()),
            model: "sonnet".into(),
            effort: "".into(),
            system_prompt: "sys\nline2".into(),
            mcp_config: Some(PathBuf::from("C:\\x.json")),
            cwd: PathBuf::from("C:\\w"),
            timeout: Duration::from_secs(1),
            idle_timeout: Duration::from_secs(1),
        };
        let a = build_args(&o);
        let pos = |s: &str| a.iter().position(|x| x == s).unwrap();
        assert_eq!(a[pos("--tools") + 1], "");
        assert_eq!(a[pos("--resume") + 1], "abc");
        assert_eq!(a[pos("--model") + 1], "sonnet");
        assert_eq!(a[pos("--allowedTools") + 1], "mcp__windows_ai");
        assert!(a.contains(&"--strict-mcp-config".to_string()));
        assert!(!a.contains(&"--effort".to_string()));
        assert!(!a.iter().any(|x| x.contains("dangerously")));
    }

    #[test]
    fn missing_cli_is_reported() {
        assert!(locate("C:\\definitely\\missing\\claude.exe").is_none());
    }

    // ------------------------------------------------ backend tests (fake + live CLI)

    use crate::config::AppConfig;
    use crate::db::Db;
    use crate::integrations::context::{compose_prompt, CapturedContext};
    use crate::mcp::McpManager;
    use crate::permissions::{Approvals, Notifier};
    use crate::tools::executor::ToolHub;
    use std::sync::Arc;

    #[derive(Default)]
    struct Rec(Mutex<Vec<Value>>);
    impl Notifier for Rec {
        fn emit(&self, _event: &str, payload: Value) {
            self.0.lock().unwrap().push(payload);
        }
    }

    fn fake_cli() -> Option<Cli> {
        if std::process::Command::new("node").arg("--version").output().is_err() {
            eprintln!("node not available; skipping");
            return None;
        }
        let js = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("fake_claude.js");
        Some(Cli { program: PathBuf::from("node"), prefix: vec![js.to_string_lossy().into()] })
    }

    fn params(db: &Db, conv: &str, rec: Arc<Rec>, cancel: CancellationToken, model: &str) -> RunParams {
        let mut cfg = AppConfig::default();
        cfg.ai.claude_code.model = model.into();
        RunParams {
            run_id: "run".into(),
            conversation_id: conv.into(),
            cfg,
            db: db.clone(),
            provider: None,
            hub: Arc::new(ToolHub::new(Arc::new(McpManager::new()), Approvals::new())),
            notifier: rec,
            cancel,
            data_dir: std::env::temp_dir().join("windows-ai-tests"),
            cli_runs: None,
        }
    }

    fn user(db: &Db, conv: &str, text: &str) {
        memory::append_message(db, conv, &ChatMessage { role: Role::User, content: vec![ContentBlock::Text { text: text.into() }] }, None).unwrap();
    }

    fn last_text(db: &Db, conv: &str) -> String {
        text_of(&memory::list_messages(db, conv).unwrap().last().unwrap().content)
    }

    #[tokio::test]
    async fn fake_cli_selection_explain_and_continuation() {
        let Some(cli) = fake_cli() else { return };
        std::env::set_var("ANTHROPIC_WINDOWS_AI_LEAK_TEST", "should-not-leak");
        let db = Db::open_in_memory().unwrap();
        let conv = memory::create_conversation(&db, "t", None).unwrap().id;
        let mut sel = CapturedContext::new("selection", "text", "Photosynthesis converts light to chemical energy.".into());
        sel.app = Some("Microsoft Edge".into());
        user(&db, &conv, &compose_prompt(&[sel], Some("explain"), "", "English"));
        let rec = Arc::new(Rec::default());
        run_conversation(&params(&db, &conv, rec.clone(), CancellationToken::new(), ""), Some(cli.clone())).await.unwrap();
        let first = last_text(&db, &conv);
        assert!(first.contains("resumed=false"), "{first}");
        assert!(first.contains("leaked_env=0"), "Anthropic/Claude env vars must be stripped: {first}");
        assert!(first.contains("tools=none"), "built-in Claude Code tools must be disabled");
        assert!(first.contains("<captured_context source=\"selection\" kind=\"text\" app=\"Microsoft Edge\">"));
        assert!(first.contains("Explain the captured text"));
        assert!(rec.0.lock().unwrap().iter().any(|v| v["type"] == "delta"), "streams deltas");
        let session = memory::get_cli_session(&db, &conv).unwrap().unwrap();

        user(&db, &conv, "and in simpler words?");
        run_conversation(&params(&db, &conv, Arc::new(Rec::default()), CancellationToken::new(), ""), Some(cli)).await.unwrap();
        let second = last_text(&db, &conv);
        assert!(second.contains(&format!("session={session} resumed=true")), "{second}");
        assert!(second.ends_with("prompt=and in simpler words?"), "resume sends only the new message: {second}");
    }

    #[tokio::test]
    async fn fake_cli_error_cases() {
        let Some(cli) = fake_cli() else { return };
        let db = Db::open_in_memory().unwrap();
        let conv = memory::create_conversation(&db, "t", None).unwrap().id;

        user(&db, &conv, "TRIGGER_LIMIT");
        let rec = Arc::new(Rec::default());
        let e = run_conversation(&params(&db, &conv, rec.clone(), CancellationToken::new(), ""), Some(cli.clone())).await.unwrap_err();
        assert!(e.to_string().contains("usage limit"), "{e}");
        assert!(rec.0.lock().unwrap().iter().any(|v| v["type"] == "usage" && v["status"] == "rejected"));

        let conv = memory::create_conversation(&db, "auth", None).unwrap().id;
        user(&db, &conv, "TRIGGER_AUTH");
        let e = run_conversation(&params(&db, &conv, Arc::new(Rec::default()), CancellationToken::new(), ""), Some(cli.clone())).await.unwrap_err();
        assert_eq!(e.kind(), "missing_credential");

        let conv = memory::create_conversation(&db, "crash", None).unwrap().id;
        user(&db, &conv, "TRIGGER_CRASH");
        let e = run_conversation(&params(&db, &conv, Arc::new(Rec::default()), CancellationToken::new(), ""), Some(cli.clone())).await.unwrap_err();
        assert!(e.to_string().contains("boom"), "{e}");

        let missing = Cli { program: PathBuf::from("C:\\nope\\claude.exe"), prefix: vec![] };
        user(&db, &conv, "hello");
        let e = run_conversation(&params(&db, &conv, Arc::new(Rec::default()), CancellationToken::new(), ""), Some(missing)).await.unwrap_err();
        assert!(e.to_string().contains("could not start Claude Code"), "{e}");
    }

    #[tokio::test]
    async fn fake_cli_cancellation_is_prompt() {
        let Some(cli) = fake_cli() else { return };
        let db = Db::open_in_memory().unwrap();
        let conv = memory::create_conversation(&db, "t", None).unwrap().id;
        user(&db, &conv, "TRIGGER_SLOW");
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(700)).await;
            c2.cancel();
        });
        let t = Instant::now();
        let e = run_conversation(&params(&db, &conv, Arc::new(Rec::default()), cancel, ""), Some(cli)).await.unwrap_err();
        assert_eq!(e.kind(), "cancelled");
        assert!(t.elapsed() < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn fake_cli_lost_session_falls_back_with_transcript() {
        let Some(cli) = fake_cli() else { return };
        let db = Db::open_in_memory().unwrap();
        let conv = memory::create_conversation(&db, "t", None).unwrap().id;
        user(&db, &conv, "my name is Ada");
        memory::append_message(&db, &conv, &ChatMessage { role: Role::Assistant, content: vec![ContentBlock::Text { text: "Hi Ada".into() }] }, None).unwrap();
        memory::set_cli_session(&db, &conv, Some("gone")).unwrap();
        user(&db, &conv, "what is my name?");
        run_conversation(&params(&db, &conv, Arc::new(Rec::default()), CancellationToken::new(), ""), Some(cli)).await.unwrap();
        let t = last_text(&db, &conv);
        assert!(t.contains("resumed=false"));
        assert!(t.contains("<previous_conversation>") && t.contains("User: my name is Ada") && t.contains("Assistant: Hi Ada"), "{t}");
        assert_ne!(memory::get_cli_session(&db, &conv).unwrap().as_deref(), Some("gone"));
    }

    // Live tests against the real Claude Code CLI and the user's subscription.
    // Run explicitly: cargo test live_ -- --ignored --test-threads=1

    async fn live_ready() -> Cli {
        ensure_ready(&ClaudeCodeConfig::default()).await.expect("Claude Code must be installed and signed in with a subscription")
    }

    #[tokio::test]
    #[ignore = "uses the real Claude Code CLI and subscription usage"]
    async fn live_status_reports_subscription() {
        let st = status(&ClaudeCodeConfig::default()).await;
        assert!(st.installed, "{st:?}");
        assert!(st.logged_in, "{st:?}");
        assert_eq!(st.billing, "subscription", "{st:?}");
    }

    #[tokio::test]
    #[ignore = "uses the real Claude Code CLI and subscription usage"]
    async fn live_basic_selection_and_continuation() {
        let cli = live_ready().await;
        let db = Db::open_in_memory().unwrap();
        let conv = memory::create_conversation(&db, "t", None).unwrap().id;
        user(&db, &conv, "Reply with exactly the word: pong");
        run_conversation(&params(&db, &conv, Arc::new(Rec::default()), CancellationToken::new(), "haiku"), Some(cli.clone())).await.unwrap();
        assert!(last_text(&db, &conv).to_lowercase().contains("pong"), "{}", last_text(&db, &conv));

        let sel = CapturedContext::new("selection", "code", "let total = items.iter().map(|i| i.price).sum::<f64>();".into());
        user(&db, &conv, &compose_prompt(&[sel], Some("explain"), "One sentence.", "English"));
        run_conversation(&params(&db, &conv, Arc::new(Rec::default()), CancellationToken::new(), "haiku"), Some(cli.clone())).await.unwrap();
        let ex = last_text(&db, &conv).to_lowercase();
        assert!(ex.contains("price") || ex.contains("sum"), "{ex}");

        user(&db, &conv, "Remember the secret word 'heliotrope'. Reply OK.");
        run_conversation(&params(&db, &conv, Arc::new(Rec::default()), CancellationToken::new(), "haiku"), Some(cli.clone())).await.unwrap();
        user(&db, &conv, "What was the secret word? Answer with the word only.");
        run_conversation(&params(&db, &conv, Arc::new(Rec::default()), CancellationToken::new(), "haiku"), Some(cli)).await.unwrap();
        assert!(last_text(&db, &conv).to_lowercase().contains("heliotrope"), "{}", last_text(&db, &conv));
    }

    #[tokio::test]
    #[ignore = "uses the real Claude Code CLI and subscription usage"]
    async fn live_long_response_and_cancel() {
        let cli = live_ready().await;
        let db = Db::open_in_memory().unwrap();
        let conv = memory::create_conversation(&db, "t", None).unwrap().id;
        user(&db, &conv, "Write a 500-word overview of the history of the Windows operating system.");
        let rec = Arc::new(Rec::default());
        run_conversation(&params(&db, &conv, rec.clone(), CancellationToken::new(), "haiku"), Some(cli.clone())).await.unwrap();
        let text = last_text(&db, &conv);
        assert!(text.len() > 1500, "long response ({} chars)", text.len());
        assert!(rec.0.lock().unwrap().iter().filter(|v| v["type"] == "delta").count() > 10, "streamed in many chunks");

        user(&db, &conv, "Now write a 2000-word essay about operating system kernels.");
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(4)).await;
            c2.cancel();
        });
        let e = run_conversation(&params(&db, &conv, Arc::new(Rec::default()), cancel, "haiku"), Some(cli)).await.unwrap_err();
        assert_eq!(e.kind(), "cancelled");
    }

    #[test]
    fn registry_tokens() {
        let r = CliRunRegistry::default();
        let t = r.register("conv", CancellationToken::new());
        assert_eq!(r.lookup(&t).unwrap().0, "conv");
        r.unregister(&t);
        assert!(r.lookup(&t).is_none());
    }
}
