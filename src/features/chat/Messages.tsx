import { CheckCircle2, ChevronRight, CircleDashed, FileText, Globe, Code2, ClipboardList, MousePointerSquareDashed, RotateCcw, Sparkles, Volume2, Wrench, XCircle } from "lucide-react";
import { Markdown, CopyButton } from "./Markdown";
import type { ContentBlock, StoredMessage } from "../../lib/types";
import type { LiveRun, LiveTool } from "../../lib/stream";
import { parseUserMessage, SOURCE_LABEL, KIND_LABEL, formatSize, type ParsedContextBlock } from "../../lib/context";
import { speak } from "../../lib/voice";
import { useApp } from "../../store/app";

const SOURCE_ICON: Record<string, typeof Globe> = {
  browser: Globe, vscode: Code2, explorer: FileText, selection: MousePointerSquareDashed, clipboard: ClipboardList,
};

export function ContextCard({ block }: { block: ParsedContextBlock }) {
  const a = block.attrs;
  const Icon = SOURCE_ICON[a.source] ?? FileText;
  return (
    <details className="ctx-card">
      <summary>
        <Icon size={14} style={{ color: "var(--accent-2)", flex: "none" }} />
        <b style={{ fontWeight: 600 }}>{SOURCE_LABEL[a.source] ?? a.source}</b>
        <span className="muted ellipsis">{a.title || a.file || a.app || KIND_LABEL[a.kind]}</span>
        <span className="spacer" />
        <span className="muted">{formatSize(block.body.length)}{a.truncated ? " · truncated" : ""}</span>
        <ChevronRight size={13} className="muted" />
      </summary>
      {a.url && <div className="small muted" style={{ padding: "0 12px 6px" }}>{a.url}</div>}
      <pre>{block.body}</pre>
    </details>
  );
}

function toolName(n: string) {
  return n.replace(/^mcp__windows_ai__/, "").replace(/^mcp__([^_]+)__/, "$1 · ");
}

export function ToolCard({ name, status, content, input }: { name: string; status: string; content?: string; input?: unknown }) {
  const icon =
    status === "running" ? <CircleDashed size={14} className="dot pulse" style={{ color: "var(--accent-2)", background: "none" }} />
    : status === "success" ? <CheckCircle2 size={14} style={{ color: "var(--ok)" }} />
    : <XCircle size={14} style={{ color: status === "denied" ? "var(--warn)" : "var(--err)" }} />;
  const label = status === "running" ? "Running" : status === "success" ? "Done" : status === "denied" ? "Denied" : status === "cancelled" ? "Cancelled" : "Failed";
  return (
    <details className="tool-card">
      <summary>
        <Wrench size={13} className="muted" />
        <span className="mono">{toolName(name)}</span>
        <span className="spacer" />
        {icon}
        <span className="muted">{label}</span>
      </summary>
      {input !== undefined && <pre>{JSON.stringify(input, null, 2)}</pre>}
      {content && <pre>{content}</pre>}
    </details>
  );
}

function textBlocks(blocks: ContentBlock[]) {
  return blocks.filter((b): b is Extract<ContentBlock, { type: "text" }> => b.type === "text").map((b) => b.text).join("");
}

export function isToolResultOnly(m: StoredMessage) {
  return m.role === "user" && m.content.length > 0 && m.content.every((b) => b.type === "tool_result");
}

export function MessageItem({ message, results, onRegenerate }: { message: StoredMessage; results: Map<string, { content: string; is_error: boolean }>; onRegenerate?: () => void }) {
  const config = useApp((s) => s.config);
  if (message.role === "user") {
    const parsed = parseUserMessage(textBlocks(message.content));
    return (
      <div className="msg user">
        <div style={{ maxWidth: "85%", display: "flex", flexDirection: "column", alignItems: "flex-end" }}>
          {parsed.contexts.map((c, i) => <ContextCard key={i} block={c} />)}
          {parsed.text && <div className="bubble">{parsed.text}</div>}
        </div>
      </div>
    );
  }
  const text = textBlocks(message.content);
  const thinking = message.content.filter((b) => b.type === "thinking").map((b) => (b as { thinking: string }).thinking).join("\n");
  const tools = message.content.filter((b): b is Extract<ContentBlock, { type: "tool_use" }> => b.type === "tool_use");
  return (
    <div className="msg assistant">
      <div className="avatar"><Sparkles size={14} strokeWidth={2.4} /></div>
      <div className="bubble">
        {thinking.trim() && (
          <details className="thinking">
            <summary><ChevronRight size={12} /> Reasoning</summary>
            <div className="body-text">{thinking}</div>
          </details>
        )}
        {tools.map((t) => {
          const r = results.get(t.id);
          const status = !r ? "running" : r.is_error ? (r.content.includes("denied") ? "denied" : "error") : "success";
          return <ToolCard key={t.id} name={t.name} status={status} content={r?.content} input={t.input} />;
        })}
        {text && <Markdown text={text} />}
        {text && (
          <div className="msg-meta">
            <CopyButton text={text} label="Copy response" />
            {config && (
              <button className="icon-btn" style={{ width: 28, height: 28 }} title="Read aloud" onClick={() => speak(text, { voice: config.voice.tts_voice, rate: config.voice.tts_rate, pitch: config.voice.tts_pitch, volume: config.voice.tts_volume })}>
                <Volume2 size={14} />
              </button>
            )}
            {onRegenerate && (
              <button className="icon-btn" style={{ width: 28, height: 28 }} title="Regenerate" onClick={onRegenerate}>
                <RotateCcw size={14} />
              </button>
            )}
            {message.model && <span className="muted small">{message.model}</span>}
          </div>
        )}
      </div>
    </div>
  );
}

export function LiveMessage({ run }: { run: LiveRun }) {
  const working = !run.done;
  const hasContent = run.text || run.thinking || run.tools.length;
  if (!working && !run.error && !run.usageNotice) return null;
  return (
    <div className="msg assistant">
      <div className="avatar"><Sparkles size={14} strokeWidth={2.4} /></div>
      <div className="bubble">
        {run.usageNotice && <div className="notice warn small" style={{ marginBottom: 8 }}>{run.usageNotice}</div>}
        {run.thinking && (
          <details className="thinking" open={!run.text}>
            <summary><ChevronRight size={12} /> {working && !run.text ? "Thinking…" : "Reasoning"}</summary>
            <div className="body-text">{run.thinking}</div>
          </details>
        )}
        {run.tools.map((t: LiveTool) => <ToolCard key={t.id} name={t.name} status={t.status} content={t.content} />)}
        {run.text ? <Markdown text={run.text} streaming={working} /> : working && !hasContent ? <div className="muted cursor-blink">Thinking</div> : null}
        {run.error && run.errorKind !== "cancelled" && <div className="notice err small" style={{ marginTop: 8 }}>{run.error}</div>}
        {run.errorKind === "cancelled" && <div className="muted small" style={{ marginTop: 6 }}>Stopped.</div>}
      </div>
    </div>
  );
}
