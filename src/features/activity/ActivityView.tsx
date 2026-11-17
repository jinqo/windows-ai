import { useEffect, useState } from "react";
import { CheckCircle2, CircleDashed, Clock, ListChecks, Play, RotateCcw, SkipForward, Square, Trash2, XCircle, Ban } from "lucide-react";
import { api, errorMessage } from "../../lib/api";
import { useApp } from "../../store/app";
import { prettyJson, relativeTime } from "../../lib/format";
import type { ActivityEntry, Task } from "../../lib/types";

function StatusIcon({ s }: { s: string }) {
  if (s === "success" || s === "completed") return <CheckCircle2 size={16} style={{ color: "var(--ok)" }} />;
  if (s === "running") return <CircleDashed size={16} style={{ color: "var(--accent-2)" }} className="dot pulse" />;
  if (s === "pending_approval" || s === "awaiting_review" || s === "pending") return <Clock size={16} style={{ color: "var(--warn)" }} />;
  if (s === "denied") return <Ban size={16} style={{ color: "var(--warn)" }} />;
  if (s === "cancelled" || s === "skipped") return <Ban size={16} className="muted" />;
  return <XCircle size={16} style={{ color: "var(--err)" }} />;
}

function TaskCard({ task, reload }: { task: Task; reload: () => void }) {
  const toast = useApp((s) => s.toast);
  const call = (fn: () => Promise<unknown>) => fn().then(reload).catch((e) => toast("error", errorMessage(e)));
  const done = task.steps.filter((s) => s.status === "success" || s.status === "skipped").length;
  return (
    <div className="memory-item">
      <div className="row">
        <ListChecks size={16} style={{ color: "var(--accent-2)" }} />
        <b style={{ flex: 1 }}>{task.title}</b>
        <span className={`badge ${task.status === "completed" ? "ok" : task.status === "failed" ? "err" : task.status === "running" ? "accent" : ""}`}>{task.status.replace("_", " ")}</span>
        <span className="small muted">{done}/{task.steps.length}</span>
      </div>
      <div style={{ height: 4, background: "var(--surface-3)", borderRadius: 4, margin: "10px 0 4px", overflow: "hidden" }}>
        <div style={{ width: `${(done / Math.max(1, task.steps.length)) * 100}%`, height: "100%", background: "var(--accent)", transition: "width 300ms" }} />
      </div>
      {task.steps.map((s) => (
        <div key={s.id} className="step">
          <div className={`step-dot ${s.status}`}>{s.idx + 1}</div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div>{s.description} <span className="mono small muted">{s.tool}</span></div>
            {s.error && <div className="small" style={{ color: "var(--err)" }}>{s.error}</div>}
            {s.result && <details className="small muted"><summary style={{ cursor: "pointer" }}>Result</summary><pre className="mono selectable" style={{ whiteSpace: "pre-wrap", maxHeight: 160, overflow: "auto" }}>{s.result}</pre></details>}
          </div>
          <StatusIcon s={s.status} />
        </div>
      ))}
      {task.error && <div className="notice err small">{task.error}</div>}
      <div className="row" style={{ justifyContent: "flex-end", marginTop: 6 }}>
        {task.status === "awaiting_review" && <button className="btn primary sm" onClick={() => call(() => api.runTask(task.id))}><Play size={13} /> Run (each step still asks if needed)</button>}
        {task.status === "failed" && (
          <>
            <button className="btn sm" onClick={() => call(() => api.runTask(task.id))}><RotateCcw size={13} /> Retry step</button>
            <button className="btn sm" onClick={() => call(() => api.skipTaskStep(task.id))}><SkipForward size={13} /> Skip step</button>
          </>
        )}
        {["running", "awaiting_review", "failed"].includes(task.status) && <button className="btn sm danger" onClick={() => call(() => api.cancelTask(task.id))}><Square size={12} /> Cancel</button>}
        {task.status !== "running" && <button className="icon-btn danger" title="Delete" onClick={() => call(() => api.deleteTask(task.id))}><Trash2 size={14} /></button>}
      </div>
    </div>
  );
}

export function ActivityView() {
  const [entries, setEntries] = useState<ActivityEntry[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [filter, setFilter] = useState("");
  const toast = useApp((s) => s.toast);
  const load = () => {
    api.listActivity(filter).then(setEntries).catch((e) => toast("error", errorMessage(e)));
    api.listTasks().then(setTasks).catch(() => {});
  };
  useEffect(() => {
    load();
    const on = () => load();
    window.addEventListener("wai-activity-changed", on);
    return () => window.removeEventListener("wai-activity-changed", on);
  }, [filter]);

  return (
    <div className="page">
      <div className="page-main">
        <h2>Tasks</h2>
        <p className="desc">Multi-step plans drafted by the assistant. Nothing runs until you start it; sensitive steps still ask for approval.</p>
        {tasks.length === 0 && <div className="empty small">No tasks yet. Ask: “make a task plan to…”.</div>}
        {tasks.map((t) => <TaskCard key={t.id} task={t} reload={load} />)}

        <div className="row" style={{ margin: "22px 0 8px" }}>
          <h2 style={{ margin: 0 }}>Activity log</h2>
          <span className="spacer" />
          <select className="select" style={{ width: 170 }} value={filter} onChange={(e) => setFilter(e.target.value)}>
            <option value="">All actions</option>
            <option value="success">Succeeded</option>
            <option value="error">Errors</option>
            <option value="denied">Denied</option>
            <option value="pending_approval">Awaiting approval</option>
            <option value="cancelled">Cancelled</option>
          </select>
          <button className="btn sm" onClick={() => api.clearActivity().then(load)}>Clear</button>
        </div>
        {entries.length === 0 && <div className="empty small">No tool activity yet.</div>}
        {entries.map((e) => (
          <div key={e.id} className="activity-item">
            <StatusIcon s={e.status} />
            <div style={{ minWidth: 0 }}>
              <div>{e.description}</div>
              <div className="small muted"><span className="mono">{e.tool}</span> · {e.source}{e.duration_ms != null ? ` · ${e.duration_ms} ms` : ""} · {e.status.replace("_", " ")}</div>
              {e.error && <div className="small" style={{ color: "var(--err)" }}>{e.error}</div>}
              <details className="small muted"><summary style={{ cursor: "pointer" }}>Details</summary><pre>{prettyJson(e.input)}{e.result ? `\n\n→ ${e.result}` : ""}</pre></details>
            </div>
            <span className="small muted">{relativeTime(e.created_at)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
