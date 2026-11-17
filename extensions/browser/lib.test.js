import { describe, expect, it } from "vitest";
import { cleanText, friendlyError, isRestrictedUrl, pageCapture, selectionCapture, MAX_CHARS } from "./lib.js";
import { readFileSync } from "node:fs";
import { join } from "node:path";

describe("browser extension helpers", () => {
  it("builds selection captures labelled as browser content", () => {
    const c = selectionCapture({ text: "  Hello world  ", title: "Docs", url: "https://x.dev", action: "explain" });
    expect(c).toMatchObject({ source: "browser", kind: "text", content: "Hello world", title: "Docs", url: "https://x.dev", action: "explain", truncated: false });
    expect(selectionCapture({ text: "x", action: "ask" }).action).toBeNull();
    expect(selectionCapture({ text: "function a() {\n  return 1;\n}" }).kind).toBe("code");
  });

  it("cleans and truncates pages", () => {
    const c = pageCapture({ text: "a   b\n\n\n\nc" + "x".repeat(MAX_CHARS), title: "T", url: "https://y", action: "summarize" });
    expect(c.kind).toBe("page");
    expect(c.content.startsWith("a b\n\nc")).toBe(true);
    expect(c.content.length).toBe(MAX_CHARS);
    expect(c.truncated).toBe(true);
    expect(cleanText("  a\t\tb  ")).toBe("a b");
  });

  it("recognizes restricted pages and maps native-host errors", () => {
    expect(isRestrictedUrl("chrome://settings")).toBe(true);
    expect(isRestrictedUrl("edge://flags")).toBe(true);
    expect(isRestrictedUrl("https://example.com")).toBe(false);
    expect(friendlyError("Specified native messaging host not found.")).toMatch(/Register/);
  });

  it("manifest pins the extension ID used by the native host", () => {
    const m = JSON.parse(readFileSync(join(__dirname, "manifest.json"), "utf8"));
    expect(m.manifest_version).toBe(3);
    expect(m.permissions).toContain("nativeMessaging");
    expect(m.permissions).not.toContain("<all_urls>");
    expect(m.key.length).toBeGreaterThan(300);
  });
});
