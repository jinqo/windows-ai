import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { MessageCircleQuestion, Sparkles, X } from "lucide-react";
import { api, errorMessage } from "../../lib/api";
import { actionsFor, SOURCE_LABEL } from "../../lib/context";
import type { CapturedContext } from "../../lib/types";
import "../../styles/global.css";

/**
 * Floating selection toolbar. Shown near the cursor after the selection
 * shortcut; it never appears on its own. Esc or clicking away dismisses it.
 */
export default function ToolbarWindow() {
  const [ctx, setCtx] = useState<CapturedContext | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const items = await api.inboxList().catch(() => []);
    const latest = items.filter((c) => c.source === "selection").pop() ?? items[items.length - 1] ?? null;
    setCtx(latest);
  }, []);

  useEffect(() => {
    document.body.style.background = "transparent";
    const subs = [
      listen<CapturedContext>("context-captured", (e) => {
        setError(null);
        setCtx(e.payload);
      }),
      listen<{ error: unknown }>("capture-error", (e) => {
        setCtx(null);
        setError(errorMessage(e.payload.error));
      }),
      listen("toolbar-shown", () => refresh()),
    ];
    refresh();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && api.hideToolbar();
    window.addEventListener("keydown", onKey);
    return () => {
      subs.forEach((p) => p.then((u) => u()));
      window.removeEventListener("keydown", onKey);
    };
  }, [refresh]);

  const run = (action: string) => ctx && api.toolbarAction(ctx.id, action);

  return (
    <div className="toolbar-shell" role="toolbar" aria-label="Windows AI actions">
      <Sparkles size={15} style={{ color: "var(--accent-2)", flex: "none" }} />
      {error ? (
        <span className="small" style={{ color: "var(--warn)", flex: 1 }}>{error}</span>
      ) : ctx ? (
        <>
          <button className="tb-btn ask" onClick={() => run("ask")}><MessageCircleQuestion size={14} /> Ask AI</button>
          {actionsFor(ctx.kind).slice(0, 5).map((a) => (
            <button key={a.id} className="tb-btn" onClick={() => run(a.id)}>{a.label}</button>
          ))}
          <span className="spacer" />
          <span className="src" title={`${SOURCE_LABEL[ctx.source]} · ${ctx.content.length} chars`}>{ctx.app ?? SOURCE_LABEL[ctx.source]}</span>
        </>
      ) : (
        <span className="small muted" style={{ flex: 1 }}>Capturing selection…</span>
      )}
      <button className="icon-btn" style={{ width: 28, height: 28 }} aria-label="Dismiss" onClick={() => api.hideToolbar()}><X size={14} /></button>
    </div>
  );
}
