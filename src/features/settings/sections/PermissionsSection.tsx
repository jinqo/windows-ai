import { useEffect, useState } from "react";
import { Lock, Plus, Trash2 } from "lucide-react";
import type { SectionProps } from "../SettingsView";
import { Card, Field, Notice, Segmented, Toggle } from "../../../components/ui";
import { api, errorMessage } from "../../../lib/api";
import { useApp } from "../../../store/app";
import type { PermissionMode, ToolInfo } from "../../../lib/types";

export function PermissionsSection({ draft, update }: SectionProps) {
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const toast = useApp((s) => s.toast);
  const load = () => api.listTools().then(setTools).catch((e) => toast("error", errorMessage(e)));
  useEffect(() => {
    load();
  }, []);
  const p = draft.permissions;
  const setMode = async (t: ToolInfo, mode: PermissionMode) => {
    try {
      await api.setToolMode(t.name, mode);
      load();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };
  const [dir, setDir] = useState("");
  const [app, setApp] = useState({ name: "", path: "" });
  const [cmd, setCmd] = useState({ name: "", program: "", args: "", cwd: "", description: "" });

  return (
    <>
      <h2>Permissions</h2>
      <p className="desc">Everything the assistant can do on this PC. Sensitive actions always show a confirmation with an exact description first, and every call is logged in Activity.</p>
      <Notice kind="info" icon={<Lock size={16} />}>There is no unrestricted shell. Commands run only from your allowlist, files only inside approved folders, and Windows AI never asks for administrator rights.</Notice>

      <Card title="Tools">
        {tools.map((t) => (
          <div key={t.name} className="list-row">
            <div className="grow">
              <div className="row"><span className="mono">{t.name}</span><span className="badge">{t.access}</span>{t.origin !== "builtin" && <span className="badge accent">{t.origin}</span>}{t.locked_ask && <span className="badge warn" title="Always asks before running">always asks</span>}</div>
              <div className="small muted ellipsis" title={t.description}>{t.description}</div>
            </div>
            <Segmented
              value={t.mode}
              onChange={(m) => setMode(t, m)}
              options={[
                { value: "allow", label: "Allow", disabled: t.locked_ask, className: "allow" },
                { value: "ask", label: "Ask", className: "ask" },
                { value: "deny", label: "Deny", className: "deny" },
              ]}
            />
          </div>
        ))}
        <div className="list-row"><button className="btn sm" onClick={() => api.clearSessionApprovals().then(() => toast("success", "Session approvals cleared"))}>Reset “allow for this session” approvals</button></div>
      </Card>

      <Card title="Approved folders">
        {p.directories.length === 0 && <div className="small muted" style={{ padding: "10px 0" }}>No folders approved — file tools cannot access anything.</div>}
        {p.directories.map((d, i) => (
          <div key={i} className="list-row">
            <div className="grow mono ellipsis" title={d.path}>{d.path}</div>
            <span className="small muted">Read</span>
            <Toggle on={d.read} onChange={(v) => update((c) => { c.permissions.directories[i].read = v; })} />
            <span className="small muted">Write</span>
            <Toggle on={d.write} onChange={(v) => update((c) => { c.permissions.directories[i].write = v; })} />
            <button className="icon-btn danger" onClick={() => update((c) => { c.permissions.directories.splice(i, 1); })}><Trash2 size={14} /></button>
          </div>
        ))}
        <div className="list-row">
          <input className="input" placeholder="C:\Users\you\Documents" value={dir} onChange={(e) => setDir(e.target.value)} />
          <button className="btn" disabled={!/^[a-zA-Z]:\\/.test(dir.trim())} onClick={() => { update((c) => { c.permissions.directories.push({ path: dir.trim(), read: true, write: false }); }); setDir(""); }}>
            <Plus size={14} /> Add (read-only)
          </button>
        </div>
      </Card>

      <Card title="Approved applications">
        {p.apps.map((a, i) => (
          <div key={i} className="list-row">
            <b style={{ width: 120 }}>{a.name}</b>
            <div className="grow mono ellipsis">{a.path} {a.args.join(" ")}</div>
            <button className="icon-btn danger" onClick={() => update((c) => { c.permissions.apps.splice(i, 1); })}><Trash2 size={14} /></button>
          </div>
        ))}
        <div className="list-row">
          <input className="input" style={{ width: 140 }} placeholder="name" value={app.name} onChange={(e) => setApp({ ...app, name: e.target.value })} />
          <input className="input" placeholder="C:\Path\app.exe or app.exe" value={app.path} onChange={(e) => setApp({ ...app, path: e.target.value })} />
          <button className="btn" disabled={!app.name.trim() || !app.path.trim()} onClick={() => { update((c) => { c.permissions.apps.push({ name: app.name.trim(), path: app.path.trim(), args: [] }); }); setApp({ name: "", path: "" }); }}><Plus size={14} /> Add</button>
        </div>
      </Card>

      <Card title="Allow-listed commands">
        <div className="small muted" style={{ paddingTop: 10 }}>The assistant can only run these exact commands, by name, and must ask every time.</div>
        {p.commands.map((c, i) => (
          <div key={i} className="list-row">
            <div className="grow">
              <b>{c.name}</b> <span className="small muted">{c.description}</span>
              <div className="mono small ellipsis">{c.program} {c.args.join(" ")}{c.cwd ? `  (in ${c.cwd})` : ""}</div>
            </div>
            <button className="icon-btn danger" onClick={() => update((x) => { x.permissions.commands.splice(i, 1); })}><Trash2 size={14} /></button>
          </div>
        ))}
        <div className="list-row" style={{ flexWrap: "wrap" }}>
          <input className="input" style={{ width: 130 }} placeholder="name" value={cmd.name} onChange={(e) => setCmd({ ...cmd, name: e.target.value })} />
          <input className="input" style={{ width: 130 }} placeholder="program" value={cmd.program} onChange={(e) => setCmd({ ...cmd, program: e.target.value })} />
          <input className="input" style={{ flex: 1, minWidth: 140 }} placeholder="arguments (space separated)" value={cmd.args} onChange={(e) => setCmd({ ...cmd, args: e.target.value })} />
          <input className="input" style={{ flex: 1, minWidth: 140 }} placeholder="working folder (optional)" value={cmd.cwd} onChange={(e) => setCmd({ ...cmd, cwd: e.target.value })} />
          <input className="input" style={{ flex: 2, minWidth: 200 }} placeholder="description" value={cmd.description} onChange={(e) => setCmd({ ...cmd, description: e.target.value })} />
          <button className="btn" disabled={!cmd.name.trim() || !cmd.program.trim()} onClick={() => {
            update((x) => { x.permissions.commands.push({ name: cmd.name.trim(), program: cmd.program.trim(), args: cmd.args.split(" ").filter(Boolean), cwd: cmd.cwd.trim() || null, description: cmd.description, timeout_secs: 60 }); });
            setCmd({ name: "", program: "", args: "", cwd: "", description: "" });
          }}><Plus size={14} /> Add</button>
        </div>
      </Card>

      <Card title="Limits">
        <Field label="Max file read size" hint="Bytes per read.">
          <input className="input" type="number" value={p.max_read_bytes} onChange={(e) => update((c) => { c.permissions.max_read_bytes = Number(e.target.value) || 524288; })} style={{ width: 120 }} />
        </Field>
        <Field label="Approval timeout" hint="Seconds before an unanswered request is denied.">
          <input className="input" type="number" value={p.approval_timeout_secs} onChange={(e) => update((c) => { c.permissions.approval_timeout_secs = Number(e.target.value) || 300; })} style={{ width: 100 }} />
        </Field>
      </Card>
    </>
  );
}
