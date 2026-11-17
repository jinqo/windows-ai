import { useEffect, useState } from "react";
import { Plug, PlugZap, Plus, RefreshCw, ShieldCheck, Trash2 } from "lucide-react";
import type { SectionProps } from "../SettingsView";
import { Card, Field, Notice, Toggle } from "../../../components/ui";
import { api, errorMessage } from "../../../lib/api";
import { useApp } from "../../../store/app";
import type { McpServerStatus } from "../../../lib/types";

export function McpSection({ draft, update }: SectionProps) {
  const config = useApp((s) => s.config);
  const toast = useApp((s) => s.toast);
  const [statuses, setStatuses] = useState<McpServerStatus[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [form, setForm] = useState({ id: "", name: "", command: "", args: "" });
  const load = () => api.mcpStatuses().then(setStatuses).catch(() => {});
  useEffect(() => {
    load();
  }, [config]);
  const savedIds = new Set(config?.mcp.servers.map((s) => s.id));

  const act = async (id: string, fn: () => Promise<unknown>) => {
    setBusy(id);
    try {
      await fn();
    } catch (e) {
      toast("error", errorMessage(e));
    }
    setBusy(null);
    load();
  };

  return (
    <>
      <h2>MCP servers</h2>
      <p className="desc">Connect Model Context Protocol servers (local stdio processes) to give Windows AI more tools.</p>
      <Notice kind="info" icon={<ShieldCheck size={16} />}>New tools are <b>untrusted</b> until you trust each one. Trusted tools still go through the permission system (default: ask every time).</Notice>
      {draft.mcp.servers.map((s, i) => {
        const st = statuses.find((x) => x.id === s.id);
        const saved = savedIds.has(s.id);
        return (
          <Card key={s.id} title={s.name || s.id}>
            <Field label={<span className="row">{s.name} <span className={`badge ${st?.state === "connected" ? "ok" : st?.state === "error" ? "err" : ""}`}>{st?.state ?? "not saved"}</span></span>} hint={<span className="mono">{s.command} {s.args.join(" ")}</span>}>
              <Toggle on={s.enabled} onChange={(v) => update((c) => { c.mcp.servers[i].enabled = v; })} label="Enabled" />
              {st?.state === "connected" ? (
                <button className="btn sm" disabled={busy === s.id} onClick={() => act(s.id, () => api.mcpDisconnect(s.id))}><PlugZap size={13} /> Disconnect</button>
              ) : (
                <button className="btn sm" disabled={!saved || busy === s.id} title={saved ? "" : "Save settings first"} onClick={() => act(s.id, () => api.mcpConnect(s.id))}><Plug size={13} /> Connect</button>
              )}
              <button className="icon-btn danger" onClick={() => update((c) => { c.mcp.servers.splice(i, 1); })}><Trash2 size={14} /></button>
            </Field>
            {st?.error && <div className="notice err small" style={{ margin: "8px 0" }}>{st.error}</div>}
            {st?.tools.map((t) => (
              <div key={t.name} className="list-row">
                <div className="grow">
                  <span className="mono">{t.name}</span>
                  <div className="small muted ellipsis" title={t.description}>{t.description}</div>
                </div>
                <span className="small muted">{t.trusted ? "Trusted" : "Untrusted"}</span>
                <Toggle on={t.trusted} onChange={(v) => act(s.id, () => api.mcpSetToolTrust(s.id, t.name, v))} label={`Trust ${t.name}`} />
              </div>
            ))}
          </Card>
        );
      })}
      <Card title="Add server">
        <div className="list-row" style={{ flexWrap: "wrap" }}>
          <input className="input" style={{ width: 120 }} placeholder="id (a-z, 0-9, -)" value={form.id} onChange={(e) => setForm({ ...form, id: e.target.value.replace(/[^a-zA-Z0-9_-]/g, "") })} />
          <input className="input" style={{ width: 160 }} placeholder="Display name" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} />
          <input className="input" style={{ width: 140 }} placeholder="command (npx, node…)" value={form.command} onChange={(e) => setForm({ ...form, command: e.target.value })} />
          <input className="input" style={{ flex: 1, minWidth: 200 }} placeholder="arguments (space separated)" value={form.args} onChange={(e) => setForm({ ...form, args: e.target.value })} />
          <button className="btn" disabled={!form.id || !form.command || draft.mcp.servers.some((s) => s.id === form.id)} onClick={() => {
            update((c) => { c.mcp.servers.push({ id: form.id, name: form.name || form.id, command: form.command, args: form.args.split(" ").filter(Boolean), env: {}, cwd: null, enabled: true, trusted_tools: [] }); });
            setForm({ id: "", name: "", command: "", args: "" });
          }}><Plus size={14} /> Add</button>
        </div>
        <div className="list-row"><button className="btn sm" onClick={load}><RefreshCw size={13} /> Refresh status</button></div>
      </Card>
    </>
  );
}
