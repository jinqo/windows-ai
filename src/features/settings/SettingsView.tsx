import { useEffect, useState } from "react";
import { Bot, Brain, Keyboard, Mic, Palette, Plug, Power, Server, Shield } from "lucide-react";
import { useApp } from "../../store/app";
import type { AppConfig } from "../../lib/types";
import { AppearanceSection } from "./sections/AppearanceSection";
import { ProvidersSection } from "./sections/ProvidersSection";
import { MemorySettingsSection } from "./sections/MemorySettingsSection";
import { VoiceSection } from "./sections/VoiceSection";
import { ShortcutsSection } from "./sections/ShortcutsSection";
import { StartupSection } from "./sections/StartupSection";
import { PermissionsSection } from "./sections/PermissionsSection";
import { McpSection } from "./sections/McpSection";
import { IntegrationsSection } from "./sections/IntegrationsSection";

export interface SectionProps {
  draft: AppConfig;
  update: (fn: (c: AppConfig) => void) => void;
}

const SECTIONS = [
  { id: "providers", label: "AI providers", icon: Bot, C: ProvidersSection },
  { id: "integrations", label: "Integrations", icon: Plug, C: IntegrationsSection },
  { id: "permissions", label: "Permissions", icon: Shield, C: PermissionsSection },
  { id: "mcp", label: "MCP servers", icon: Server, C: McpSection },
  { id: "memory", label: "Memory", icon: Brain, C: MemorySettingsSection },
  { id: "voice", label: "Voice", icon: Mic, C: VoiceSection },
  { id: "appearance", label: "Appearance", icon: Palette, C: AppearanceSection },
  { id: "shortcuts", label: "Shortcuts", icon: Keyboard, C: ShortcutsSection },
  { id: "startup", label: "Startup", icon: Power, C: StartupSection },
] as const;

export function SettingsView() {
  const config = useApp((s) => s.config);
  const saveConfig = useApp((s) => s.saveConfig);
  const [section, setSection] = useState<(typeof SECTIONS)[number]["id"]>("providers");
  const [draft, setDraft] = useState<AppConfig | null>(config);
  const [saving, setSaving] = useState(false);

  useEffect(() => setDraft(config), [config]);
  if (!draft || !config) return <div className="empty">Loading settings…</div>;

  const dirty = JSON.stringify(draft) !== JSON.stringify(config);
  const update = (fn: (c: AppConfig) => void) =>
    setDraft((d) => {
      const copy = structuredClone(d!) as AppConfig;
      fn(copy);
      return copy;
    });
  const Active = SECTIONS.find((s) => s.id === section)!.C;

  return (
    <div className="page">
      <nav className="page-nav" aria-label="Settings sections">
        {SECTIONS.map(({ id, label, icon: Icon }) => (
          <button key={id} className={section === id ? "active" : ""} onClick={() => setSection(id)}>
            <Icon size={15} />
            <span>{label}</span>
          </button>
        ))}
      </nav>
      <div className="page-main">
        <Active draft={draft} update={update} />
        {dirty && (
          <div className="row" style={{ position: "sticky", bottom: 0, padding: "10px 0", background: "linear-gradient(transparent, rgba(18,17,23,0.95) 30%)", justifyContent: "flex-end" }}>
            <span className="muted small">Unsaved changes</span>
            <button className="btn" onClick={() => setDraft(config)}>Discard</button>
            <button
              className="btn primary"
              disabled={saving}
              onClick={async () => {
                setSaving(true);
                await saveConfig(draft);
                setSaving(false);
              }}
            >
              Save changes
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
