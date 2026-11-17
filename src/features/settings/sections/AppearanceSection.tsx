import type { SectionProps } from "../SettingsView";
import { Card, Field, Segmented, Toggle } from "../../../components/ui";

export function AppearanceSection({ draft, update }: SectionProps) {
  const a = draft.appearance;
  const w = draft.window;
  return (
    <>
      <h2>Appearance</h2>
      <p className="desc">How the overlay looks and where it appears.</p>
      <Card title="Theme">
        <Field label="Accent colour">
          <Segmented value={a.accent} onChange={(v) => update((d) => { d.appearance.accent = v; })}
            options={[{ value: "violet", label: "Violet" }, { value: "purple", label: "Purple" }, { value: "indigo", label: "Indigo" }, { value: "fuchsia", label: "Fuchsia" }]} />
        </Field>
        <Field label="Background opacity" hint={`${Math.round(a.opacity * 100)}%`}>
          <input type="range" min={0.6} max={1} step={0.02} value={a.opacity} onChange={(e) => update((d) => { d.appearance.opacity = Number(e.target.value); })} />
        </Field>
        <Field label="Text size" hint={`${a.font_size}px`}>
          <input type="range" min={12} max={18} step={1} value={a.font_size} onChange={(e) => update((d) => { d.appearance.font_size = Number(e.target.value); })} />
        </Field>
        <Field label="Reduce motion"><Toggle on={a.reduce_motion} onChange={(v) => update((d) => { d.appearance.reduce_motion = v; })} /></Field>
      </Card>
      <Card title="Window">
        <Field label="Position when opened">
          <Segmented value={w.position} onChange={(v) => update((d) => { d.window.position = v; })}
            options={[{ value: "center", label: "Center" }, { value: "top", label: "Top" }, { value: "remember", label: "Remember" }]} />
        </Field>
        <Field label="Size" hint="Width × height in pixels.">
          <input className="input" type="number" min={420} value={w.width} onChange={(e) => update((d) => { d.window.width = Number(e.target.value) || 860; })} style={{ width: 90 }} />
          <input className="input" type="number" min={360} value={w.height} onChange={(e) => update((d) => { d.window.height = Number(e.target.value) || 640; })} style={{ width: 90 }} />
        </Field>
        <Field label="Always on top"><Toggle on={w.always_on_top} onChange={(v) => update((d) => { d.window.always_on_top = v; })} /></Field>
        <Field label="Hide when focus is lost" hint="Not while an approval is pending."><Toggle on={w.hide_on_blur} onChange={(v) => update((d) => { d.window.hide_on_blur = v; })} /></Field>
      </Card>
    </>
  );
}
