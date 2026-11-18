import { useEffect, useState } from "react";
import { CheckCircle2, Cloud, Cpu, ExternalLink, Sparkles, TriangleAlert } from "lucide-react";
import { api, errorMessage } from "../../lib/api";
import { PROVIDER_PRESETS } from "../../lib/presets";
import type { AppConfig, ClaudeCodeStatus } from "../../lib/types";
import { useApp } from "../../store/app";
import { Notice } from "../../components/ui";
import { SecretField } from "../settings/sections/ProvidersSection";

type Choice = "claude" | "free" | "local";

const CLOUD = PROVIDER_PRESETS.filter((p) => p.use_api_key);
const LOCAL = PROVIDER_PRESETS.filter((p) => !p.use_api_key);

/** Applies a first-run choice to a config copy. Pure, so it is unit-tested. */
export function applyChoice(cfg: AppConfig, choice: Choice, presetId: string): AppConfig {
  const next: AppConfig = structuredClone(cfg);
  if (choice === "claude") {
    next.ai.provider = "claude_code";
    return next;
  }
  const p = PROVIDER_PRESETS.find((x) => x.id === presetId);
  if (!p) return next;
  next.ai.provider = "openai_compatible";
  next.ai.openai_compatible = { ...next.ai.openai_compatible, base_url: p.base_url, model: p.model, use_api_key: p.use_api_key };
  return next;
}

function open(url: string) {
  import("@tauri-apps/plugin-opener").then((m) => m.openUrl(url));
}

export function Onboarding() {
  const config = useApp((s) => s.config);
  const saveConfig = useApp((s) => s.saveConfig);
  const [choice, setChoice] = useState<Choice | null>(null);
  const [cloudId, setCloudId] = useState("groq");
  const [localId, setLocalId] = useState("ollama");
  const [cc, setCc] = useState<ClaudeCodeStatus | null>(null);
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; detail: string } | null>(null);

  const checkClaude = () => api.claudeCodeStatus().then(setCc).catch(() => setCc(null));
  useEffect(() => {
    checkClaude();
  }, []);
  // Recommend what already works on this PC.
  useEffect(() => {
    if (choice || !cc) return;
    setChoice(cc.installed && cc.logged_in ? "claude" : "free");
  }, [cc, choice]);

  if (!config || config.startup.onboarded) return null;
  const presetId = choice === "local" ? localId : cloudId;
  const preset = PROVIDER_PRESETS.find((p) => p.id === presetId);

  const test = async () => {
    if (!choice) return;
    setTesting(true);
    setResult(null);
    try {
      if (!(await saveConfig(applyChoice(config, choice, presetId)))) throw new Error("Could not save settings");
      const st = await api.providerStatus();
      setResult({ ok: st.state === "connected", detail: st.detail });
    } catch (e) {
      setResult({ ok: false, detail: errorMessage(e) });
    } finally {
      setTesting(false);
    }
  };

  const finish = async () => {
    const cur = useApp.getState().config ?? config;
    await saveConfig({ ...cur, startup: { ...cur.startup, onboarded: true } });
  };

  const card = (id: Choice, icon: JSX.Element, title: string, desc: string) => (
    <button className={`onb-choice ${choice === id ? "on" : ""}`} onClick={() => { setChoice(id); setResult(null); }} aria-pressed={choice === id}>
      {icon}
      <span><b>{title}</b><small>{desc}</small></span>
    </button>
  );

  return (
    <div className="modal-backdrop" role="dialog" aria-label="Set up Windows AI">
      <div className="modal onb">
        <h3><Sparkles size={16} /> Welcome to Windows AI</h3>
        <p className="muted small">Pick which AI answers you. You can change this any time in Settings → AI providers.</p>

        <div className="onb-choices">
          {card("free", <Cloud size={18} />, "Free cloud model", "Groq, Gemini, OpenRouter or FreeLLMAPI. Free key, no card.")}
          {card("local", <Cpu size={18} />, "On this PC", "Ollama or LM Studio. Private, works offline.")}
          {card("claude", <Sparkles size={18} />, "Claude (Claude Code)", cc?.installed ? (cc.logged_in ? "Claude Code is signed in" : "Installed, needs sign-in") : "Needs Claude Code + a Claude plan")}
        </div>

        {choice === "free" && (
          <div className="onb-step">
            <select className="select" value={cloudId} onChange={(e) => { setCloudId(e.target.value); setResult(null); }} aria-label="Provider">
              {CLOUD.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
            </select>
            {preset && <p className="muted small">{preset.note} <a href="#" onClick={(e) => { e.preventDefault(); open(preset.key_url); }}>Get a key <ExternalLink size={11} /></a></p>}
            <SecretField name="openai_compatible_api_key" label="API key" hint="Stored in Windows Credential Manager." />
          </div>
        )}
        {choice === "local" && (
          <div className="onb-step">
            <select className="select" value={localId} onChange={(e) => { setLocalId(e.target.value); setResult(null); }} aria-label="Local server">
              {LOCAL.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
            </select>
            {preset && <p className="muted small">{preset.note} <a href="#" onClick={(e) => { e.preventDefault(); open(preset.key_url); }}>Download <ExternalLink size={11} /></a></p>}
          </div>
        )}
        {choice === "claude" && (
          <div className="onb-step">
            {!cc?.installed && <p className="muted small">Install Claude Code in PowerShell: <code>irm https://claude.ai/install.ps1 | iex</code>, then press Test.</p>}
            {cc?.installed && !cc.logged_in && <button className="btn" onClick={() => api.claudeCodeLogin().catch(() => {})}>Sign in with Claude</button>}
            <p className="muted small">Uses the official Claude Code CLI and your own Claude login. Windows AI never sees your password or tokens.</p>
          </div>
        )}

        {result && (
          <Notice kind={result.ok ? "ok" : "warn"} icon={result.ok ? <CheckCircle2 size={14} /> : <TriangleAlert size={14} />}>
            {result.ok ? "Connected. " : "Not working yet: "}{result.detail}
          </Notice>
        )}

        <div className="onb-actions">
          <button className="btn ghost" onClick={finish}>Skip for now</button>
          <span style={{ flex: 1 }} />
          <button className="btn" disabled={!choice || testing} onClick={async () => { if (choice === "claude") await checkClaude(); await test(); }}>{testing ? "Testing…" : "Test"}</button>
          <button className="btn primary" disabled={!result?.ok} onClick={finish}>Start</button>
        </div>
      </div>
    </div>
  );
}
