import { useEffect, useRef, useState } from "react";
import { ArrowUp, ClipboardPaste, Code2, FileText, Globe, Loader2, Mic, MousePointerSquareDashed, Square, X, ClipboardList } from "lucide-react";
import { useApp } from "../../store/app";
import { api, errorMessage } from "../../lib/api";
import { actionsFor, contextTitle, formatSize, SOURCE_LABEL, KIND_LABEL } from "../../lib/context";
import { useVoice } from "../../hooks/useVoice";
import type { CapturedContext } from "../../lib/types";

const ICON: Record<string, typeof Globe> = { browser: Globe, vscode: Code2, explorer: FileText, selection: MousePointerSquareDashed, clipboard: ClipboardList };

function ContextPreview({ c, onClose }: { c: CapturedContext; onClose: () => void }) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={{ width: "min(640px, 100%)" }}>
        <div className="row">
          <h3>Exactly what will be shared</h3>
          <span className="spacer" />
          <button className="icon-btn" onClick={onClose} aria-label="Close"><X size={16} /></button>
        </div>
        <div className="small muted" style={{ margin: "4px 0 8px" }}>
          {SOURCE_LABEL[c.source]} · {KIND_LABEL[c.kind]}
          {c.app ? ` · ${c.app}` : ""}{c.method ? ` · captured via ${c.method.replace("_", " ")}` : ""}
          {c.truncated ? " · truncated to the configured limit" : ""}
        </div>
        {c.title && <div className="small"><b>Title:</b> {c.title}</div>}
        {c.url && <div className="small ellipsis"><b>URL:</b> {c.url}</div>}
        {c.files.length > 0 && (
          <div className="small" style={{ marginTop: 6 }}>
            {c.files.map((f) => (
              <div key={f.path} className="ellipsis mono">{f.is_dir ? "📁" : f.included ? "📄" : "▫️"} {f.path}{!f.is_dir && !f.included ? " (name and size only)" : ""}</div>
            ))}
          </div>
        )}
        <pre style={{ maxHeight: 340 }}>{c.content || "(no text content)"}</pre>
        <div className="small muted">This content is sent to your configured AI provider only when you press Send.</div>
      </div>
    </div>
  );
}

export function Composer() {
  const draft = useApp((s) => s.draft);
  const setDraft = useApp((s) => s.setDraft);
  const inbox = useApp((s) => s.inbox);
  const attached = useApp((s) => s.attached);
  const pendingAction = useApp((s) => s.pendingAction);
  const run = useApp((s) => s.run);
  const config = useApp((s) => s.config);
  const { send, stop, detach, setPendingAction, toast } = useApp.getState();
  const ref = useRef<HTMLTextAreaElement>(null);
  const [preview, setPreview] = useState<CapturedContext | null>(null);
  const working = !!run && !run.done;
  const contexts = inbox.filter((c) => attached.includes(c.id));

  const voice = useVoice((text) => {
    const next = draft ? `${draft} ${text}` : text;
    if (config?.voice.auto_send_transcript) send(next);
    else setDraft(next);
    ref.current?.focus();
  });

  useEffect(() => {
    const focus = () => setTimeout(() => ref.current?.focus(), 30);
    window.addEventListener("wai-focus-composer", focus);
    focus();
    return () => window.removeEventListener("wai-focus-composer", focus);
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [draft]);

  // Hold F8 for push-to-talk while the overlay is focused.
  useEffect(() => {
    const down = (e: KeyboardEvent) => e.key === "F8" && !e.repeat && voice.start();
    const up = (e: KeyboardEvent) => e.key === "F8" && voice.stop();
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, [voice.start, voice.stop]);

  const canSend = !working && (draft.trim().length > 0 || contexts.length > 0);
  const submit = () => canSend && send(draft);
  const pressStart = useRef(0);

  return (
    <div className="composer-wrap">
      <div className="composer">
        {contexts.length > 0 && (
          <>
            <div className="chips">
              {contexts.map((c) => {
                const Icon = ICON[c.source] ?? FileText;
                return (
                  <span key={c.id} className="chip" title="Click to see exactly what will be shared" onClick={() => setPreview(c)}>
                    <Icon size={13} />
                    <span className="label"><b>{SOURCE_LABEL[c.source]}</b> · {contextTitle(c)}</span>
                    <span className="muted">{formatSize(c.content.length)}</span>
                    <button aria-label="Remove" onClick={(e) => { e.stopPropagation(); detach(c.id); }}><X size={12} /></button>
                  </span>
                );
              })}
            </div>
            <div className="action-row">
              {actionsFor(contexts[0].kind).map((a) => (
                <button key={a.id} className={`action-pill ${pendingAction === a.id ? "on" : ""}`} onClick={() => setPendingAction(pendingAction === a.id ? null : a.id)}>
                  {a.label}
                </button>
              ))}
            </div>
          </>
        )}
        <textarea
          ref={ref}
          rows={1}
          value={voice.listening ? voice.partial || draft : draft}
          placeholder={voice.listening ? "Listening…" : contexts.length ? (pendingAction ? "Add instructions (optional) and press Enter" : "Ask about the attached content…") : "Ask Windows AI anything…"}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
          }}
          aria-label="Message"
        />
        <div className="composer-bar">
          <button
            className="icon-btn"
            title="Attach clipboard text"
            onClick={() => api.captureClipboard().catch((e) => toast("error", errorMessage(e)))}
          >
            <ClipboardPaste size={16} />
          </button>
          {voice.enabled && (
            <button
              className={`icon-btn ${voice.listening ? "mic-on" : ""}`}
              title="Hold to talk (or hold F8). Click to toggle."
              aria-pressed={voice.listening}
              disabled={voice.busy}
              onPointerDown={() => {
                // Short click toggles; press-and-hold is push-to-talk.
                if (voice.listening) {
                  pressStart.current = 0;
                  voice.stop();
                } else {
                  pressStart.current = Date.now();
                  voice.start();
                }
              }}
              onPointerUp={() => {
                if (pressStart.current && Date.now() - pressStart.current > 350) voice.stop();
                pressStart.current = 0;
              }}
            >
              {voice.busy ? <Loader2 size={16} className="spin" /> : <Mic size={16} />}
            </button>
          )}
          <span className="spacer" />
          {working ? (
            <button className="send-btn" title="Stop" aria-label="Stop" onClick={stop} style={{ background: "var(--surface-3)" }}>
              <Square size={13} fill="currentColor" />
            </button>
          ) : (
            <button className="send-btn" title="Send (Enter)" aria-label="Send" disabled={!canSend} onClick={submit}>
              <ArrowUp size={17} strokeWidth={2.4} />
            </button>
          )}
        </div>
      </div>
      {preview && <ContextPreview c={preview} onClose={() => setPreview(null)} />}
    </div>
  );
}
