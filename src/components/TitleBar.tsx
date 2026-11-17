import { getCurrentWindow } from "@tauri-apps/api/window";
import { Activity, Brain, MessageSquare, PanelLeft, Settings, Sparkles, SquarePen, X } from "lucide-react";
import { useApp } from "../store/app";
import { api } from "../lib/api";
import type { View } from "../lib/types";

const NAV: { view: View; icon: typeof MessageSquare; label: string }[] = [
  { view: "chat", icon: MessageSquare, label: "Chat" },
  { view: "memory", icon: Brain, label: "Memory" },
  { view: "activity", icon: Activity, label: "Activity" },
  { view: "settings", icon: Settings, label: "Settings" },
];

export function TitleBar() {
  const view = useApp((s) => s.view);
  const setView = useApp((s) => s.setView);
  const toggleSidebar = useApp((s) => s.toggleSidebar);
  const newConversation = useApp((s) => s.newConversation);
  const run = useApp((s) => s.run);
  const approvals = useApp((s) => s.approvals.length);

  const startDrag = (e: React.MouseEvent) => {
    if (e.button === 0) getCurrentWindow().startDragging().catch(() => {});
  };

  return (
    <header className="titlebar" onDoubleClick={(e) => e.preventDefault()}>
      <button className="icon-btn" title="Conversations" aria-label="Conversations" onClick={toggleSidebar}>
        <PanelLeft size={17} />
      </button>
      <div className="brand" onMouseDown={startDrag}>
        <div className="brand-mark">
          <Sparkles size={13} strokeWidth={2.5} />
        </div>
        Windows AI
        {run && !run.done && <span className="badge accent"><span className="dot pulse" /> Working</span>}
        {approvals > 0 && <span className="badge warn">{approvals} awaiting approval</span>}
      </div>
      <div className="drag" onMouseDown={startDrag} data-tauri-drag-region />
      <nav className="nav">
        <button className="icon-btn" title="New chat (Ctrl+N)" aria-label="New chat" onClick={newConversation}>
          <SquarePen size={16} />
        </button>
        {NAV.map(({ view: v, icon: Icon, label }) => (
          <button key={v} className={`icon-btn ${view === v ? "active" : ""}`} title={label} aria-label={label} onClick={() => setView(v)}>
            <Icon size={16} />
          </button>
        ))}
      </nav>
      <button className="icon-btn" title="Hide (Esc)" aria-label="Hide to tray" onClick={() => api.hideOverlay()}>
        <X size={17} />
      </button>
    </header>
  );
}
