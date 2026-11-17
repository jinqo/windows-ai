import { describe, expect, it } from "vitest";
import { applyStream, newRun } from "./stream";
import { actionsFor, contextTitle, parseUserMessage } from "./context";
import { speakableText } from "./voice";
import { relativeTime, formatBytes } from "./format";
import { acceleratorFromEvent } from "../features/settings/sections/ShortcutsSection";
import type { CapturedContext, ChatStreamPayload } from "./types";

const base = { run_id: "r1", conversation_id: "c1" };

describe("stream reducer", () => {
  it("accumulates text, thinking and tool status", () => {
    let run = newRun("r1", "c1");
    const events: ChatStreamPayload[] = [
      { ...base, type: "delta", event: { type: "thinking_delta", text: "plan " } },
      { ...base, type: "delta", event: { type: "text_delta", text: "Hel" } },
      { ...base, type: "delta", event: { type: "text_delta", text: "lo" } },
      { ...base, type: "tool_running", tool_use_id: "t1", name: "read_file" },
      { ...base, type: "tool_result", tool_use_id: "t1", name: "read_file", status: "denied", is_error: true, content: "The user denied this action." },
    ];
    for (const e of events) run = applyStream(run, e);
    expect(run.text).toBe("Hello");
    expect(run.thinking).toBe("plan ");
    expect(run.tools).toEqual([{ id: "t1", name: "read_file", status: "denied", content: "The user denied this action." }]);
  });

  it("ignores other runs and records errors", () => {
    let run = newRun("r1", "c1");
    run = applyStream(run, { run_id: "other", conversation_id: "c1", type: "delta", event: { type: "text_delta", text: "x" } });
    expect(run.text).toBe("");
    run = applyStream(run, { ...base, type: "error", error: { kind: "missing_credential", message: "sign in" } });
    expect(run.done).toBe(true);
    expect(run.errorKind).toBe("missing_credential");
  });

  it("clears the buffer when the assistant message is persisted and shows usage notices", () => {
    let run = applyStream(newRun("r1", "c1"), { ...base, type: "delta", event: { type: "text_delta", text: "abc" } });
    run = applyStream(run, { ...base, type: "usage", status: "allowed_warning", message: "close to limit" });
    run = applyStream(run, { ...base, type: "message", message: { id: "m", conversation_id: "c1", role: "assistant", content: [], created_at: "" } });
    expect(run.text).toBe("");
    expect(run.usageNotice).toBe("close to limit");
  });
});

describe("captured context", () => {
  it("parses context blocks out of a stored user message", () => {
    const raw = `<captured_context source="browser" kind="page" title="A &quot;B&quot;" url="https://x.dev">\nPage body\n</captured_context>\n\nSummarize the captured web page.\n\nfocus on pricing`;
    const p = parseUserMessage(raw);
    expect(p.contexts).toHaveLength(1);
    expect(p.contexts[0].attrs).toMatchObject({ source: "browser", kind: "page", title: 'A "B"', url: "https://x.dev" });
    expect(p.contexts[0].body).toBe("Page body");
    expect(p.text).toBe("Summarize the captured web page.\n\nfocus on pricing");
  });

  it("offers actions by kind and titles captures", () => {
    expect(actionsFor("code").map((a) => a.id)).toContain("refactor");
    expect(actionsFor("text").map((a) => a.id)).not.toContain("refactor");
    const c: CapturedContext = { id: "1", source: "explorer", kind: "files", files: [{ path: "C:\\a\\b.txt", size: 1, is_dir: false, included: true }, { path: "C:\\a\\c.txt", size: 1, is_dir: false, included: true }], content: "", captured_at: "", truncated: false };
    expect(contextTitle(c)).toBe("b.txt +1 more");
  });
});

describe("helpers", () => {
  it("makes markdown speakable", () => {
    expect(speakableText("# Title\nUse `npm i` and **bold**\n```js\nx()\n```")).toBe("Title Use npm i and bold (code omitted)");
  });
  it("formats time and bytes", () => {
    const now = Date.parse("2026-01-01T12:00:00Z");
    expect(relativeTime("2026-01-01T11:58:00Z", now)).toBe("2m ago");
    expect(formatBytes(2048)).toBe("2.0 KB");
  });
  it("builds accelerators from key events", () => {
    expect(acceleratorFromEvent({ ctrlKey: true, shiftKey: true, altKey: false, metaKey: false, key: " ", code: "Space" })).toBe("Ctrl+Shift+Space");
    expect(acceleratorFromEvent({ ctrlKey: false, shiftKey: false, altKey: true, metaKey: false, key: "k", code: "KeyK" })).toBe("Alt+K");
    expect(acceleratorFromEvent({ ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, key: "k", code: "KeyK" })).toBeNull();
    expect(acceleratorFromEvent({ ctrlKey: true, shiftKey: false, altKey: false, metaKey: false, key: "Control", code: "ControlLeft" })).toBeNull();
  });
});
