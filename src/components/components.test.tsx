import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

import { Markdown } from "../features/chat/Markdown";
import { ApprovalDialog } from "./ApprovalDialog";
import { MessageItem } from "../features/chat/Messages";
import { useApp } from "../store/app";

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
});

describe("Markdown", () => {
  it("renders code blocks with a language label and copy button", () => {
    render(<Markdown text={"Here:\n\n```rust\nfn main() {}\n```"} />);
    expect(screen.getByText("rust")).toBeInTheDocument();
    expect(screen.getByLabelText("Copy code")).toBeInTheDocument();
  });

  it("renders GFM tables", () => {
    render(<Markdown text={"| a | b |\n|---|---|\n| 1 | 2 |"} />);
    expect(screen.getByRole("table")).toBeInTheDocument();
  });
});

describe("ApprovalDialog", () => {
  it("shows the exact action and sends the user's decision", async () => {
    useApp.setState({
      approvals: [{ id: "a1", tool: "write_file", title: "t", description: "Create file C:\\x\\notes.txt (5 characters)", input: { path: "C:\\x\\notes.txt" }, access: "write", risk: "medium", source: "chat", allow_session: false }],
    });
    render(<ApprovalDialog />);
    expect(screen.getByText("Create file C:\\x\\notes.txt (5 characters)")).toBeInTheDocument();
    expect(screen.queryByText("Allow for this session")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("Allow once"));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("resolve_approval", { id: "a1", decision: "approve" }));
    await waitFor(() => expect(useApp.getState().approvals).toHaveLength(0));
  });
});

describe("MessageItem", () => {
  it("renders captured context as a labelled card, separate from the typed text", () => {
    useApp.setState({ config: null });
    const text = `<captured_context source="vscode" kind="code" language="ts" file="app.ts">\nconst x = 1;\n</captured_context>\n\nExplain the captured code clearly and concisely.`;
    render(<MessageItem message={{ id: "m", conversation_id: "c", role: "user", content: [{ type: "text", text }], created_at: "" }} results={new Map()} />);
    expect(screen.getByText("VS Code")).toBeInTheDocument();
    expect(screen.getByText("app.ts")).toBeInTheDocument();
    expect(screen.getByText("Explain the captured code clearly and concisely.")).toBeInTheDocument();
  });
});

import { Onboarding, applyChoice } from "../features/onboarding/Onboarding";
import type { AppConfig } from "../lib/types";

function baseConfig(): AppConfig {
  return {
    ai: {
      provider: "claude_code", custom_instructions: "", max_tool_iterations: 12,
      openai_compatible: { base_url: "http://localhost:11434/v1", model: "llama3.1", max_tokens: 4096, use_api_key: false },
      fallback: { enabled: true, base_url: "http://127.0.0.1:31415/v1", models: ["fusion", "auto"], max_tokens: 8192 },
    },
    startup: { launch_on_login: false, start_hidden: false, onboarded: false },
  } as unknown as AppConfig;
}

describe("first-run setup", () => {
  it("applies a free cloud preset", () => {
    const c = applyChoice(baseConfig(), "free", "groq");
    expect(c.ai.provider).toBe("openai_compatible");
    expect(c.ai.openai_compatible.base_url).toBe("https://api.groq.com/openai/v1");
    expect(c.ai.openai_compatible.use_api_key).toBe(true);
  });

  it("recommends free when Claude Code is missing, tests, then finishes", async () => {
    const saved: AppConfig[] = [];
    invoke.mockImplementation(async (cmd: string, args?: { config?: AppConfig }) => {
      if (cmd === "claude_code_status") return { installed: false, logged_in: false, billing: "unknown" };
      if (cmd === "secret_status") return { name: "x", stored: true, from_env: false };
      if (cmd === "save_config") { saved.push(args!.config!); return { warnings: [] }; }
      if (cmd === "provider_status") return { provider: "openai_compatible", model: "m", state: "connected", detail: "ok 12 models" };
      return undefined;
    });
    useApp.setState({ config: baseConfig() });
    render(<Onboarding />);
    await waitFor(() => expect(screen.getByRole("button", { name: /Free cloud model/ })).toHaveAttribute("aria-pressed", "true"));
    fireEvent.click(screen.getByText("Test"));
    await waitFor(() => expect(screen.getByText(/Connected\./)).toBeInTheDocument());
    expect(saved[0].ai.provider).toBe("openai_compatible");
    fireEvent.click(screen.getByText("Start"));
    await waitFor(() => expect(saved.at(-1)!.startup.onboarded).toBe(true));
  });

  it("is hidden once onboarded", () => {
    const c = baseConfig();
    c.startup.onboarded = true;
    useApp.setState({ config: c });
    const { container } = render(<Onboarding />);
    expect(container).toBeEmptyDOMElement();
  });
});
