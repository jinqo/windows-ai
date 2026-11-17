import { useState } from "react";
import { Pencil, Pin, PinOff, Plus, Search, Trash2 } from "lucide-react";
import { useApp } from "../store/app";
import { api, errorMessage } from "../lib/api";
import { relativeTime } from "../lib/format";

export function Sidebar() {
  const open = useApp((s) => s.sidebarOpen);
  const conversations = useApp((s) => s.conversations);
  const currentId = useApp((s) => s.currentId);
  const search = useApp((s) => s.search);
  const { setSearch, openConversation, newConversation, loadConversations, toggleSidebar, toast } = useApp.getState();
  const [editing, setEditing] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const rename = async (id: string) => {
    try {
      await api.renameConversation(id, title);
      setEditing(null);
      loadConversations();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  const remove = async (id: string) => {
    await api.deleteConversation(id).catch((e) => toast("error", errorMessage(e)));
    setConfirmDelete(null);
    if (id === currentId) newConversation();
    loadConversations();
  };

  return (
    <>
      {open && <div className="scrim" onClick={toggleSidebar} />}
      <aside className={`sidebar ${open ? "open" : ""}`} aria-hidden={!open}>
        <div className="sidebar-head">
          <button className="btn primary" onClick={newConversation}>
            <Plus size={15} /> New chat
          </button>
          <div className="row" style={{ position: "relative" }}>
            <Search size={14} style={{ position: "absolute", left: 10, color: "var(--text-3)" }} />
            <input className="input" style={{ paddingLeft: 30 }} placeholder="Search conversations" value={search} onChange={(e) => setSearch(e.target.value)} />
          </div>
        </div>
        <div className="conv-list">
          {conversations.length === 0 && <div className="empty small">No conversations yet</div>}
          {conversations.map((c) => (
            <div key={c.id} className={`conv-item ${c.id === currentId ? "active" : ""}`} onClick={() => editing !== c.id && openConversation(c.id)}>
              {c.pinned && <Pin size={12} style={{ color: "var(--accent-2)", flex: "none" }} />}
              {editing === c.id ? (
                <input
                  className="input"
                  autoFocus
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  onClick={(e) => e.stopPropagation()}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") rename(c.id);
                    if (e.key === "Escape") {
                      e.preventDefault();
                      setEditing(null);
                    }
                  }}
                  onBlur={() => rename(c.id)}
                />
              ) : confirmDelete === c.id ? (
                <div className="row title" onClick={(e) => e.stopPropagation()}>
                  <span className="small">Delete?</span>
                  <button className="btn sm danger" onClick={() => remove(c.id)}>Delete</button>
                  <button className="btn sm ghost" onClick={() => setConfirmDelete(null)}>Cancel</button>
                </div>
              ) : (
                <>
                  <div className="title" title={c.title}>{c.title}</div>
                  <span className="muted small actions-hide">{relativeTime(c.updated_at)}</span>
                  <div className="actions" onClick={(e) => e.stopPropagation()}>
                    <button className="icon-btn" style={{ width: 26, height: 26 }} title={c.pinned ? "Unpin" : "Pin"} onClick={() => api.pinConversation(c.id, !c.pinned).then(loadConversations)}>
                      {c.pinned ? <PinOff size={13} /> : <Pin size={13} />}
                    </button>
                    <button className="icon-btn" style={{ width: 26, height: 26 }} title="Rename" onClick={() => { setEditing(c.id); setTitle(c.title); }}>
                      <Pencil size={13} />
                    </button>
                    <button className="icon-btn danger" style={{ width: 26, height: 26 }} title="Delete" onClick={() => setConfirmDelete(c.id)}>
                      <Trash2 size={13} />
                    </button>
                  </div>
                </>
              )}
            </div>
          ))}
        </div>
      </aside>
    </>
  );
}
