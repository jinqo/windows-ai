import { useEffect, useState } from "react";
import { Code2, FolderOpen, Globe, MousePointerSquareDashed } from "lucide-react";
import type { SectionProps } from "../SettingsView";
import { Card, Field, Notice, Segmented, Toggle } from "../../../components/ui";
import { api, errorMessage } from "../../../lib/api";
import { useApp } from "../../../store/app";

export function IntegrationsSection({ draft, update }: SectionProps) {
  const toast = useApp((s) => s.toast);
  const [status, setStatus] = useState<Record<string, any> | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api.integrationStatus().then(setStatus).catch(() => {});
  }, []);
  const run = async (fn: () => Promise<Record<string, any>>, ok: string) => {
    setBusy(true);
    try {
      setStatus(await fn());
      toast("success", ok);
    } catch (e) {
      toast("error", errorMessage(e));
    }
    setBusy(false);
  };
  const i = draft.integrations;
  const browserOn = !!(status?.browser_host?.chrome || status?.browser_host?.edge);

  return (
    <>
      <h2>Integrations</h2>
      <p className="desc">Bring content from other apps into Windows AI. Every integration opens the same assistant, conversations and memory, and you always see exactly what will be shared before sending.</p>

      <Card title="Selected text (any app)">
        <Field label={<span className="row"><MousePointerSquareDashed size={15} /> Selection shortcut</span>} hint={<>Select text, press <span className="kbd">{i.selection_shortcut}</span>. Nothing is captured until you press it — the clipboard is never monitored.</>}>
          <Toggle on={i.selection_enabled} onChange={(v) => update((d) => { d.integrations.selection_enabled = v; })} />
        </Field>
        <Field label="After capturing" hint="Floating toolbar near the cursor, or the full assistant.">
          <Segmented value={i.selection_mode} onChange={(v) => update((d) => { d.integrations.selection_mode = v; })} options={[{ value: "toolbar", label: "Toolbar" }, { value: "overlay", label: "Assistant" }]} />
        </Field>
        <Field label="Clipboard fallback" hint="If an app doesn’t expose its selection through UI Automation, briefly copy it (Ctrl+C) and restore your previous clipboard immediately afterwards.">
          <Toggle on={i.clipboard_fallback} onChange={(v) => update((d) => { d.integrations.clipboard_fallback = v; })} />
        </Field>
        <Field label="Translate to">
          <input className="input" value={i.translate_language} onChange={(e) => update((d) => { d.integrations.translate_language = e.target.value; })} style={{ width: 150 }} />
        </Field>
        <Field label="Max captured size" hint="Characters; longer content is truncated and marked as such.">
          <input className="input" type="number" value={i.max_context_chars} onChange={(e) => update((d) => { d.integrations.max_context_chars = Number(e.target.value) || 100000; })} style={{ width: 120 }} />
        </Field>
      </Card>

      <Card title="File Explorer">
        <Field label={<span className="row"><FolderOpen size={15} /> Right-click menu</span>} hint="Adds “Windows AI → Ask / Explain / Summarize / Analyze” for files and folders (per-user, no admin). On Windows 11 it appears under “Show more options”.">
          <span className={`badge ${status?.explorer_menu ? "ok" : ""}`}>{status?.explorer_menu ? "Installed" : "Not installed"}</span>
          <button className="btn sm" disabled={busy} onClick={() => run(() => api.setExplorerMenu(!status?.explorer_menu), status?.explorer_menu ? "Menu removed" : "Menu installed")}>
            {status?.explorer_menu ? "Remove" : "Install"}
          </button>
        </Field>
      </Card>

      <Card title="Chrome & Edge">
        <Field label={<span className="row"><Globe size={15} /> Browser connection</span>} hint="Registers the native-messaging host so the Windows AI extension can send selected text and pages here.">
          <span className={`badge ${browserOn ? "ok" : ""}`}>{browserOn ? `Chrome ${status?.browser_host?.chrome ? "✓" : "✗"} · Edge ${status?.browser_host?.edge ? "✓" : "✗"}` : "Not registered"}</span>
          <button className="btn sm" disabled={busy} onClick={() => run(() => api.setBrowserHost(!browserOn), browserOn ? "Browser connection removed" : "Browser connection registered")}>
            {browserOn ? "Remove" : "Register"}
          </button>
        </Field>
        <div className="small muted" style={{ padding: "8px 0 12px" }}>
          Then load the extension: open <span className="mono">chrome://extensions</span> or <span className="mono">edge://extensions</span>, enable Developer mode, choose “Load unpacked” and select the <span className="mono">extensions/browser</span> folder. Extension ID: <span className="mono selectable">{status?.extension_id}</span>
        </div>
      </Card>

      <Card title="VS Code">
        <Field label={<span className="row"><Code2 size={15} /> VS Code extension</span>} hint="Explain, debug, refactor and document selected code; send diagnostics. Install the .vsix from extensions/vscode (see README).">
          <span className="badge">{draft.integrations.bridge_enabled ? "Bridge on" : "Bridge off"}</span>
        </Field>
        <Field label="Accept content from extensions" hint="The local bridge only accepts authenticated requests from your user account and can only deliver context — it cannot run tools or send messages.">
          <Toggle on={i.bridge_enabled} onChange={(v) => update((d) => { d.integrations.bridge_enabled = v; })} />
        </Field>
        {status?.bridge_pipe && <div className="small muted mono" style={{ paddingBottom: 12 }}>{status.bridge_pipe}</div>}
      </Card>

      <Notice kind="info">Captured content is sent only to the AI provider selected under AI providers, and only when you press Send.</Notice>
    </>
  );
}
