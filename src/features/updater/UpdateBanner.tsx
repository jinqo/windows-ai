import { useEffect } from "react";
import { create } from "zustand";
import { Download, X } from "lucide-react";
import type { Update } from "@tauri-apps/plugin-updater";

type Phase = "idle" | "checking" | "available" | "none" | "downloading" | "error";

interface UpdaterState {
  phase: Phase;
  update: Update | null;
  progress: number;
  error: string;
  dismissed: boolean;
  check: (manual?: boolean) => Promise<void>;
  install: () => Promise<void>;
  dismiss: () => void;
}

/** Signed updates from GitHub Releases (latest.json); install runs the NSIS installer in passive mode. */
export const useUpdater = create<UpdaterState>((set, get) => ({
  phase: "idle",
  update: null,
  progress: 0,
  error: "",
  dismissed: false,
  check: async (manual = false) => {
    if (get().phase === "checking" || get().phase === "downloading") return;
    set({ phase: "checking", error: "", dismissed: manual ? false : get().dismissed });
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      set({ update, phase: update ? "available" : "none" });
    } catch (e) {
      set({ phase: manual ? "error" : "idle", error: String(e) });
    }
  },
  install: async () => {
    const u = get().update;
    if (!u) return;
    let total = 0;
    let done = 0;
    set({ phase: "downloading", progress: 0 });
    try {
      await u.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        if (ev.event === "Progress") {
          done += ev.data.chunkLength;
          set({ progress: total ? Math.round((done / total) * 100) : 0 });
        }
      });
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (e) {
      set({ phase: "error", error: String(e) });
    }
  },
  dismiss: () => set({ dismissed: true }),
}));

const SIX_HOURS = 6 * 60 * 60 * 1000;

export function UpdateBanner() {
  const { phase, update, progress, error, dismissed, check, install, dismiss } = useUpdater();
  useEffect(() => {
    const first = setTimeout(() => check(), 8000);
    const every = setInterval(() => check(), SIX_HOURS);
    return () => { clearTimeout(first); clearInterval(every); };
  }, [check]);

  if (dismissed || !update || !["available", "downloading", "error"].includes(phase)) return null;
  return (
    <div className="update-banner" role="status">
      <Download size={14} />
      <span>
        {phase === "downloading" ? `Downloading update ${update.version}… ${progress}%` : phase === "error" ? `Update failed: ${error}` : `Windows AI ${update.version} is available.`}
      </span>
      {phase === "available" && <button className="btn primary small" onClick={install}>Install &amp; restart</button>}
      {phase !== "downloading" && <button className="icon-btn" aria-label="Dismiss" onClick={dismiss}><X size={14} /></button>}
    </div>
  );
}
