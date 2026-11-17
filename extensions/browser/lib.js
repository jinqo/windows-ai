// Pure helpers shared by the background worker and popup (unit-tested).

export const HOST = "com.windowsai.bridge";
export const MAX_CHARS = 100000;

export const SELECTION_ACTIONS = [
  { id: "ask", title: "Ask Windows AI…" },
  { id: "explain", title: "Explain" },
  { id: "summarize", title: "Summarize" },
  { id: "rewrite", title: "Rewrite" },
  { id: "translate", title: "Translate" },
  { id: "improve", title: "Improve" },
];

/** Collapses whitespace runs and trims, keeping paragraph breaks. */
export function cleanText(text) {
  return String(text ?? "")
    .replace(/\r/g, "")
    .replace(/[ \t\f\v]+/g, " ")
    .replace(/\n\s*\n\s*\n+/g, "\n\n")
    .trim();
}

export function truncate(text, max = MAX_CHARS) {
  return text.length > max ? { text: text.slice(0, max), truncated: true } : { text, truncated: false };
}

function looksLikeCode(text) {
  const lines = text.split("\n");
  const hits = lines.filter((l) => /[{};]|=>|\bfunction\b|\bdef\b|\bclass\b|^\s{4}/.test(l)).length;
  return lines.length > 1 && hits / lines.length > 0.5;
}

/** Capture payload for selected text. */
export function selectionCapture({ text, title, url, action }) {
  const cleaned = String(text ?? "").trim();
  const { text: content, truncated } = truncate(cleaned);
  return {
    source: "browser",
    kind: looksLikeCode(content) ? "code" : "text",
    title: title || null,
    url: url || null,
    content,
    truncated,
    action: action && action !== "ask" ? action : null,
    method: "browser_selection",
  };
}

/** Capture payload for a whole page. */
export function pageCapture({ text, title, url, action, question }) {
  const { text: content, truncated } = truncate(cleanText(text));
  return {
    source: "browser",
    kind: "page",
    title: title || null,
    url: url || null,
    content,
    truncated,
    action: action || null,
    question: question || null,
    method: "browser_page",
  };
}

export function isRestrictedUrl(url) {
  return !url || /^(chrome|edge|about|chrome-extension|devtools|view-source):/i.test(url) || /chromewebstore\.google\.com|microsoftedge\.microsoft\.com\/addons/i.test(url);
}

/** Runs inside the page (injected); returns readable text. */
export function extractPageInPage() {
  const pick = document.querySelector("article, main, [role=main]") || document.body;
  const clone = pick.cloneNode(true);
  clone.querySelectorAll("script, style, noscript, svg, nav, footer, iframe, [aria-hidden=true]").forEach((n) => n.remove());
  return { title: document.title, url: location.href, text: clone.innerText || "" };
}

export function friendlyError(message) {
  const m = String(message || "");
  if (/native messaging host not found|Specified native messaging host not found/i.test(m)) {
    return "Windows AI is not connected. Open Windows AI → Settings → Integrations → Chrome & Edge → Register.";
  }
  if (/Access to the specified native messaging host is forbidden/i.test(m)) {
    return "This extension ID is not allowed by Windows AI. Re-register the browser connection in Windows AI settings.";
  }
  return m || "Unknown error";
}
