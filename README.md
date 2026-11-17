# Windows AI

A native-feeling AI assistant for Windows 11. Press **Alt+Space** anywhere to open a floating overlay, chat with Claude, and let it work with your PC through permission-checked tools. Select text in any app and press **Ctrl+Shift+Space** to explain, rewrite, summarize or translate it. Right-click files in Explorer, pages in Chrome/Edge, or code in VS Code to send them to the same assistant.

## Features

- **Floating overlay**: always-on-top, frameless, dark charcoal + violet glass design; tray icon; close hides to tray; Esc hides; launch on sign-in.
- **Claude via your subscription**: uses the official **Claude Code CLI** you’re signed in to. No API key needed, no API billing. Streaming, conversation continuity, cancellation, usage-limit notices.
- **Free models**: one-click presets for free API tiers (Groq, Google Gemini, OpenRouter `:free` models) and local models (Ollama, LM Studio). No Claude account needed.
- **Optional API mode**: Anthropic API key (billed per use).
- **Local memory**: conversations, memories (preferences, facts, notes), projects — all in a local SQLite database with full-text search, editing and deletion.
- **Windows tools**: search/list/read files in approved folders, create files, launch approved apps, open URLs, system info, run allow-listed commands, manage settings, plan multi-step tasks.
- **Permission system**: per-tool Allow / Ask / Deny, folder read/write scopes, approval dialogs that describe the exact action, execution history.
- **MCP**: connect stdio MCP servers; discovered tools are untrusted until you trust them.
- **Voice**: push-to-talk (Windows speech recognition or Whisper API), spoken replies with local Windows voices.
- **Automation**: assistant-drafted task plans you review and run step by step, with retry/skip/cancel.
- **Integrations**: universal selection capture + floating toolbar, File Explorer right-click menu, Chrome/Edge extension, VS Code extension. You always see exactly what will be shared before sending.

## Requirements

- Windows 10 21H2+ / Windows 11 with Microsoft Edge WebView2 (preinstalled on Windows 11)
- For subscription mode: [Claude Code](https://code.claude.com/docs/en/setup) and a Claude Pro or Max plan
- To build: Node.js 20+, Rust stable, and either MinGW-w64 (GNU target, with `windres`) or Visual Studio Build Tools + Windows SDK (MSVC target)

## Install & run

### One command (PowerShell)

```powershell
irm https://raw.githubusercontent.com/jinqo/windows-ai/main/install.ps1 | iex
```

Downloads the latest release installer, verifies its SHA-256 checksum, installs per-user (no admin) and starts Windows AI.

### From the installer
Download `Windows AI_<version>_x64-setup.exe` from Releases and run it. Windows AI starts in the tray; press **Alt+Space**.

### From source

```bash
npm install
```

The Rust build needs a short target directory if the project lives in a deep folder (Windows MAX_PATH):

```bash
export CARGO_TARGET_DIR="$(cygpath -w "$TEMP")\\wai-target"
```

```bash
npx tauri dev
```

Release build (exe + NSIS installer under `$CARGO_TARGET_DIR/release/bundle/nsis/`):

```bash
npx tauri build
```

## Connecting Claude

### Subscription mode (default)

1. Install Claude Code (PowerShell): `irm https://claude.ai/install.ps1 | iex`
2. Open Windows AI → **Settings → AI providers** → **Sign in with Claude**. This opens Claude Code’s own official sign-in (`claude auth login`) in a console window and your browser. Windows AI never sees your password or tokens.
3. The panel shows *Signed in with your Claude … subscription — billing: subscription usage limits*.

How it works: each request runs `claude -p --output-format stream-json` as a child process. Windows AI:
- removes all `ANTHROPIC_*` / `CLAUDE_*` environment variables from the child, so an API key in your environment can never switch you to paid API billing;
- refuses to run if Claude Code is signed in with an API (Console) account, unless you explicitly allow API billing;
- disables Claude Code’s own built-in tools and instead offers Windows AI’s permission-checked tools over a private, per-request MCP connection;
- resumes the same Claude Code session for follow-up messages.

Usage counts against your Claude plan’s limits. When you approach or hit a limit, the chat shows when it resets.

**Free fallback (FreeLLMAPI).** If Claude can't answer (usage limit, signed out, Claude Code missing), Windows AI can answer with free models through a local [FreeLLMAPI](https://github.com/tashfeenahmed/freellmapi) router. It uses `fusion` first (several free models answer in parallel and a judge merges them), then `auto`. The chat always shows *Claude unavailable … Answering with FreeLLMAPI · fusion*. It's on only when a FreeLLMAPI key is stored (Settings → AI providers → Free fallback) or `FREELLM_API_KEY` is set. Default router URL: `http://127.0.0.1:31415/v1` (desktop app); Docker installs use `http://localhost:3001/v1`.

> **Important — subscription mode and Anthropic's terms.** Windows AI is an independent, unofficial project, not made or endorsed by Anthropic. Subscription mode runs the official Claude Code CLI on your own machine with your own login; Windows AI never handles your credentials. Anthropic's terms restrict third-party products from offering Claude.ai subscription access without approval, so **you** are responsible for making sure your use complies with the [Claude Code legal and compliance terms](https://code.claude.com/docs/en/legal-and-compliance) and Anthropic's Consumer/Commercial Terms. If in doubt, use **API mode** with your own API key.

### Free / local models (no Claude account)
Settings → AI providers → **Free / local** → pick a preset:

| Preset | Cost | Key |
|---|---|---|
| FreeLLMAPI | Free, local router over many free tiers | unified `freellmapi-…` key from the app |
| Ollama | Free, runs on your PC | none ([download](https://ollama.com/download), then `ollama pull llama3.2`) |
| LM Studio | Free, runs on your PC | none |
| Groq | Free tier, rate limited | [console.groq.com/keys](https://console.groq.com/keys) |
| Google Gemini | Free tier, daily limits | [aistudio.google.com/apikey](https://aistudio.google.com/apikey) |
| OpenRouter | Models ending in `:free` | [openrouter.ai/keys](https://openrouter.ai/keys) |

Paste the key (stored in Windows Credential Manager), **Save**, then **Load models** to pick from the provider's live list. Models without function calling still chat; Windows tools are skipped for them. Free tiers are rate limited and your messages are processed under that provider's terms.

### API mode (optional)
Settings → AI providers → **API key**. Paste a key from console.anthropic.com (stored in Windows Credential Manager). Billed per token to that Console account.

## Integrations setup

| Integration | Setup |
|---|---|
| Selected text (any app) | On by default. Select text → **Ctrl+Shift+Space** → toolbar appears at the cursor (or the full assistant — configurable). |
| File Explorer | Settings → Integrations → File Explorer → **Install**. Right-click files/folders → *Windows AI* (classic menu: *Show more options*). For the Windows 11 top-level menu, register the signed sparse package: `shell-ext/package/register.ps1` (see below). |
| Chrome / Edge | Settings → Integrations → Chrome & Edge → **Register**. Then `chrome://extensions` (or `edge://extensions`) → Developer mode → **Load unpacked** → `extensions/browser`. |
| VS Code | `code --install-extension extensions/vscode/windows-ai-vscode-0.1.0.vsix`. Right-click in the editor → *Windows AI*, or **Ctrl+Alt+A**. |

All integrations deliver content into the same assistant, conversations, settings and memory. Content is sent to the AI only after you press Send (quick actions from the toolbar send immediately after you click them).

## Permissions & safety

- File tools work only in folders you approve (read and write separately).
- `write_file`, `run_command` and saving memories always ask first; you can never set them to auto-allow.
- Commands: only exact, allow-listed program + arguments, by name. There is no free-form shell.
- Every tool call is logged in **Activity** with input, result, duration and status.
- MCP tools must be trusted one by one.
- Windows AI runs without administrator rights and only writes per-user registry keys (Explorer menu, browser host, autostart). The uninstaller removes them.

## Configuration

Settings are stored in `%APPDATA%\com.windowsai.app\config.json` (see `config.example.json`). Secrets are stored in Windows Credential Manager, never in the config file. Data lives in `%APPDATA%\com.windowsai.app\windows-ai.db`.

## License

MIT — see [LICENSE](LICENSE). "Claude" and "Claude Code" are trademarks of Anthropic; this project is not affiliated with Anthropic.

## Testing

```bash
npm run typecheck
```

```bash
npm test
```

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Live subscription tests (use real Claude Code + a small amount of your plan usage):

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib live_ -- --ignored --test-threads=1
```

## Windows 11 top-level context menu

`shell-ext/` contains an `IExplorerCommand` handler DLL (`windows_ai_shell.dll`, shipped next to `windows-ai.exe`) and a sparse MSIX manifest that gives it package identity. Windows only accepts **signed** packages:

1. Install the Windows SDK (for `makeappx` / `signtool`).
2. Build and sign with a code-signing certificate whose subject becomes the package Publisher (`signing.json`):
   `shell-ext/package/package.ps1 -PfxPath codesign.pfx -PfxPassword (Read-Host -AsSecureString)`
3. Register for the current user: `shell-ext/package/register.ps1`

### Signing status

Release builds are **not code-signed yet**, so Windows SmartScreen warns on first run (*More info → Run anyway*). The pipeline is ready: add a trusted code-signing certificate as repo secrets `MSIX_PFX_BASE64` + `MSIX_PFX_PASSWORD`, put its exact Subject in `shell-ext/package/signing.json` → `profiles.release.publisher`, and the release workflow signs the installer and builds the signed MSIX automatically. Free/cheap options for open-source projects: [SignPath Foundation](https://signpath.org) (free for OSS), Certum Open Source Code Signing, or Azure Artifact Signing.

For your own PC only, a self-signed dev certificate works: `new-dev-cert.ps1`, then `trust-dev-cert.ps1` in an **elevated** PowerShell, then `package.ps1` and `register.ps1`.

## Known limitations

- **Windows 11 modern context menu** requires the signed sparse package above. Without it, entries appear under “Show more options”.
- **Selection capture** cannot read elevated (admin) windows (UIPI) or apps that block copy. The clipboard fallback may show up in Windows clipboard history (Win+V).
- **Windows speech recognition** needs a microphone and *Settings → Privacy & security → Speech → Online speech recognition*.
- **Whisper STT** and **API mode** need their own API keys and are billed by those providers.
- In subscription mode, model, effort and session behaviour follow what the installed Claude Code CLI supports.
- The browser extension is loaded unpacked (not published to a store); its ID is fixed by the key in `manifest.json`.
