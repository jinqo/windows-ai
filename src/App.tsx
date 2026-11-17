import { useEffect } from "react";
import { TitleBar } from "./components/TitleBar";
import { Sidebar } from "./components/Sidebar";
import { ApprovalDialog } from "./components/ApprovalDialog";
import { Toasts } from "./components/Toasts";
import { ChatView } from "./features/chat/ChatView";
import { SettingsView } from "./features/settings/SettingsView";
import { MemoryView } from "./features/memory/MemoryView";
import { ActivityView } from "./features/activity/ActivityView";
import { useBackendEvents } from "./hooks/useBackendEvents";
import { useApp } from "./store/app";
import { api } from "./lib/api";

export default function App() {
  const view = useApp((s) => s.view);
  const config = useApp((s) => s.config);
  useBackendEvents();

  useEffect(() => {
    useApp.getState().init();
  }, []);

  // Apply appearance settings as CSS variables.
  useEffect(() => {
    if (!config) return;
    const root = document.documentElement;
    root.dataset.accent = config.appearance.accent;
    root.dataset.motion = config.appearance.reduce_motion ? "reduced" : "full";
    root.style.setProperty("--overlay-opacity", String(config.appearance.opacity));
    root.style.setProperty("--fs", `${config.appearance.font_size}px`);
  }, [config]);

  // Escape hides the overlay (unless a dialog or the sidebar handles it first).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      const s = useApp.getState();
      if (s.approvals.length) return;
      if (s.sidebarOpen) return s.toggleSidebar();
      if (s.view !== "chat") return s.setView("chat");
      api.hideOverlay();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="shell">
      <TitleBar />
      <div className="body">
        <Sidebar />
        {view === "chat" && <ChatView />}
        {view === "settings" && <SettingsView />}
        {view === "memory" && <MemoryView />}
        {view === "activity" && <ActivityView />}
        <ApprovalDialog />
        <Toasts />
      </div>
    </div>
  );
}
