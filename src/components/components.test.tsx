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
