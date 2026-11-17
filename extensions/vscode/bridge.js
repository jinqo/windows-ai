// Authenticated client for the Windows AI local bridge (named pipe).
// Pure helpers are exported for tests; no VS Code APIs here.
const fs = require("fs");
const net = require("net");
const path = require("path");

function bridgeDir(env = process.env) {
  return path.join(env.LOCALAPPDATA || require("os").tmpdir(), "WindowsAI");
}

function pipeName(env = process.env) {
  const user = String(env.USERNAME || "user").replace(/[^A-Za-z0-9_-]/g, "");
  return `\\\\.\\pipe\\windows-ai-bridge-${user}`;
}

function readToken(env = process.env) {
  try {
    return fs.readFileSync(path.join(bridgeDir(env), "bridge.token"), "utf8").trim() || null;
  } catch {
    return null;
  }
}

/** Builds a capture payload for code / diagnostics. */
function buildCapture({ kind = "code", content, language, filePath, title, action, includePath = true, maxChars = 100000 }) {
  const text = String(content ?? "");
  const truncated = text.length > maxChars;
  return {
    source: "vscode",
    kind,
    content: truncated ? text.slice(0, maxChars) : text,
    truncated,
    language: language || null,
    file_path: includePath ? filePath || null : null,
    title: title || (filePath ? path.basename(filePath) : null),
    app: "Visual Studio Code",
    action: action && action !== "ask" ? action : null,
    method: "vscode_extension",
  };
}

const SEVERITY = ["Error", "Warning", "Information", "Hint"];

/** Formats diagnostics with the offending source lines for context. */
function formatDiagnostics(diags, lines) {
  return diags
    .map((d) => {
      const line = d.range.start.line;
      const src = lines[line] !== undefined ? `\n    ${line + 1} | ${lines[line]}` : "";
      const code = d.code ? ` [${typeof d.code === "object" ? d.code.value : d.code}]` : "";
      return `${SEVERITY[d.severity] ?? "Issue"} at line ${line + 1}, col ${d.range.start.character + 1}${code} (${d.source ?? "unknown"}): ${d.message}${src}`;
    })
    .join("\n\n");
}

/** Sends one request over the pipe and resolves with the JSON response. */
function request(req, { env = process.env, timeoutMs = 10000 } = {}) {
  return new Promise((resolve, reject) => {
    const token = readToken(env);
    if (!token) return reject(new Error("Windows AI is not running (start it from the Start menu)."));
    const sock = net.createConnection(pipeName(env));
    let buf = "";
    const timer = setTimeout(() => {
      sock.destroy();
      reject(new Error("Windows AI did not respond."));
    }, timeoutMs);
    sock.on("connect", () => sock.write(JSON.stringify({ ...req, token }) + "\n"));
    sock.on("data", (d) => {
      buf += d.toString("utf8");
      const i = buf.indexOf("\n");
      if (i >= 0) {
        clearTimeout(timer);
        sock.end();
        try {
          resolve(JSON.parse(buf.slice(0, i)));
        } catch (e) {
          reject(e);
        }
      }
    });
    sock.on("error", (e) => {
      clearTimeout(timer);
      reject(new Error(e.code === "ENOENT" ? "Windows AI is not running." : e.message));
    });
  });
}

async function sendCapture(payload, opts) {
  const r = await request({ type: "capture", payload }, opts);
  if (!r || !r.ok) throw new Error(r?.error || "Windows AI rejected the request");
  return r;
}

module.exports = { bridgeDir, pipeName, readToken, buildCapture, formatDiagnostics, request, sendCapture };
