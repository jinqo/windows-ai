# Launch posts (drafts, not committed)

Attach `docs/screenshot.png`, or better a 10–20 s screen recording of: Alt+Space → ask, select text → Ctrl+Shift+Space → Explain, right-click file → Windows AI. Video gets far more clicks than a still.

Before posting: the installer is not code-signed yet, so say SmartScreen will warn. Hiding it gets "this is malware" comments.

---

## Show HN (news.ycombinator.com/submit)

**Title:** Show HN: Windows AI – an Alt+Space AI overlay for Windows that works with free models

**URL:** https://github.com/jinqo/windows-ai

**First comment:**
I wanted a Spotlight-style AI on Windows that can actually work with what's on my screen, so I built one (Tauri 2 + Rust + React).

- Alt+Space opens a floating chat from anywhere
- Select text in any app, press Ctrl+Shift+Space → explain / rewrite / summarize / translate. It uses UI Automation first and only falls back to a clipboard copy (snapshotted and restored), and never monitors the clipboard
- Right-click files in Explorer (Win11 top-level menu via a sparse MSIX), plus Chrome/Edge and VS Code extensions
- Tools are permission-gated: folder scopes, per-tool Allow/Ask/Deny, approval dialogs, activity log. No free-form shell
- Models: free/local (FreeLLMAPI router, Ollama, LM Studio, Groq, Gemini, OpenRouter), the Anthropic API, or the official Claude Code CLI if you have it installed
- Everything stays local: SQLite memory with full-text search, keys in Windows Credential Manager

MIT licensed. The installer isn't code-signed yet, so SmartScreen will warn ("More info → Run anyway"). Feedback welcome, especially on the selection capture.

---

## Reddit — r/windows, r/Windows11, r/software

**Title:** I made a free, open-source AI overlay for Windows: Alt+Space anywhere, explain any selected text, right-click files in Explorer

**Body:**
Windows AI is a small Tauri app that sits in the tray:

- **Alt+Space**: floating chat over whatever you're doing
- **Select text anywhere + Ctrl+Shift+Space**: a toolbar pops up at your cursor with Explain / Rewrite / Summarize / Translate
- **Right-click files**: "Windows AI" in the Explorer menu
- Chrome/Edge + VS Code extensions
- Runs on **free models** (Ollama locally, or free API tiers like Groq/Gemini/OpenRouter via presets) — no subscription needed
- Asks before touching files or running anything; everything is logged

Install (PowerShell, per-user, no admin):
`irm https://raw.githubusercontent.com/jinqo/windows-ai/main/install.ps1 | iex`

or grab the installer: https://github.com/jinqo/windows-ai/releases/latest

Heads-up: not code-signed yet, so SmartScreen warns. Source is all on GitHub (MIT).

---

## Reddit — r/LocalLLaMA

**Title:** Windows overlay that turns any local/OpenAI-compatible model into a system-wide assistant (select text → explain, Explorer right-click, tools with approvals)

**Body:**
Built a Windows app that plugs any OpenAI-compatible endpoint into the OS: Alt+Space chat, select text in any app → explain/rewrite, Explorer right-click, browser + VS Code extensions, and permission-gated tools (file search/read inside approved folders, launch apps, etc.).

- Presets for Ollama, LM Studio, FreeLLMAPI, Groq, Gemini, OpenRouter, with a model picker that loads `/v1/models`
- If a model has no function calling, it retries without tools instead of erroring
- Optional fallback chain: primary backend fails or hits a limit → FreeLLMAPI `fusion` → `auto`, shown visibly in chat

Rust/Tauri, MIT: https://github.com/jinqo/windows-ai

---

## X / Bluesky

Built an open-source AI overlay for Windows 🪟

⌥ Alt+Space → chat anywhere
✂️ Select any text → explain / rewrite / translate
📁 Right-click files in Explorer
🆓 Works with free & local models (Ollama, Groq, Gemini, FreeLLMAPI)

One-line install, MIT: github.com/jinqo/windows-ai

---

## Notes
- Don't advertise it as "use your Claude Pro/Max subscription in another app". Anthropic's terms restrict third-party products offering Claude.ai subscription access, and subreddits like r/ClaudeAI may remove it. Lead with free/local models; mention Claude Code only as optional.
- Post HN on a weekday morning US Eastern time. Reply to every comment in the first 2 hours.
- Space subreddit posts a day apart; read each sub's self-promotion rules first.
