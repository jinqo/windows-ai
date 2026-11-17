# Windows AI — engineering notes

Desktop AI assistant for Windows 11: Tauri 2 (Rust) backend + React/TypeScript/Vite frontend, SQLite memory, permission-gated tools, MCP, voice, automation, and OS/app integrations.

## Architecture

```
React UI (src/)  ──invoke/events──▶  Tauri commands (src-tauri/src/commands/)
                                         │
     ┌───────────────┬───────────────────┼───────────────────┬──────────────┐
     ▼               ▼                   ▼                   ▼              ▼
  ai/ (agent,     tools/ (registry,   permissions/        memory/ +       integrations/
  providers)      executor, builtin)  (modes, scopes,     db.rs (SQLite)  (bridge, selection,
     │                 ▲              approvals)                          explorer, native host,
     │                 │                                                  mcp_stdio, clipboard)
     ├─ claude_code ───┘ (Windows tools offered to Claude Code over per-run MCP → bridge → executor)
     ├─ anthropic (Messages API, SSE)
     └─ openai_compat
```

- **One gateway for actions**: every tool call (chat, tasks, Claude Code via MCP) goes through `tools::executor::ToolHub::execute` → schema validation → permission mode → approval dialog → run → activity log.
- **Backends** (`config.ai.provider`):
  - `claude_code` (default): official Claude Code CLI as a child process (`ai/claude_code.rs`). Subscription auth stays inside the CLI. Built-in CLI tools disabled (`--tools ""`); Windows AI tools exposed via `windows-ai.exe --mcp-stdio` with a per-run token. Session continuity via `--session-id`/`--resume`, stored in `conversations.cli_session_id`.
  - Fallback (`ai.fallback`): when Claude Code fails (not on cancel), `agent::run_fallback` answers via FreeLLMAPI (`fusion`, then `auto`) through `OpenAiCompatProvider::with_key_slot(.., "freellmapi_api_key")`. Only when enabled and a key exists; emits a visible `fallback` stream event.
  - `anthropic`: raw HTTP Messages API with streaming, adaptive thinking, tool loop in `ai/agent.rs`. API key in Windows Credential Manager.
  - `openai_compatible`: free/local servers (Ollama, LM Studio, Groq, Gemini, OpenRouter). Presets in `src/lib/presets.ts`; retries without `tools` when the model lacks function calling.
- **Integrations** all produce a `CapturedContext` (source + kind + metadata) pushed to an in-memory `Inbox`; the UI shows exactly what will be shared before sending. `compose_prompt` wraps captures in `<captured_context>` blocks that the system prompt marks as data.
- **Bridge**: per-user named pipe `\\.\pipe\windows-ai-bridge-<USERNAME>`, remote clients rejected, every request needs the random token in `%LOCALAPPDATA%\WindowsAI\bridge.token` (rotated each launch). `capture` requests can only deliver context; `run_*` requests need a live per-run token.
- **Process modes** (`main.rs`): normal app; `chrome-extension://…` arg → native-messaging host; `--mcp-stdio` → MCP helper for Claude Code. Explorer verbs use `--context-action <action> --file "%1"` (forwarded via single-instance, batched 700 ms).

## Project structure

```
src/                      React app (main overlay + ?window=toolbar)
  lib/                    api.ts (typed invoke wrappers), types.ts, stream.ts, context.ts, voice.ts
  store/app.ts            zustand store
  features/chat|settings|memory|activity|toolbar
src-tauri/src/
  ai/                     agent.rs, claude_code.rs, anthropic.rs, openai_compat.rs, provider.rs, sse.rs, types.rs
  tools/                  mod.rs (Tool trait), executor.rs, schema.rs, builtin/*
  permissions/            approvals.rs, scope.rs
  memory/                 conversations, memories (FTS5), projects, sensitive.rs
  integrations/           bridge, context, selection (UIA + clipboard fallback), clipboard, explorer, native_host, mcp_stdio
  mcp/                    stdio JSON-RPC client + manager (trust gate)
  automation/             task store + runner (retry/skip/cancel)
  voice/                  Windows.Media.SpeechRecognition + Whisper upload
  config.rs secrets.rs db.rs activity.rs state.rs window.rs lib.rs main.rs
src-tauri/tests/fixtures/fake_claude.js   fake CLI for backend tests
extensions/browser        MV3 extension (Chrome/Edge), fixed key → stable ID
extensions/vscode         VS Code extension (plain JS) + .vsix
```

## Commands

The repo path is long; Rust build scripts fail past MAX_PATH. Always set a short target dir:

```bash
export CARGO_TARGET_DIR="$(cygpath -w "$TEMP")\\wai-target"   # Git Bash
npm install
npm run typecheck && npm test            # TS + vitest (UI, browser ext, VS Code ext)
cargo test --manifest-path src-tauri/Cargo.toml --lib            # Rust (fake CLI tests need node)
cargo test --manifest-path src-tauri/Cargo.toml --lib live_ -- --ignored --test-threads=1   # real Claude Code + subscription
npx tauri dev                            # run app
npx tauri build                          # release exe + NSIS installer
```

Toolchain: Rust `x86_64-pc-windows-gnu` with MinGW (windres used by build.rs to embed the app manifest). MSVC also supported by build.rs. Crate type is `rlib` only (GNU ld cannot export a Tauri cdylib).

## Security boundaries (do not weaken)

- No unrestricted shell. `run_command` only runs allow-listed program+args by name and is `locked_ask`.
- File tools only inside approved directories; read/write separated; paths canonicalized, `..` rejected.
- `write_file`, `run_command`, `remember` are `locked_ask` (cannot be set to Allow). Overwrites are labelled destructive.
- MCP tools are untrusted until the user trusts each one; trusted tools default to Ask.
- Claude Code mode: never read/copy CLI tokens or cookies; strip all `ANTHROPIC_*`/`CLAUDE*` env vars from the child (prevents silent API billing); refuse to run if `claude auth status` reports non-subscription billing unless `allow_api_billing`; never pass `--dangerously-skip-permissions`; never impersonate Claude Code.
- Secrets live only in Windows Credential Manager; the UI can set/clear but never read them.
- Memories with detected secrets are refused unless the user explicitly confirms.
- Selection capture runs only on the user's shortcut; clipboard is snapshotted and restored; never monitored.
- No admin rights requested (`asInvoker` manifest, HKCU-only registry).

## Coding standards

- Rust: domain logic in modules, thin `#[tauri::command]` wrappers; `AppResult<T>` with structured `AppError {kind, message}`; unit tests beside code. Keep pure functions testable without Tauri (`Notifier` trait abstracts events).
- TS: strict mode, all backend calls via `lib/api.ts`, shared types in `lib/types.ts`, pure logic in `lib/*` with tests.
- UI: CSS tokens in `styles/global.css`; reuse `components/ui`.

## Feature status

| Area | Status |
|---|---|
| Overlay, tray, Alt+Space, Esc, close-to-tray, autostart, positioning | Done |
| Chat streaming, Markdown, code copy, history, search, pin/rename/delete | Done |
| Claude subscription via Claude Code CLI (status, sign-in, stream, resume, cancel, limits) | Done, live-tested |
| Anthropic API provider / OpenAI-compatible provider + free presets | Done (unit-tested; no keys available for live test) |
| Memory (SQLite + FTS5), projects, sensitive-data guard | Done |
| Tools + permissions + approvals + activity log | Done |
| MCP client (stdio), trust gate | Done (tested with fake server) |
| Automation tasks (plan, run, retry, skip, cancel) | Done |
| Voice: Windows STT, Whisper STT, TTS | Done; Windows STT needs mic + Online speech recognition |
| Selection capture (UIA + clipboard fallback) + floating toolbar | Done |
| Explorer context menu (classic verbs, HKCU) | Done; Win11 top-level menu not supported (needs MSIX identity) |
| Chrome/Edge extension + native messaging host | Done |
| VS Code extension | Done |

## Decisions

- Claude Code CLI is the default backend because the user has a Claude subscription and no API key; subscription auth is only available through the official CLI.
- History is stored in the canonical content-block format so API providers can replay it; Claude Code conversations store final assistant text and resume CLI sessions (fallback: transcript replay if the CLI session is gone).
- Captured content is treated as untrusted data in prompts.
