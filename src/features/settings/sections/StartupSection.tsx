import { useEffect, useState } from "react";
import type { SectionProps } from "../SettingsView";
import { Card, Field, Toggle } from "../../../components/ui";
import { api } from "../../../lib/api";
import { useUpdater } from "../../updater/UpdateBanner";

export function StartupSection({ draft, update }: SectionProps) {
  const [info, setInfo] = useState<Record<string, unknown> | null>(null);
  const up = useUpdater();
  useEffect(() => {
    api.appInfo().then(setInfo).catch(() => {});
  }, []);
  return (
    <>
      <h2>Startup</h2>
      <p className="desc">Closing the window keeps Windows AI running in the system tray. Quit from the tray menu.</p>
      <Card>
        <Field label="Launch when I sign in to Windows" hint="Starts minimized to the tray (per-user, no admin rights).">
          <Toggle on={draft.startup.launch_on_login} onChange={(v) => update((d) => { d.startup.launch_on_login = v; })} />
        </Field>
        <Field label="Start hidden" hint="Don’t show the overlay when the app starts.">
          <Toggle on={draft.startup.start_hidden} onChange={(v) => update((d) => { d.startup.start_hidden = v; })} />
        </Field>
      </Card>
      {info && (
        <Card title="About">
          <Field label="Version" hint={up.phase === "none" ? "You're up to date." : up.phase === "available" ? `Version ${up.update?.version} is available.` : up.phase === "error" ? `Update check failed: ${up.error}` : undefined}>
            <span className="mono">{String(info.version)}</span>
            {up.phase === "available"
              ? <button className="btn primary" onClick={up.install}>Install &amp; restart</button>
              : <button className="btn" disabled={up.phase === "checking" || up.phase === "downloading"} onClick={() => up.check(true)}>{up.phase === "checking" ? "Checking…" : up.phase === "downloading" ? `Downloading ${up.progress}%` : "Check for updates"}</button>}
          </Field>
          <Field label="Settings file" stack><span className="mono selectable small">{String(info.config_path)}</span></Field>
          <Field label="Data folder" stack><span className="mono selectable small">{String(info.data_dir)}</span></Field>
        </Card>
      )}
    </>
  );
}
