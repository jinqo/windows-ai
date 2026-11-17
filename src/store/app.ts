// Global UI state (zustand). Backend data is fetched through `api`.
import { create } from "zustand";
import { api, errorKind, errorMessage } from "../lib/api";
import { applyStream, newRun, type LiveRun } from "../lib/stream";
import type { AppConfig, ApprovalRequest, CapturedContext, ChatStreamPayload, Conversation, ProviderStatus, StoredMessage, View } from "../lib/types";

export interface Toast { id: number; kind: "info" | "error" | "success"; text: string }

interface State {
  view: View;
  config: AppConfig | null;
  provider: ProviderStatus | null;
  conversations: Conversation[];
  search: string;
  currentId: string | null;
  messages: StoredMessage[];
  run: LiveRun | null;
  approvals: ApprovalRequest[];
  inbox: CapturedContext[];
  attached: string[]; // context ids attached to the composer
  pendingAction: string | null;
  draft: string;
  sidebarOpen: boolean;
  toasts: Toast[];

  setView: (v: View) => void;
  init: () => Promise<void>;
  loadConfig: () => Promise<void>;
  saveConfig: (c: AppConfig) => Promise<boolean>;
  refreshProvider: () => Promise<void>;
  loadConversations: () => Promise<void>;
  setSearch: (s: string) => void;
  openConversation: (id: string | null) => Promise<void>;
  newConversation: () => void;
  send: (text: string, opts?: { action?: string | null; contextIds?: string[] }) => Promise<void>;
  stop: () => Promise<void>;
  onStream: (p: ChatStreamPayload) => void;
  addApproval: (a: ApprovalRequest) => void;
  removeApproval: (id: string) => void;
  loadInbox: () => Promise<void>;
  onCaptured: (c: CapturedContext) => void;
  attach: (id: string) => void;
  detach: (id: string) => void;
  setPendingAction: (a: string | null) => void;
  setDraft: (d: string) => void;
  toggleSidebar: () => void;
  toast: (kind: Toast["kind"], text: string) => void;
  dismissToast: (id: number) => void;
}

let toastId = 0;

export const useApp = create<State>((set, get) => ({
  view: "chat",
  config: null,
  provider: null,
  conversations: [],
  search: "",
  currentId: null,
  messages: [],
  run: null,
  approvals: [],
  inbox: [],
  attached: [],
  pendingAction: null,
  draft: "",
  sidebarOpen: false,
  toasts: [],

  setView: (view) => set({ view }),

  init: async () => {
    await Promise.all([get().loadConfig(), get().loadConversations(), get().loadInbox()]);
    api.pendingApprovals().then((approvals) => set({ approvals })).catch(() => {});
    get().refreshProvider();
  },

  loadConfig: async () => {
    try {
      set({ config: await api.getConfig() });
    } catch (e) {
      get().toast("error", `Could not load settings: ${errorMessage(e)}`);
    }
  },

  saveConfig: async (c) => {
    try {
      const r = await api.saveConfig(c);
      set({ config: c });
      r.warnings.forEach((w) => get().toast("error", w));
      get().toast("success", "Settings saved");
      get().refreshProvider();
      return true;
    } catch (e) {
      get().toast("error", errorMessage(e));
      return false;
    }
  },

  refreshProvider: async () => {
    try {
      set({ provider: await api.providerStatus() });
    } catch (e) {
      set({ provider: { provider: get().config?.ai.provider ?? "?", model: "", state: "error", detail: errorMessage(e) } });
    }
  },

  loadConversations: async () => {
    try {
      set({ conversations: await api.listConversations(get().search) });
    } catch (e) {
      get().toast("error", errorMessage(e));
    }
  },

  setSearch: (search) => {
    set({ search });
    get().loadConversations();
  },

  openConversation: async (id) => {
    set({ currentId: id, messages: [], view: "chat", sidebarOpen: false });
    if (!id) return;
    try {
      const messages = await api.getMessages(id);
      if (get().currentId === id) set({ messages });
    } catch (e) {
      get().toast("error", errorMessage(e));
    }
  },

  newConversation: () => set({ currentId: null, messages: [], run: null, view: "chat", sidebarOpen: false }),

  send: async (text, opts) => {
    const s = get();
    if (s.run && !s.run.done) return;
    const contextIds = opts?.contextIds ?? s.attached;
    const action = opts?.action ?? s.pendingAction;
    if (!text.trim() && contextIds.length === 0) return;
    try {
      const r = await api.sendMessage({ conversationId: s.currentId, text, contextIds, action });
      set((st) => ({
        currentId: r.conversation_id,
        messages: st.currentId === r.conversation_id ? [...st.messages, r.user_message] : [r.user_message],
        run: newRun(r.run_id, r.conversation_id),
        attached: [],
        pendingAction: null,
        draft: "",
        inbox: st.inbox.filter((c) => !contextIds.includes(c.id)),
      }));
      get().loadConversations();
    } catch (e) {
      get().toast("error", errorMessage(e));
    }
  },

  stop: async () => {
    const run = get().run;
    if (run && !run.done) await api.cancelRun(run.runId).catch(() => {});
  },

  onStream: (p) => {
    const s = get();
    if (p.type === "message" && p.conversation_id === s.currentId) {
      if (!s.messages.some((m) => m.id === p.message.id)) set({ messages: [...s.messages, p.message] });
    }
    if (s.run) {
      const run = applyStream(s.run, p);
      set({ run });
      if (p.type === "done" || p.type === "error" || p.type === "cancelled") {
        get().loadConversations();
        if (p.type === "error") {
          const kind = errorKind(p.error) ?? p.error.kind;
          if (kind === "missing_credential" || kind === "not_found") get().refreshProvider();
        }
      }
    }
  },

  addApproval: (a) => set((s) => ({ approvals: s.approvals.some((x) => x.id === a.id) ? s.approvals : [...s.approvals, a] })),
  removeApproval: (id) => set((s) => ({ approvals: s.approvals.filter((a) => a.id !== id) })),

  loadInbox: async () => {
    try {
      const inbox = await api.inboxList();
      set((s) => ({ inbox, attached: s.attached.filter((id) => inbox.some((c) => c.id === id)) }));
    } catch {
      /* not fatal */
    }
  },

  onCaptured: (c) =>
    set((s) => ({
      inbox: [...s.inbox.filter((x) => x.id !== c.id), c],
      attached: s.attached.includes(c.id) ? s.attached : [...s.attached, c.id],
      pendingAction: c.action ?? s.pendingAction,
      view: "chat",
    })),

  attach: (id) => set((s) => ({ attached: s.attached.includes(id) ? s.attached : [...s.attached, id] })),
  detach: (id) => {
    api.inboxRemove(id).catch(() => {});
    set((s) => ({ attached: s.attached.filter((x) => x !== id), inbox: s.inbox.filter((c) => c.id !== id) }));
  },
  setPendingAction: (pendingAction) => set({ pendingAction }),
  setDraft: (draft) => set({ draft }),
  toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),

  toast: (kind, text) => {
    const id = ++toastId;
    set((s) => ({ toasts: [...s.toasts.slice(-3), { id, kind, text }] }));
    setTimeout(() => get().dismissToast(id), kind === "error" ? 7000 : 3500);
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));
