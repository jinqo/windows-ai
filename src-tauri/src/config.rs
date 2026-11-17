//! Application configuration.
//!
//! Non-secret settings live in `config.json` under the app config dir.
//! Secrets (API keys) never go here: see `secrets.rs` (Windows Credential Manager).

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub appearance: AppearanceConfig,
    pub window: WindowConfig,
    pub ai: AiConfig,
    pub memory: MemoryConfig,
    pub voice: VoiceConfig,
    pub shortcuts: ShortcutConfig,
    pub startup: StartupConfig,
    pub permissions: PermissionsConfig,
    pub mcp: McpConfig,
    pub integrations: IntegrationsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct IntegrationsConfig {
    /// Global shortcut that captures the current text selection.
    pub selection_enabled: bool,
    pub selection_shortcut: String,
    /// "toolbar" (small floating action bar at the cursor) | "overlay"
    pub selection_mode: String,
    /// Use UI Automation first; fall back to a simulated Ctrl+C that
    /// restores the previous clipboard contents afterwards.
    pub clipboard_fallback: bool,
    /// Accept captures from the browser and VS Code extensions.
    pub bridge_enabled: bool,
    /// Max characters of captured context sent to the model.
    pub max_context_chars: u32,
    /// Default target language for the Translate action.
    pub translate_language: String,
}

impl Default for IntegrationsConfig {
    fn default() -> Self {
        Self {
            selection_enabled: true,
            selection_shortcut: "Ctrl+Shift+Space".into(),
            selection_mode: "toolbar".into(),
            clipboard_fallback: true,
            bridge_enabled: true,
            max_context_chars: 100_000,
            translate_language: "English".into(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            appearance: AppearanceConfig::default(),
            window: WindowConfig::default(),
            ai: AiConfig::default(),
            memory: MemoryConfig::default(),
            voice: VoiceConfig::default(),
            shortcuts: ShortcutConfig::default(),
            startup: StartupConfig::default(),
            permissions: PermissionsConfig::default(),
            mcp: McpConfig::default(),
            integrations: IntegrationsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppearanceConfig {
    /// Accent hue name: "violet" | "purple" | "indigo" | "fuchsia"
    pub accent: String,
    /// Background opacity of the overlay surface, 0.6 - 1.0
    pub opacity: f32,
    pub font_size: u32,
    pub compact: bool,
    pub reduce_motion: bool,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self { accent: "violet".into(), opacity: 0.98, font_size: 14, compact: false, reduce_motion: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
    /// "center" | "top" | "remember"
    pub position: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub always_on_top: bool,
    pub hide_on_blur: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 860,
            height: 640,
            position: "center".into(),
            x: None,
            y: None,
            always_on_top: true,
            hide_on_blur: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AiConfig {
    /// "claude_code" (Claude subscription via the official CLI) |
    /// "anthropic" (API key) | "openai_compatible"
    pub provider: String,
    pub claude_code: ClaudeCodeConfig,
    pub anthropic: AnthropicConfig,
    pub openai_compatible: OpenAiCompatibleConfig,
    /// Extra instructions appended to the built-in system prompt.
    pub custom_instructions: String,
    /// Max agent iterations (model turns) per user message.
    pub max_tool_iterations: u32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: "claude_code".into(),
            claude_code: ClaudeCodeConfig::default(),
            anthropic: AnthropicConfig::default(),
            openai_compatible: OpenAiCompatibleConfig::default(),
            custom_instructions: String::new(),
            max_tool_iterations: 12,
        }
    }
}

/// Claude subscription mode: runs the official, unmodified Claude Code CLI
/// as a local child process. Authentication is entirely handled by the CLI
/// (`claude auth login`); Windows AI never reads its tokens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClaudeCodeConfig {
    /// Path to claude.exe; empty = auto-detect.
    pub cli_path: String,
    /// Model alias or full id (e.g. "sonnet", "opus", "claude-opus-5-5"); empty = CLI default.
    pub model: String,
    /// "" (CLI default) | low | medium | high | xhigh | max
    pub effort: String,
    /// Expose Windows AI's permission-checked tools to Claude Code over MCP.
    pub enable_tools: bool,
    /// Hard limit for one request, seconds.
    pub timeout_secs: u64,
    /// Explicit opt-in to run when the CLI is signed in with API (Console)
    /// billing instead of a subscription. Off by default.
    pub allow_api_billing: bool,
}

impl Default for ClaudeCodeConfig {
    fn default() -> Self {
        Self { cli_path: String::new(), model: String::new(), effort: String::new(), enable_tools: true, timeout_secs: 900, allow_api_billing: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AnthropicConfig {
    pub model: String,
    pub max_tokens: u32,
    /// "low" | "medium" | "high" | "xhigh" | "max"
    pub effort: String,
    pub base_url: String,
    /// Opt into server-side refusal fallbacks (Claude API only).
    pub refusal_fallbacks: bool,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            model: "claude-opus-5-5".into(),
            max_tokens: 32000,
            effort: "medium".into(),
            base_url: "https://api.anthropic.com".into(),
            refusal_fallbacks: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct OpenAiCompatibleConfig {
    /// e.g. http://localhost:11434/v1 for Ollama
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    /// Whether to send a stored API key as a bearer token.
    pub use_api_key: bool,
}

impl Default for OpenAiCompatibleConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".into(),
            model: "llama3.1".into(),
            max_tokens: 4096,
            use_api_key: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MemoryConfig {
    /// Inject relevant memories into the system prompt.
    pub auto_context: bool,
    pub max_context_items: u32,
    /// Let the assistant propose saving memories (always requires approval).
    pub allow_ai_save: bool,
    /// Number of prior messages sent with each request.
    pub history_window: u32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self { auto_context: true, max_context_items: 8, allow_ai_save: true, history_window: 40 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VoiceConfig {
    /// "windows" | "openai_whisper" | "none"
    pub stt_provider: String,
    pub stt_language: String,
    pub whisper_base_url: String,
    pub tts_enabled: bool,
    pub tts_voice: String,
    pub tts_rate: f32,
    pub tts_pitch: f32,
    pub tts_volume: f32,
    pub auto_send_transcript: bool,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            stt_provider: "windows".into(),
            stt_language: "en-US".into(),
            whisper_base_url: "https://api.openai.com/v1".into(),
            tts_enabled: false,
            tts_voice: String::new(),
            tts_rate: 1.0,
            tts_pitch: 1.0,
            tts_volume: 1.0,
            auto_send_transcript: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ShortcutConfig {
    pub toggle_overlay: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self { toggle_overlay: "Alt+Space".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct StartupConfig {
    pub launch_on_login: bool,
    pub start_hidden: bool,
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self { launch_on_login: false, start_hidden: false }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Run without asking.
    Allow,
    /// Ask the user every time.
    Ask,
    /// Never run.
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DirectoryScope {
    pub path: String,
    pub read: bool,
    pub write: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovedApp {
    /// Name the assistant uses, e.g. "notepad".
    pub name: String,
    /// Executable path or command on PATH.
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllowedCommand {
    /// Identifier the assistant uses, e.g. "git-status".
    pub name: String,
    pub description: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PermissionsConfig {
    /// Per-tool override. Missing entries fall back to the tool's default mode.
    pub tool_modes: HashMap<String, PermissionMode>,
    pub directories: Vec<DirectoryScope>,
    pub apps: Vec<ApprovedApp>,
    pub commands: Vec<AllowedCommand>,
    pub max_read_bytes: u64,
    pub approval_timeout_secs: u64,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            tool_modes: HashMap::new(),
            directories: Vec::new(),
            apps: vec![
                ApprovedApp { name: "notepad".into(), path: "notepad.exe".into(), args: vec![] },
                ApprovedApp { name: "calculator".into(), path: "calc.exe".into(), args: vec![] },
                ApprovedApp { name: "explorer".into(), path: "explorer.exe".into(), args: vec![] },
            ],
            commands: Vec::new(),
            max_read_bytes: 512 * 1024,
            approval_timeout_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    /// Tools the user explicitly trusted. Newly discovered tools are untrusted.
    #[serde(default)]
    pub trusted_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct McpConfig {
    pub servers: Vec<McpServerConfig>,
}

impl AppConfig {
    pub fn validate(&self) -> AppResult<()> {
        if !(0.5..=1.0).contains(&self.appearance.opacity) {
            return Err(AppError::Config("opacity must be between 0.5 and 1.0".into()));
        }
        if self.window.width < 420 || self.window.height < 360 {
            return Err(AppError::Config("window must be at least 420x360".into()));
        }
        if !["", "low", "medium", "high", "xhigh", "max"].contains(&self.ai.claude_code.effort.as_str()) {
            return Err(AppError::Config("Claude Code effort must be empty or low|medium|high|xhigh|max".into()));
        }
        if self.ai.claude_code.model.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '[' || c == ']')) {
            return Err(AppError::Config("invalid Claude Code model name".into()));
        }
        if !["claude_code", "anthropic", "openai_compatible"].contains(&self.ai.provider.as_str()) {
            return Err(AppError::Config(format!("unknown provider '{}'", self.ai.provider)));
        }
        if !["low", "medium", "high", "xhigh", "max"].contains(&self.ai.anthropic.effort.as_str()) {
            return Err(AppError::Config("effort must be low|medium|high|xhigh|max".into()));
        }
        if self.ai.max_tool_iterations == 0 || self.ai.max_tool_iterations > 50 {
            return Err(AppError::Config("max_tool_iterations must be 1-50".into()));
        }
        let mut ids = std::collections::HashSet::new();
        for s in &self.mcp.servers {
            if s.id.is_empty() || !s.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err(AppError::Config(format!("invalid MCP server id '{}'", s.id)));
            }
            if !ids.insert(&s.id) {
                return Err(AppError::Config(format!("duplicate MCP server id '{}'", s.id)));
            }
        }
        if !["toolbar", "overlay"].contains(&self.integrations.selection_mode.as_str()) {
            return Err(AppError::Config("selection_mode must be toolbar|overlay".into()));
        }
        if self.integrations.selection_enabled && self.integrations.selection_shortcut == self.shortcuts.toggle_overlay {
            return Err(AppError::Config("selection shortcut must differ from the overlay shortcut".into()));
        }
        for c in &self.permissions.commands {
            if c.name.trim().is_empty() || c.program.trim().is_empty() {
                return Err(AppError::Config("allowed commands need a name and program".into()));
            }
        }
        Ok(())
    }
}

/// Thread-safe config holder that persists to disk.
pub struct ConfigStore {
    path: PathBuf,
    inner: RwLock<AppConfig>,
}

impl ConfigStore {
    pub fn load(path: PathBuf) -> Self {
        let cfg = read_config(&path).unwrap_or_else(|e| {
            log::warn!("config load failed ({e}); using defaults");
            AppConfig::default()
        });
        Self { path, inner: RwLock::new(cfg) }
    }

    pub fn get(&self) -> AppConfig {
        self.inner.read().expect("config lock").clone()
    }

    pub fn set(&self, cfg: AppConfig) -> AppResult<()> {
        cfg.validate()?;
        write_config(&self.path, &cfg)?;
        *self.inner.write().expect("config lock") = cfg;
        Ok(())
    }

    pub fn update<F: FnOnce(&mut AppConfig)>(&self, f: F) -> AppResult<AppConfig> {
        let mut cfg = self.get();
        f(&mut cfg);
        self.set(cfg.clone())?;
        Ok(cfg)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn read_config(path: &Path) -> AppResult<AppConfig> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = std::fs::read_to_string(path)?;
    let cfg: AppConfig = serde_json::from_str(&text)
        .map_err(|e| AppError::Config(format!("invalid config.json: {e}")))?;
    Ok(cfg)
}

pub fn write_config(path: &Path, cfg: &AppConfig) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(cfg)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate() {
        AppConfig::default().validate().unwrap();
    }

    #[test]
    fn partial_json_fills_defaults() {
        let cfg: AppConfig = serde_json::from_str(r#"{"ai":{"provider":"openai_compatible"}}"#).unwrap();
        assert_eq!(cfg.ai.provider, "openai_compatible");
        assert_eq!(cfg.ai.anthropic.model, "claude-opus-5-5");
        assert_eq!(cfg.shortcuts.toggle_overlay, "Alt+Space");
    }

    #[test]
    fn roundtrip_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        let store = ConfigStore::load(p.clone());
        store.update(|c| c.appearance.opacity = 0.8).unwrap();
        let again = read_config(&p).unwrap();
        assert!((again.appearance.opacity - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn rejects_bad_values() {
        let mut c = AppConfig::default();
        c.appearance.opacity = 0.1;
        assert!(c.validate().is_err());
        let mut c = AppConfig::default();
        c.mcp.servers.push(McpServerConfig {
            id: "bad id".into(),
            name: "x".into(),
            command: "x".into(),
            args: vec![],
            env: HashMap::new(),
            cwd: None,
            enabled: false,
            trusted_tools: vec![],
        });
        assert!(c.validate().is_err());
    }

    #[test]
    fn example_config_parses() {
        let text = include_str!("../../config.example.json");
        let cfg: AppConfig = serde_json::from_str(text).unwrap();
        cfg.validate().unwrap();
    }
}
