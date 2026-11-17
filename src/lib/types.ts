// Types mirroring the Rust backend (serde shapes).

export type PermissionMode = "allow" | "ask" | "deny";

export interface DirectoryScope { path: string; read: boolean; write: boolean }
export interface ApprovedApp { name: string; path: string; args: string[] }
export interface AllowedCommand { name: string; description: string; program: string; args: string[]; cwd?: string | null; timeout_secs: number }
export interface McpServerConfig { id: string; name: string; command: string; args: string[]; env: Record<string, string>; cwd?: string | null; enabled: boolean; trusted_tools: string[] }

export interface AppConfig {
  appearance: { accent: string; opacity: number; font_size: number; compact: boolean; reduce_motion: boolean };
  window: { width: number; height: number; position: string; x?: number | null; y?: number | null; always_on_top: boolean; hide_on_blur: boolean };
  ai: {
    provider: "claude_code" | "anthropic" | "openai_compatible";
    claude_code: { cli_path: string; model: string; effort: string; enable_tools: boolean; timeout_secs: number; allow_api_billing: boolean };
    anthropic: { model: string; max_tokens: number; effort: string; base_url: string; refusal_fallbacks: boolean };
    openai_compatible: { base_url: string; model: string; max_tokens: number; use_api_key: boolean };
    fallback: { enabled: boolean; base_url: string; models: string[]; max_tokens: number };
    custom_instructions: string;
    max_tool_iterations: number;
  };
  memory: { auto_context: boolean; max_context_items: number; allow_ai_save: boolean; history_window: number };
  voice: {
    stt_provider: "windows" | "openai_whisper" | "none";
    stt_language: string; whisper_base_url: string;
    tts_enabled: boolean; tts_voice: string; tts_rate: number; tts_pitch: number; tts_volume: number;
    auto_send_transcript: boolean;
  };
  shortcuts: { toggle_overlay: string };
  startup: { launch_on_login: boolean; start_hidden: boolean };
  permissions: {
    tool_modes: Record<string, PermissionMode>;
    directories: DirectoryScope[]; apps: ApprovedApp[]; commands: AllowedCommand[];
    max_read_bytes: number; approval_timeout_secs: number;
  };
  mcp: { servers: McpServerConfig[] };
  integrations: {
    selection_enabled: boolean; selection_shortcut: string; selection_mode: "toolbar" | "overlay";
    clipboard_fallback: boolean; bridge_enabled: boolean; max_context_chars: number; translate_language: string;
  };
}

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "thinking"; thinking: string; signature?: string }
  | { type: "redacted_thinking"; data: string }
  | { type: "tool_use"; id: string; name: string; input: unknown }
  | { type: "tool_result"; tool_use_id: string; content: string; is_error: boolean };

export interface StoredMessage {
  id: string; conversation_id: string; role: "user" | "assistant";
  content: ContentBlock[]; model?: string | null; created_at: string;
}

export interface Conversation {
  id: string; title: string; project_id?: string | null; pinned: boolean;
  created_at: string; updated_at: string; message_count: number;
}

export interface Memory {
  id: string; kind: "preference" | "fact" | "project" | "note"; content: string; tags: string;
  project_id?: string | null; source: string; pinned: boolean; created_at: string; updated_at: string;
}

export interface Project { id: string; name: string; description: string; root_path?: string | null; context: string; created_at: string; updated_at: string }

export interface ApprovalRequest {
  id: string; tool: string; title: string; description: string; input: unknown;
  access: string; risk: "low" | "medium" | "high"; source: string; allow_session: boolean;
}

export interface ActivityEntry {
  id: string; created_at: string; finished_at?: string | null; source: string; tool: string; description: string;
  input: unknown; status: string; result?: string | null; error?: string | null; duration_ms?: number | null;
  conversation_id?: string | null; task_id?: string | null;
}

export interface TaskStep { id: string; idx: number; description: string; tool: string; input: unknown; status: string; result?: string | null; error?: string | null; attempts: number }
export interface Task { id: string; title: string; status: string; conversation_id?: string | null; current_step: number; error?: string | null; created_at: string; updated_at: string; steps: TaskStep[] }

export interface ToolInfo {
  name: string; description: string; input_schema: unknown; access: string;
  default_mode: PermissionMode; mode: PermissionMode; locked_ask: boolean; origin: string;
}

export interface ProviderStatus { provider: string; model: string; state: string; detail: string }
export interface ModelInfo { id: string; label: string }

export interface ClaudeCodeStatus {
  installed: boolean; path?: string | null; version?: string | null; logged_in: boolean;
  auth_method?: string | null; subscription_type?: string | null;
  billing: "subscription" | "api" | "unknown"; login_expires_at?: string | null; error?: string | null;
}

export interface McpToolStatus { name: string; exposed_name: string; description: string; input_schema: unknown; trusted: boolean }
export interface McpServerStatus { id: string; name: string; enabled: boolean; state: string; error?: string | null; server_name?: string | null; tools: McpToolStatus[] }

export interface FileRef { path: string; size: number; is_dir: boolean; included: boolean }
export interface CapturedContext {
  id: string; source: "selection" | "clipboard" | "explorer" | "browser" | "vscode" | "cli";
  kind: "text" | "code" | "files" | "page" | "diagnostics";
  app?: string | null; title?: string | null; url?: string | null; language?: string | null; file_path?: string | null;
  files: FileRef[]; content: string; action?: string | null; question?: string | null;
  captured_at: string; truncated: boolean; method?: string | null;
}

export interface AppError { kind: string; message: string }

export type StreamEvent =
  | { type: "text_delta"; text: string }
  | { type: "thinking_delta"; text: string }
  | { type: "tool_use_start"; id: string; name: string }
  | { type: "tool_input_delta"; id: string; partial_json: string };

export type ChatStreamPayload = { run_id: string; conversation_id: string } & (
  | { type: "turn_start"; iteration: number }
  | { type: "delta"; event: StreamEvent }
  | { type: "message"; message: StoredMessage }
  | { type: "tool_running"; tool_use_id: string; name: string }
  | { type: "tool_result"; tool_use_id: string; name: string; status: string; is_error: boolean; content: string }
  | { type: "context"; memories: string[] }
  | { type: "usage"; status: string; message: string; utilization?: number | null }
  | { type: "fallback"; reason: string; provider: string; model: string }
  | { type: "done" }
  | { type: "cancelled" }
  | { type: "error"; error: AppError }
);

export type View = "chat" | "settings" | "memory" | "activity";
