// Typed wrappers over Tauri commands. All backend access goes through here.
import { invoke } from "@tauri-apps/api/core";
import type {
  ActivityEntry, AppConfig, ApprovalRequest, CapturedContext, ClaudeCodeStatus, Conversation, McpServerStatus,
  Memory, ModelInfo, PermissionMode, Project, ProviderStatus, StoredMessage, Task, ToolInfo, AppError,
} from "./types";

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as AppError).message);
  return String(e);
}

export function errorKind(e: unknown): string | undefined {
  return e && typeof e === "object" && "kind" in e ? String((e as AppError).kind) : undefined;
}

export interface MemoryInput { kind: Memory["kind"]; content: string; tags?: string; project_id?: string | null; pinned?: boolean }
export interface ProjectInput { name: string; description?: string; root_path?: string | null; context?: string }

export const api = {
  // config
  getConfig: () => invoke<AppConfig>("get_config"),
  saveConfig: (config: AppConfig) => invoke<{ warnings: string[] }>("save_config", { config }),
  secretStatus: (name: string) => invoke<{ name: string; stored: boolean; from_env: boolean }>("secret_status", { name }),
  setSecret: (name: string, value: string) => invoke<void>("set_secret", { name, value }),
  deleteSecret: (name: string) => invoke<void>("delete_secret", { name }),
  appInfo: () => invoke<Record<string, unknown>>("app_info"),

  // chat
  listConversations: (search?: string) => invoke<Conversation[]>("list_conversations", { search: search || null }),
  createConversation: (title?: string) => invoke<Conversation>("create_conversation", { title: title ?? null, projectId: null }),
  getMessages: (conversationId: string) => invoke<StoredMessage[]>("get_messages", { conversationId }),
  renameConversation: (id: string, title: string) => invoke<void>("rename_conversation", { id, title }),
  pinConversation: (id: string, pinned: boolean) => invoke<void>("pin_conversation", { id, pinned }),
  setConversationProject: (id: string, projectId: string | null) => invoke<void>("set_conversation_project", { id, projectId }),
  deleteConversation: (id: string) => invoke<void>("delete_conversation", { id }),
  sendMessage: (args: { conversationId: string | null; text: string; contextIds: string[]; action: string | null }) =>
    invoke<{ conversation_id: string; run_id: string; user_message: StoredMessage }>("send_message", args),
  regenerate: (conversationId: string, fromMessageId: string) => invoke<string>("regenerate", { conversationId, fromMessageId }),
  cancelRun: (runId: string) => invoke<void>("cancel_run", { runId }),
  providerStatus: () => invoke<ProviderStatus>("provider_status"),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  claudeCodeStatus: () => invoke<ClaudeCodeStatus>("claude_code_status"),
  claudeCodeLogin: () => invoke<void>("claude_code_login"),

  // memory
  listMemories: (query?: string, kind?: string) => invoke<Memory[]>("list_memories", { query: query || null, kind: kind || null, projectId: null }),
  createMemory: (input: MemoryInput, allowSensitive = false) => invoke<Memory>("create_memory", { input, allowSensitive }),
  updateMemory: (id: string, input: MemoryInput, allowSensitive = false) => invoke<Memory>("update_memory", { id, input, allowSensitive }),
  deleteMemory: (id: string) => invoke<void>("delete_memory", { id }),
  detectSensitive: (text: string) => invoke<string[]>("detect_sensitive", { text }),
  listProjects: () => invoke<Project[]>("list_projects"),
  createProject: (input: ProjectInput) => invoke<Project>("create_project", { input }),
  updateProject: (id: string, input: ProjectInput) => invoke<Project>("update_project", { id, input }),
  deleteProject: (id: string) => invoke<void>("delete_project", { id }),

  // tools, permissions, activity, tasks
  listTools: () => invoke<ToolInfo[]>("list_tools"),
  setToolMode: (tool: string, mode: PermissionMode) => invoke<void>("set_tool_mode", { tool, mode }),
  resolveApproval: (id: string, decision: "approve" | "approve_session" | "deny") => invoke<void>("resolve_approval", { id, decision }),
  pendingApprovals: () => invoke<ApprovalRequest[]>("pending_approvals"),
  clearSessionApprovals: () => invoke<void>("clear_session_approvals"),
  listActivity: (status?: string) => invoke<ActivityEntry[]>("list_activity", { status: status || null, limit: 300 }),
  clearActivity: () => invoke<void>("clear_activity"),
  listTasks: () => invoke<Task[]>("list_tasks"),
  runTask: (id: string) => invoke<void>("run_task", { id }),
  cancelTask: (id: string) => invoke<void>("cancel_task", { id }),
  skipTaskStep: (id: string) => invoke<void>("skip_task_step", { id }),
  deleteTask: (id: string) => invoke<void>("delete_task", { id }),

  // MCP
  mcpStatuses: () => invoke<McpServerStatus[]>("mcp_statuses"),
  mcpConnect: (id: string) => invoke<McpServerStatus>("mcp_connect", { id }),
  mcpDisconnect: (id: string) => invoke<McpServerStatus>("mcp_disconnect", { id }),
  mcpSetToolTrust: (serverId: string, tool: string, trusted: boolean) => invoke<McpServerStatus>("mcp_set_tool_trust", { serverId, tool, trusted }),

  // voice
  voiceStart: () => invoke<void>("voice_start"),
  voiceStop: () => invoke<string>("voice_stop"),
  voiceCancel: () => invoke<void>("voice_cancel"),
  voiceTranscribe: (audioBase64: string, mime: string) => invoke<string>("voice_transcribe", { audioBase64, mime }),

  // integrations
  inboxList: () => invoke<CapturedContext[]>("inbox_list"),
  inboxRemove: (id: string) => invoke<void>("inbox_remove", { id }),
  inboxClear: () => invoke<void>("inbox_clear"),
  captureClipboard: () => invoke<CapturedContext>("capture_clipboard"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  integrationStatus: () => invoke<Record<string, any>>("integration_status"),
  setExplorerMenu: (enabled: boolean) => invoke<Record<string, any>>("set_explorer_menu", { enabled }),
  setBrowserHost: (enabled: boolean, extraExtensionIds: string[] = []) => invoke<Record<string, any>>("set_browser_host", { enabled, extraExtensionIds }),

  // windows
  showOverlay: () => invoke<void>("show_overlay"),
  hideOverlay: () => invoke<void>("hide_overlay"),
  hideToolbar: () => invoke<void>("hide_toolbar"),
  toolbarAction: (contextId: string, action: string) => invoke<void>("toolbar_action", { contextId, action }),
};
