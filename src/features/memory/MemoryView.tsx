import { useEffect, useState } from "react";
import { FolderKanban, Pencil, Pin, Plus, Search, Trash2 } from "lucide-react";
import { api, errorKind, errorMessage, type MemoryInput } from "../../lib/api";
import { useApp } from "../../store/app";
import { relativeTime } from "../../lib/format";
import type { Memory, Project } from "../../lib/types";

const KINDS: Memory["kind"][] = ["preference", "fact", "project", "note"];

function MemoryEditor({ initial, onDone }: { initial?: Memory; onDone: () => void }) {
  const toast = useApp((s) => s.toast);
  const [form, setForm] = useState<MemoryInput>({ kind: initial?.kind ?? "fact", content: initial?.content ?? "", tags: initial?.tags ?? "", pinned: initial?.pinned ?? false });
  const [warn, setWarn] = useState<string[] | null>(null);
  const save = async (allowSensitive = false) => {
    try {
      if (initial) await api.updateMemory(initial.id, form, allowSensitive);
      else await api.createMemory(form, allowSensitive);
      onDone();
    } catch (e) {
      if (errorKind(e) === "permission_denied") setWarn(await api.detectSensitive(form.content));
      else toast("error", errorMessage(e));
    }
  };
  return (
    <div className="memory-item" style={{ borderColor: "rgba(139,92,246,0.4)" }}>
      <div className="row" style={{ marginBottom: 8 }}>
        <select className="select" style={{ width: 140 }} value={form.kind} onChange={(e) => setForm({ ...form, kind: e.target.value as Memory["kind"] })}>
          {KINDS.map((k) => <option key={k} value={k}>{k}</option>)}
        </select>
        <input className="input" placeholder="tags (optional)" value={form.tags} onChange={(e) => setForm({ ...form, tags: e.target.value })} />
        <label className="row small" style={{ whiteSpace: "nowrap" }}><input type="checkbox" checked={!!form.pinned} onChange={(e) => setForm({ ...form, pinned: e.target.checked })} /> Always include</label>
      </div>
      <textarea className="textarea" autoFocus placeholder="What should Windows AI remember?" value={form.content} onChange={(e) => { setForm({ ...form, content: e.target.value }); setWarn(null); }} />
      {warn && (
        <div className="notice warn small" style={{ marginTop: 8 }}>
          <div>This looks like it contains sensitive data ({warn.join(", ")}). Memories are stored unencrypted in the local database. Save anyway?
            <div className="row" style={{ marginTop: 6 }}><button className="btn sm danger" onClick={() => save(true)}>Save anyway</button><button className="btn sm" onClick={() => setWarn(null)}>Edit</button></div>
          </div>
        </div>
      )}
      <div className="row" style={{ marginTop: 8, justifyContent: "flex-end" }}>
        <button className="btn ghost" onClick={onDone}>Cancel</button>
        <button className="btn primary" disabled={!form.content.trim()} onClick={() => save(false)}>Save</button>
      </div>
    </div>
  );
}

function Projects() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [editing, setEditing] = useState<Project | "new" | null>(null);
  const [form, setForm] = useState({ name: "", description: "", context: "" });
  const toast = useApp((s) => s.toast);
  const load = () => api.listProjects().then(setProjects).catch(() => {});
  useEffect(() => {
    load();
  }, []);
  const save = async () => {
    try {
      if (editing === "new") await api.createProject(form);
      else if (editing) await api.updateProject(editing.id, form);
      setEditing(null);
      load();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };
  return (
    <>
      <div className="row" style={{ margin: "22px 0 8px" }}>
        <h2 style={{ margin: 0 }}>Projects</h2>
        <span className="muted small">Context added to conversations linked to a project</span>
        <span className="spacer" />
        <button className="btn sm" onClick={() => { setEditing("new"); setForm({ name: "", description: "", context: "" }); }}><Plus size={13} /> Project</button>
      </div>
      {editing && (
        <div className="memory-item">
          <input className="input" placeholder="Project name" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} style={{ marginBottom: 6 }} />
          <input className="input" placeholder="Short description" value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} style={{ marginBottom: 6 }} />
          <textarea className="textarea" placeholder="Context the assistant should know (stack, goals, conventions…)" value={form.context} onChange={(e) => setForm({ ...form, context: e.target.value })} />
          <div className="row" style={{ justifyContent: "flex-end", marginTop: 8 }}><button className="btn ghost" onClick={() => setEditing(null)}>Cancel</button><button className="btn primary" disabled={!form.name.trim()} onClick={save}>Save</button></div>
        </div>
      )}
      {projects.map((p) => (
        <div key={p.id} className="memory-item row">
          <FolderKanban size={16} className="muted" />
          <div className="grow" style={{ flex: 1, minWidth: 0 }}><b>{p.name}</b> <span className="muted small">{p.description}</span><div className="small muted ellipsis">{p.context}</div></div>
          <button className="icon-btn" onClick={() => { setEditing(p); setForm({ name: p.name, description: p.description, context: p.context }); }}><Pencil size={14} /></button>
          <button className="icon-btn danger" onClick={() => api.deleteProject(p.id).then(load)}><Trash2 size={14} /></button>
        </div>
      ))}
      {useApp.getState().currentId && projects.length > 0 && (
        <div className="row small" style={{ marginTop: 6 }}>
          <span className="muted">Link current conversation to:</span>
          <select className="select" style={{ width: 200 }} defaultValue="" onChange={(e) => api.setConversationProject(useApp.getState().currentId!, e.target.value || null).then(() => toast("success", "Conversation linked"))}>
            <option value="">No project</option>
            {projects.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </div>
      )}
    </>
  );
}

export function MemoryView() {
  const [items, setItems] = useState<Memory[]>([]);
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState("");
  const [editing, setEditing] = useState<string | "new" | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const toast = useApp((s) => s.toast);
  const load = () => api.listMemories(query, kind).then(setItems).catch((e) => toast("error", errorMessage(e)));
  useEffect(() => {
    const t = setTimeout(load, 150);
    return () => clearTimeout(t);
  }, [query, kind]);
  useEffect(() => {
    window.addEventListener("wai-memory-changed", load);
    return () => window.removeEventListener("wai-memory-changed", load);
  }, [query, kind]);

  return (
    <div className="page">
      <div className="page-main">
        <h2>Memory</h2>
        <p className="desc">What Windows AI remembers about you. Stored only on this PC. Pinned and preference memories are always included; others are matched to each request.</p>
        <div className="row" style={{ marginBottom: 12 }}>
          <div style={{ position: "relative", flex: 1 }}>
            <Search size={14} style={{ position: "absolute", left: 10, top: 11, color: "var(--text-3)" }} />
            <input className="input" style={{ paddingLeft: 30 }} placeholder="Search memories" value={query} onChange={(e) => setQuery(e.target.value)} />
          </div>
          <select className="select" style={{ width: 140 }} value={kind} onChange={(e) => setKind(e.target.value)}>
            <option value="">All kinds</option>
            {KINDS.map((k) => <option key={k} value={k}>{k}</option>)}
          </select>
          <button className="btn primary" onClick={() => setEditing("new")}><Plus size={14} /> Add</button>
        </div>
        {editing === "new" && <MemoryEditor onDone={() => { setEditing(null); load(); }} />}
        {items.length === 0 && editing !== "new" && <div className="empty">No memories{query ? " match your search" : " yet"}.</div>}
        {items.map((m) =>
          editing === m.id ? (
            <MemoryEditor key={m.id} initial={m} onDone={() => { setEditing(null); load(); }} />
          ) : (
            <div key={m.id} className="memory-item">
              <div className="row" style={{ marginBottom: 4 }}>
                <span className="badge accent">{m.kind}</span>
                {m.pinned && <span className="badge"><Pin size={10} /> pinned</span>}
                {m.tags && <span className="small muted">#{m.tags}</span>}
                <span className="spacer" />
                <span className="small muted">{m.source === "assistant" ? "proposed by assistant · " : ""}{relativeTime(m.updated_at)}</span>
                <button className="icon-btn" title="Edit" onClick={() => setEditing(m.id)}><Pencil size={14} /></button>
                {confirm === m.id ? (
                  <><button className="btn sm danger" onClick={() => api.deleteMemory(m.id).then(() => { setConfirm(null); load(); })}>Delete</button><button className="btn sm ghost" onClick={() => setConfirm(null)}>Keep</button></>
                ) : (
                  <button className="icon-btn danger" title="Delete" onClick={() => setConfirm(m.id)}><Trash2 size={14} /></button>
                )}
              </div>
              <div className="content">{m.content}</div>
            </div>
          ),
        )}
        <Projects />
      </div>
    </div>
  );
}
