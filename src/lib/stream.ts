// Pure reducer for live streaming state of one run.
import type { ChatStreamPayload } from "./types";

export interface LiveTool { id: string; name: string; status: "running" | "success" | "error" | "denied" | "cancelled"; content?: string }

export interface LiveRun {
  runId: string;
  conversationId: string;
  text: string;
  thinking: string;
  tools: LiveTool[];
  usageNotice?: string;
  memories?: string[];
  done: boolean;
  error?: string;
  errorKind?: string;
}

export function newRun(runId: string, conversationId: string): LiveRun {
  return { runId, conversationId, text: "", thinking: "", tools: [], done: false };
}

function upsertTool(tools: LiveTool[], t: LiveTool): LiveTool[] {
  const i = tools.findIndex((x) => x.id === t.id);
  if (i < 0) return [...tools, t];
  const copy = tools.slice();
  copy[i] = { ...copy[i], ...t, name: t.name || copy[i].name };
  return copy;
}

/** Applies one `chat-stream` event. Events for other runs are ignored. */
export function applyStream(run: LiveRun, p: ChatStreamPayload): LiveRun {
  if (p.run_id !== run.runId) return run;
  switch (p.type) {
    case "turn_start":
      return { ...run, text: "", thinking: "" };
    case "delta": {
      const e = p.event;
      if (e.type === "text_delta") return { ...run, text: run.text + e.text };
      if (e.type === "thinking_delta") return { ...run, thinking: run.thinking + e.text };
      if (e.type === "tool_use_start") return { ...run, tools: upsertTool(run.tools, { id: e.id, name: e.name, status: "running" }) };
      return run;
    }
    case "tool_running":
      return { ...run, tools: upsertTool(run.tools, { id: p.tool_use_id, name: p.name, status: "running" }) };
    case "tool_result": {
      const status = (["success", "error", "denied", "cancelled"].includes(p.status) ? p.status : p.is_error ? "error" : "success") as LiveTool["status"];
      return { ...run, tools: upsertTool(run.tools, { id: p.tool_use_id, name: p.name, status, content: p.content }) };
    }
    case "message":
      // A persisted message replaces the streamed buffer.
      return p.message.role === "assistant" ? { ...run, text: "", thinking: "" } : run;
    case "context":
      return { ...run, memories: p.memories };
    case "usage":
      return { ...run, usageNotice: p.message };
    case "done":
      return { ...run, done: true };
    case "cancelled":
      return { ...run, done: true, error: "Stopped." , errorKind: "cancelled" };
    case "error":
      return { ...run, done: true, error: p.error.message, errorKind: p.error.kind };
    default:
      return run;
  }
}
