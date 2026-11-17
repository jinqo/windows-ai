import { useEffect, useMemo, useRef } from "react";
import { Sparkles } from "lucide-react";
import { useApp } from "../../store/app";
import { api, errorMessage } from "../../lib/api";
import { Composer } from "./Composer";
import { LiveMessage, MessageItem, isToolResultOnly } from "./Messages";
import { newRun } from "../../lib/stream";

const SUGGESTIONS = [
  { title: "Explain a selection", prompt: "", hint: "Select text anywhere, then press the selection shortcut" },
  { title: "System check", prompt: "How much memory and disk space do I have free?", hint: "Uses the system info tool" },
  { title: "Draft an email", prompt: "Draft a friendly email asking my team for their status updates by Friday.", hint: "Writing help" },
  { title: "Plan a task", prompt: "Help me plan the steps to organize my Downloads folder. Make a task plan I can review.", hint: "Creates a reviewable automation plan" },
];

function Welcome() {
  const config = useApp((s) => s.config);
  const send = useApp((s) => s.send);
  const provider = useApp((s) => s.provider);
  const sel = config?.integrations.selection_shortcut ?? "Ctrl+Shift+Space";
  return (
    <div className="welcome">
      <div className="orb"><Sparkles size={26} strokeWidth={2.2} /></div>
      <h1>What can I help with?</h1>
      <div className="muted">
        <span className="kbd">{config?.shortcuts.toggle_overlay ?? "Alt+Space"}</span> opens me anywhere ·{" "}
        <span className="kbd">{sel}</span> captures selected text · <span className="kbd">Esc</span> hides
      </div>
      {provider && provider.state !== "connected" && (
        <div className="notice warn small" style={{ marginTop: 16, textAlign: "left" }}>
          <div>
            <b>AI provider not ready.</b> {provider.detail}{" "}
            <a href="#" onClick={(e) => { e.preventDefault(); useApp.getState().setView("settings"); }}>Open settings</a>
          </div>
        </div>
      )}
      <div className="suggestions">
        {SUGGESTIONS.map((s) => (
          <div
            key={s.title}
            className="suggestion"
            role="button"
            tabIndex={0}
            onClick={() => (s.prompt ? send(s.prompt) : useApp.getState().toast("info", `Select text in any app and press ${sel}`))}
          >
            <b>{s.title}</b>
            <span className="muted small">{s.hint}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function StatusLine() {
  const provider = useApp((s) => s.provider);
  const config = useApp((s) => s.config);
  const run = useApp((s) => s.run);
  if (!provider || !config) return null;
  const label =
    provider.provider === "claude_code" ? "Claude subscription · Claude Code"
    : provider.provider === "anthropic" ? "Anthropic API (billed per use)"
    : "OpenAI-compatible";
  const cls = provider.state === "connected" ? "ok" : provider.state === "missing_credentials" || provider.state === "missing_cli" ? "warn" : "err";
  const runningTool = run && !run.done ? run.tools.find((t) => t.status === "running") : undefined;
  return (
    <div className="statusline">
      <span className={`badge ${cls}`} title={provider.detail}><span className="dot" /> {label}</span>
      <span>{provider.model || "default model"}</span>
      <span className="spacer" />
      {runningTool && <span className="badge accent"><span className="dot pulse" /> {runningTool.name.replace(/^mcp__windows_ai__/, "")}</span>}
    </div>
  );
}

export function ChatView() {
  const messages = useApp((s) => s.messages);
  const run = useApp((s) => s.run);
  const currentId = useApp((s) => s.currentId);
  const scroller = useRef<HTMLDivElement>(null);

  const results = useMemo(() => {
    const m = new Map<string, { content: string; is_error: boolean }>();
    for (const msg of messages) for (const b of msg.content) if (b.type === "tool_result") m.set(b.tool_use_id, { content: b.content, is_error: b.is_error });
    return m;
  }, [messages]);

  const visible = messages.filter((m) => !isToolResultOnly(m));
  const runHere = run && run.conversationId === currentId ? run : null;

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 160;
    if (nearBottom) el.scrollTop = el.scrollHeight;
  }, [visible.length, runHere?.text, runHere?.tools.length]);

  const regenerate = async (userMessageId: string) => {
    if (!currentId || (run && !run.done)) return;
    try {
      const runId = await api.regenerate(currentId, userMessageId);
      const msgs = await api.getMessages(currentId);
      useApp.setState({ messages: msgs, run: newRun(runId, currentId) });
    } catch (e) {
      useApp.getState().toast("error", errorMessage(e));
    }
  };

  const lastUserId = [...visible].reverse().find((m) => m.role === "user")?.id;
  const lastAssistantId = [...visible].reverse().find((m) => m.role === "assistant")?.id;

  return (
    <div className="chat">
      <div className="messages" ref={scroller}>
        {visible.length === 0 && !runHere ? (
          <Welcome />
        ) : (
          <>
            {visible.map((m) => (
              <MessageItem
                key={m.id}
                message={m}
                results={results}
                onRegenerate={m.id === lastAssistantId && lastUserId && (!run || run.done) ? () => regenerate(lastUserId) : undefined}
              />
            ))}
            {runHere && <LiveMessage run={runHere} />}
          </>
        )}
      </div>
      <Composer />
      <div style={{ padding: "0 18px 10px", marginTop: -6 }}>
        <StatusLine />
      </div>
    </div>
  );
}
