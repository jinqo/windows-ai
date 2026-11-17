import { useEffect, useState } from "react";
import { CheckCircle2, ExternalLink, KeyRound, LogIn, RefreshCw, TriangleAlert } from "lucide-react";
import type { SectionProps } from "../SettingsView";
import { Card, Field, Notice, Segmented, Toggle } from "../../../components/ui";
import { api, errorMessage } from "../../../lib/api";
import { useApp } from "../../../store/app";
import type { ClaudeCodeStatus } from "../../../lib/types";

function SecretField({ name, label, hint }: { name: string; label: string; hint: string }) {
  const [status, setStatus] = useState<{ stored: boolean; from_env: boolean } | null>(null);
  const [value, setValue] = useState("");
  const toast = useApp((s) => s.toast);
  const load = () => api.secretStatus(name).then(setStatus).catch(() => {});
  useEffect(() => {
    load();
  }, [name]);
  return (
    <Field
      label={label}
      hint={<>{hint} {status?.stored ? "Stored in Windows Credential Manager." : status?.from_env ? "Using environment variable." : "Not set."}</>}
      stack
    >
      <input className="input" type="password" autoComplete="off" placeholder={status?.stored ? "•••••••• (stored)" : "Paste key"} value={value} onChange={(e) => setValue(e.target.value)} style={{ flex: 1 }} />
      <button className="btn" disabled={!value.trim()} onClick={async () => {
        try {
          await api.setSecret(name, value);
          setValue("");
          toast("success", "Key saved securely");
          load();
          useApp.getState().refreshProvider();
        } catch (e) {
          toast("error", errorMessage(e));
        }
      }}>
        <KeyRound size={14} /> Save
      </button>
      {status?.stored && <button className="btn danger" onClick={() => api.deleteSecret(name).then(load)}>Remove</button>}
    </Field>
  );
}

function ClaudeCodePanel({ draft, update }: SectionProps) {
  const [st, setSt] = useState<ClaudeCodeStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const toast = useApp((s) => s.toast);
  const check = async () => {
    setChecking(true);
    try {
      setSt(await api.claudeCodeStatus());
      useApp.getState().refreshProvider();
    } finally {
      setChecking(false);
    }
  };
  useEffect(() => {
    check();
  }, []);
  // After launching sign-in, poll until the CLI reports a login.
  const login = async () => {
    try {
      await api.claudeCodeLogin();
      toast("info", "Complete sign-in in the Claude Code window and your browser.");
      for (let i = 0; i < 60; i++) {
        await new Promise((r) => setTimeout(r, 3000));
        const s = await api.claudeCodeStatus();
        setSt(s);
        if (s.logged_in) {
          toast("success", "Signed in to Claude Code");
          useApp.getState().refreshProvider();
          break;
        }
      }
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };
  const c = draft.ai.claude_code;
  return (
    <>
      {st && !st.installed && (
        <Notice kind="warn" icon={<TriangleAlert size={16} />}>
          <b>Claude Code is not installed.</b> Install the official CLI, then come back and sign in.
          <div className="mono small" style={{ marginTop: 6 }}>PowerShell: irm https://claude.ai/install.ps1 | iex</div>
          <div className="small" style={{ marginTop: 4 }}>
            Docs:{" "}
            <a href="#" onClick={(e) => { e.preventDefault(); import("@tauri-apps/plugin-opener").then((m) => m.openUrl("https://code.claude.com/docs/en/setup")); }}>
              code.claude.com/docs/en/setup <ExternalLink size={11} />
            </a>
          </div>
        </Notice>
      )}
      {st?.installed && !st.logged_in && (
        <Notice kind="warn" icon={<LogIn size={16} />}>
          <b>Claude Code is installed but not signed in.</b> Sign-in happens in Claude Code’s own official flow — Windows AI never sees your password or tokens.
        </Notice>
      )}
      {st?.logged_in && st.billing === "subscription" && (
        <Notice kind="ok" icon={<CheckCircle2 size={16} />}>
          Signed in with your <b>Claude {st.subscription_type ?? ""}</b> subscription. Billing mode: <b>subscription usage limits</b> (no API charges).
          {st.login_expires_at && <span className="muted"> Session renews automatically; current login valid until {new Date(st.login_expires_at).toLocaleDateString()}.</span>}
        </Notice>
      )}
      {st?.logged_in && st.billing === "api" && (
        <Notice kind="err" icon={<TriangleAlert size={16} />}>
          Claude Code is signed in with <b>{st.auth_method}</b>, which is <b>billed as API usage</b>. Subscription mode will not run unless you allow API billing below, or sign in with your Claude subscription.
        </Notice>
      )}
      <Card title="Claude Code (subscription)">
        <Field label="Status" hint={st ? `${st.installed ? `Installed ${st.version ?? ""}` : "Not installed"}${st.path ? ` · ${st.path}` : ""}` : "Checking…"}>
          <button className="btn" onClick={check} disabled={checking}><RefreshCw size={14} /> Check</button>
          {st?.installed && <button className="btn primary" onClick={login}><LogIn size={14} /> {st.logged_in ? "Sign in again" : "Sign in with Claude"}</button>}
        </Field>
        <Field label="Model" hint="Alias for the latest model of a family, or a full model id. Empty uses Claude Code’s default.">
          <select className="select" value={["", "opus", "sonnet", "haiku", "fable"].includes(c.model) ? c.model : "custom"} onChange={(e) => update((d) => { d.ai.claude_code.model = e.target.value === "custom" ? "claude-opus-5-5" : e.target.value; })}>
            <option value="">Claude Code default</option>
            <option value="opus">Opus (latest)</option>
            <option value="sonnet">Sonnet (latest)</option>
            <option value="haiku">Haiku (latest)</option>
            <option value="fable">Fable (latest)</option>
            <option value="custom">Custom id…</option>
          </select>
        </Field>
        {!["", "opus", "sonnet", "haiku", "fable"].includes(c.model) && (
          <Field label="Custom model id"><input className="input" value={c.model} onChange={(e) => update((d) => { d.ai.claude_code.model = e.target.value.trim(); })} /></Field>
        )}
        <Field label="Effort" hint="How much the model thinks. Empty uses the default.">
          <select className="select" value={c.effort} onChange={(e) => update((d) => { d.ai.claude_code.effort = e.target.value; })}>
            <option value="">Default</option>
            {["low", "medium", "high", "xhigh", "max"].map((x) => <option key={x} value={x}>{x}</option>)}
          </select>
        </Field>
        <Field label="Windows tools" hint="Offer Windows AI’s permission-checked tools to Claude over a private, per-request MCP connection. Claude Code’s own built-in tools stay disabled.">
          <Toggle on={c.enable_tools} onChange={(v) => update((d) => { d.ai.claude_code.enable_tools = v; })} />
        </Field>
        <Field label="Request timeout" hint="Seconds before a request is stopped.">
          <input className="input" type="number" min={30} max={7200} value={c.timeout_secs} onChange={(e) => update((d) => { d.ai.claude_code.timeout_secs = Number(e.target.value) || 900; })} style={{ width: 110 }} />
        </Field>
        <Field label="CLI path" hint="Leave empty to auto-detect claude.exe.">
          <input className="input" placeholder="Auto-detect" value={c.cli_path} onChange={(e) => update((d) => { d.ai.claude_code.cli_path = e.target.value; })} />
        </Field>
        <Field label="Allow API billing" hint="Only if Claude Code is signed in with an Anthropic Console account and you accept per-use charges. Off by default.">
          <Toggle on={c.allow_api_billing} onChange={(v) => update((d) => { d.ai.claude_code.allow_api_billing = v; })} />
        </Field>
      </Card>
    </>
  );
}

export function ProvidersSection(props: SectionProps) {
  const { draft, update } = props;
  const provider = useApp((s) => s.provider);
  return (
    <>
      <h2>AI providers</h2>
      <p className="desc">Choose how Windows AI talks to Claude. The active mode and how it is billed are always shown here and in the chat status line.</p>
      <Card title="Active provider">
        <Field label="Mode" hint={provider ? `${provider.state} — ${provider.detail}` : undefined}>
          <Segmented
            value={draft.ai.provider}
            onChange={(v) => update((d) => { d.ai.provider = v; })}
            options={[
              { value: "claude_code", label: "Claude subscription" },
              { value: "anthropic", label: "API key" },
              { value: "openai_compatible", label: "Other" },
            ]}
          />
        </Field>
        <Field label="Billing" hint="Windows AI never switches billing mode on its own.">
          <span className={`badge ${draft.ai.provider === "claude_code" ? "ok" : draft.ai.provider === "anthropic" ? "warn" : ""}`}>
            {draft.ai.provider === "claude_code" ? "Claude subscription (usage limits)" : draft.ai.provider === "anthropic" ? "Anthropic API (pay per use)" : "Depends on your server"}
          </span>
        </Field>
      </Card>

      {draft.ai.provider === "claude_code" && <ClaudeCodePanel {...props} />}

      {draft.ai.provider === "anthropic" && (
        <Card title="Anthropic API">
          <Notice kind="info">Uses an API key from console.anthropic.com, billed per token to that Console account. This is separate from a Claude.ai subscription.</Notice>
          <SecretField name="anthropic_api_key" label="API key" hint="Starts with sk-ant-." />
          <Field label="Model">
            <select className="select" value={draft.ai.anthropic.model} onChange={(e) => update((d) => { d.ai.anthropic.model = e.target.value; })}>
              {["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5", "claude-fable-5-1"].map((m) => <option key={m}>{m}</option>)}
            </select>
          </Field>
          <Field label="Effort">
            <select className="select" value={draft.ai.anthropic.effort} onChange={(e) => update((d) => { d.ai.anthropic.effort = e.target.value; })}>
              {["low", "medium", "high", "xhigh", "max"].map((x) => <option key={x}>{x}</option>)}
            </select>
          </Field>
          <Field label="Max output tokens">
            <input className="input" type="number" value={draft.ai.anthropic.max_tokens} onChange={(e) => update((d) => { d.ai.anthropic.max_tokens = Number(e.target.value) || 32000; })} style={{ width: 120 }} />
          </Field>
        </Card>
      )}

      {draft.ai.provider === "openai_compatible" && (
        <Card title="OpenAI-compatible server">
          <Notice kind="info">For local models (e.g. Ollama at http://localhost:11434/v1) or other compatible endpoints.</Notice>
          <Field label="Base URL"><input className="input" value={draft.ai.openai_compatible.base_url} onChange={(e) => update((d) => { d.ai.openai_compatible.base_url = e.target.value; })} /></Field>
          <Field label="Model"><input className="input" value={draft.ai.openai_compatible.model} onChange={(e) => update((d) => { d.ai.openai_compatible.model = e.target.value; })} /></Field>
          <Field label="Send API key"><Toggle on={draft.ai.openai_compatible.use_api_key} onChange={(v) => update((d) => { d.ai.openai_compatible.use_api_key = v; })} /></Field>
          {draft.ai.openai_compatible.use_api_key && <SecretField name="openai_compatible_api_key" label="API key" hint="Bearer token for the server." />}
        </Card>
      )}

      <Card title="Behaviour">
        <Field label="Custom instructions" hint="Added to every conversation." stack>
          <textarea className="textarea" value={draft.ai.custom_instructions} onChange={(e) => update((d) => { d.ai.custom_instructions = e.target.value; })} placeholder="e.g. Answer in British English. Prefer short answers." />
        </Field>
        <Field label="Max tool steps per message" hint="API modes only.">
          <input className="input" type="number" min={1} max={50} value={draft.ai.max_tool_iterations} onChange={(e) => update((d) => { d.ai.max_tool_iterations = Number(e.target.value) || 12; })} style={{ width: 90 }} />
        </Field>
      </Card>
    </>
  );
}
