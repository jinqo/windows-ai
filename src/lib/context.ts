// Helpers for captured context: labels, previews and parsing of the
// <captured_context> blocks stored inside user messages.
import type { CapturedContext } from "./types";

export const SOURCE_LABEL: Record<string, string> = {
  selection: "Selected text",
  clipboard: "Clipboard",
  explorer: "File Explorer",
  browser: "Browser",
  vscode: "VS Code",
  cli: "Command line",
};

export const KIND_LABEL: Record<string, string> = {
  text: "Text",
  code: "Code",
  files: "Files",
  page: "Web page",
  diagnostics: "Errors",
};

export const QUICK_ACTIONS: { id: string; label: string; kinds?: string[] }[] = [
  { id: "explain", label: "Explain" },
  { id: "summarize", label: "Summarize" },
  { id: "rewrite", label: "Rewrite", kinds: ["text", "page"] },
  { id: "improve", label: "Improve", kinds: ["text", "code"] },
  { id: "translate", label: "Translate", kinds: ["text", "page"] },
  { id: "analyze", label: "Analyze", kinds: ["files", "page", "diagnostics"] },
  { id: "debug", label: "Debug", kinds: ["code", "diagnostics"] },
  { id: "refactor", label: "Refactor", kinds: ["code"] },
  { id: "document", label: "Document", kinds: ["code"] },
];

export function actionsFor(kind: string) {
  return QUICK_ACTIONS.filter((a) => !a.kinds || a.kinds.includes(kind));
}

export function contextTitle(c: CapturedContext): string {
  if (c.kind === "files" && c.files.length) {
    const first = c.files[0].path.split(/[\\/]/).pop() ?? c.files[0].path;
    return c.files.length > 1 ? `${first} +${c.files.length - 1} more` : first;
  }
  return c.title || c.file_path?.split(/[\\/]/).pop() || c.app || SOURCE_LABEL[c.source] || "Captured content";
}

export function formatSize(chars: number): string {
  if (chars < 1000) return `${chars} chars`;
  return `${(chars / 1000).toFixed(chars < 10000 ? 1 : 0)}k chars`;
}

export interface ParsedContextBlock { attrs: Record<string, string>; body: string }
export interface ParsedUserMessage { contexts: ParsedContextBlock[]; text: string }

const unescapeAttr = (v: string) => v.replace(/&quot;/g, '"').replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");

/** Splits a stored user message into captured-context blocks and the typed text. */
export function parseUserMessage(raw: string): ParsedUserMessage {
  const contexts: ParsedContextBlock[] = [];
  const re = /<captured_context([^>]*)>\n?([\s\S]*?)\n?<\/captured_context>/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(raw))) {
    const attrs: Record<string, string> = {};
    for (const a of m[1].matchAll(/(\w+)="([^"]*)"/g)) attrs[a[1]] = unescapeAttr(a[2]);
    contexts.push({ attrs, body: m[2].replace(/<\\\/captured_context/g, "</captured_context") });
  }
  const text = raw.replace(re, "").trim();
  return { contexts, text };
}
