import type { SectionProps } from "../SettingsView";
import { Card, Field, Notice, Toggle } from "../../../components/ui";

export function MemorySettingsSection({ draft, update }: SectionProps) {
  const m = draft.memory;
  return (
    <>
      <h2>Memory</h2>
      <p className="desc">Memories and conversations are stored only on this PC in a local SQLite database.</p>
      <Notice kind="info">The assistant can only <i>propose</i> a memory; every save shows an approval dialog, and content that looks like a password, key or card number is refused.</Notice>
      <Card>
        <Field label="Use memories as context" hint="Include relevant saved memories with each request.">
          <Toggle on={m.auto_context} onChange={(v) => update((d) => { d.memory.auto_context = v; })} />
        </Field>
        <Field label="Memories per request">
          <input className="input" type="number" min={0} max={30} value={m.max_context_items} onChange={(e) => update((d) => { d.memory.max_context_items = Number(e.target.value); })} style={{ width: 80 }} />
        </Field>
        <Field label="Let the assistant propose memories"><Toggle on={m.allow_ai_save} onChange={(v) => update((d) => { d.memory.allow_ai_save = v; })} /></Field>
        <Field label="Conversation history sent" hint="Messages of history included per request (API modes; subscription mode resumes the Claude Code session).">
          <input className="input" type="number" min={2} max={400} value={m.history_window} onChange={(e) => update((d) => { d.memory.history_window = Number(e.target.value) || 40; })} style={{ width: 80 }} />
        </Field>
      </Card>
    </>
  );
}
