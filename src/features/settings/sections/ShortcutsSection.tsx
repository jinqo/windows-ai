import { useState } from "react";
import type { SectionProps } from "../SettingsView";
import { Card, Field } from "../../../components/ui";

/** Converts a keyboard event into a Tauri accelerator string, e.g. "Ctrl+Shift+Space". */
export function acceleratorFromEvent(e: Pick<KeyboardEvent, "ctrlKey" | "altKey" | "shiftKey" | "metaKey" | "key" | "code">): string | null {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return null;
  let key = e.code.startsWith("Key") ? e.code.slice(3) : e.code.startsWith("Digit") ? e.code.slice(5) : e.code;
  if (key === "Space" || e.key === " ") key = "Space";
  if (mods.length === 0 && !/^F\d+$/.test(key)) return null; // require a modifier (except F-keys)
  return [...mods, key].join("+");
}

function ShortcutInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [recording, setRecording] = useState(false);
  return (
    <button
      className={`btn ${recording ? "primary" : ""}`}
      onClick={() => setRecording(true)}
      onBlur={() => setRecording(false)}
      onKeyDown={(e) => {
        if (!recording) return;
        e.preventDefault();
        if (e.key === "Escape") return setRecording(false);
        const acc = acceleratorFromEvent(e.nativeEvent);
        if (acc) {
          onChange(acc);
          setRecording(false);
        }
      }}
    >
      {recording ? "Press keys…" : <span className="kbd">{value}</span>}
    </button>
  );
}

export function ShortcutsSection({ draft, update }: SectionProps) {
  return (
    <>
      <h2>Keyboard shortcuts</h2>
      <p className="desc">Global shortcuts work from any app. Click a shortcut, then press the new key combination.</p>
      <Card>
        <Field label="Show / hide Windows AI" hint="Default Alt+Space.">
          <ShortcutInput value={draft.shortcuts.toggle_overlay} onChange={(v) => update((d) => { d.shortcuts.toggle_overlay = v; })} />
        </Field>
        <Field label="Capture selected text" hint="Default Ctrl+Shift+Space. Configure behaviour under Integrations.">
          <ShortcutInput value={draft.integrations.selection_shortcut} onChange={(v) => update((d) => { d.integrations.selection_shortcut = v; })} />
        </Field>
        <Field label="Push-to-talk" hint="Hold while the overlay is focused."><span className="kbd">F8</span></Field>
        <Field label="Hide overlay"><span className="kbd">Esc</span></Field>
        <Field label="Send / new line"><span className="kbd">Enter</span> <span className="kbd">Shift+Enter</span></Field>
      </Card>
    </>
  );
}
