import { describe, expect, it } from "vitest";
import { createRequire } from "node:module";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import net from "node:net";

const require = createRequire(import.meta.url);
const bridge = require("./bridge.js");

describe("VS Code bridge helpers", () => {
  it("builds code captures", () => {
    const c = bridge.buildCapture({ content: "fn main() {}", language: "rust", filePath: "C:\\p\\main.rs", action: "refactor", maxChars: 5 });
    expect(c).toMatchObject({ source: "vscode", kind: "code", language: "rust", file_path: "C:\\p\\main.rs", title: "main.rs", action: "refactor", truncated: true, content: "fn ma" });
    expect(bridge.buildCapture({ content: "x", action: "ask", includePath: false, filePath: "C:\\a" }).file_path).toBeNull();
  });

  it("formats diagnostics with source lines", () => {
    const out = bridge.formatDiagnostics(
      [{ severity: 0, message: "cannot find value `y`", source: "rustc", code: "E0425", range: { start: { line: 1, character: 4 } } }],
      ["fn a() {", "    y", "}"],
    );
    expect(out).toBe("Error at line 2, col 5 [E0425] (rustc): cannot find value `y`\n    2 |     y");
  });

  it("uses a per-user pipe name", () => {
    expect(bridge.pipeName({ USERNAME: "liam p" })).toBe("\\\\.\\pipe\\windows-ai-bridge-liamp");
  });

  it("round-trips a request with the token over a named pipe", async () => {
    const dir = mkdtempSync(join(tmpdir(), "wai-"));
    mkdirSync(join(dir, "WindowsAI"));
    writeFileSync(join(dir, "WindowsAI", "bridge.token"), "secret\n");
    const env = { LOCALAPPDATA: dir, USERNAME: `test${Date.now()}` };
    let seen;
    const server = net.createServer((s) => {
      s.on("data", (d) => {
        seen = JSON.parse(d.toString().trim());
        s.write(JSON.stringify({ ok: seen.token === "secret", id: "1" }) + "\n");
      });
    });
    await new Promise((r) => server.listen(bridge.pipeName(env), r));
    const resp = await bridge.request({ type: "capture", payload: { source: "vscode" } }, { env });
    server.close();
    expect(resp.ok).toBe(true);
    expect(seen.type).toBe("capture");
  });

  it("reports a missing app clearly", async () => {
    await expect(bridge.request({ type: "ping" }, { env: { LOCALAPPDATA: join(tmpdir(), "nope-wai"), USERNAME: "x" } })).rejects.toThrow(/not running/);
  });
});
