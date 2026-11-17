import { useEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useApp } from "../store/app";
import { api, errorMessage } from "../lib/api";
import { speak } from "../lib/voice";
import { parseUserMessage } from "../lib/context";
import type { AppConfig, ApprovalRequest, CapturedContext, ChatStreamPayload } from "../lib/types";

/** Subscribes the overlay to backend events for its lifetime. */
export function useBackendEvents() {
  useEffect(() => {
    const subs: Promise<UnlistenFn>[] = [];
    const st = () => useApp.getState();

    subs.push(
      listen<ChatStreamPayload>("chat-stream", (e) => {
        st().onStream(e.payload);
        // Read the final answer aloud when TTS is enabled.
        const cfg = st().config;
        if (e.payload.type === "message" && e.payload.message.role === "assistant" && cfg?.voice.tts_enabled) {
          const text = e.payload.message.content.map((b) => (b.type === "text" ? b.text : "")).join("");
          if (text.trim()) speak(parseUserMessage(text).text, { voice: cfg.voice.tts_voice, rate: cfg.voice.tts_rate, pitch: cfg.voice.tts_pitch, volume: cfg.voice.tts_volume });
        }
      }),
    );
    subs.push(listen<ApprovalRequest>("approval-request", (e) => st().addApproval(e.payload)));
    subs.push(listen<{ id: string }>("approval-resolved", (e) => st().removeApproval(e.payload.id)));
    subs.push(listen<CapturedContext>("context-captured", (e) => st().onCaptured(e.payload)));
    subs.push(listen<{ error: unknown }>("capture-error", (e) => st().toast("error", errorMessage(e.payload.error))));
    subs.push(listen<AppConfig>("config-changed", (e) => useApp.setState({ config: e.payload })));
    subs.push(listen<{ view: "settings" | "activity" | "chat" | "memory" }>("navigate", (e) => st().setView(e.payload.view)));
    subs.push(listen<{ errors: string[] }>("shortcut-error", (e) => e.payload.errors.forEach((x) => st().toast("error", `Shortcut unavailable: ${x}`))));
    subs.push(listen("memory-changed", () => window.dispatchEvent(new Event("wai-memory-changed"))));
    subs.push(listen("activity", () => window.dispatchEvent(new Event("wai-activity-changed"))));
    subs.push(listen("task-updated", () => window.dispatchEvent(new Event("wai-activity-changed"))));
    subs.push(
      listen<{ context_id: string; action: string }>("run-context-action", async (e) => {
        await st().loadInbox();
        const { context_id, action } = e.payload;
        if (action === "ask") {
          st().attach(context_id);
          st().setPendingAction(null);
          window.dispatchEvent(new Event("wai-focus-composer"));
        } else {
          st().newConversation();
          await st().send("", { action, contextIds: [context_id] });
        }
      }),
    );
    subs.push(
      listen("overlay-shown", () => {
        st().loadInbox();
        window.dispatchEvent(new Event("wai-focus-composer"));
      }),
    );
    // Re-sync approvals in case some arrived before listeners were attached.
    api.pendingApprovals().then((a) => a.forEach((x) => st().addApproval(x))).catch(() => {});

    return () => {
      subs.forEach((p) => p.then((un) => un()));
    };
  }, []);
}
