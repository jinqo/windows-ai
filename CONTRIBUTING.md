# Contributing

Thanks for helping! Bug reports, ideas and pull requests are all welcome.

## Setup

Requirements: Windows 10/11, Node.js 20+, Rust stable, and either MinGW-w64 (GNU target) or Visual Studio Build Tools (MSVC).

```bash
npm install
npx tauri dev
```

If the repo lives in a deep folder, set a short Cargo target dir (Windows MAX_PATH):

```bash
export CARGO_TARGET_DIR="$(cygpath -w "$TEMP")\wai-target"
```

## Before opening a pull request

```bash
npm run build:shell
npm run typecheck
npm test
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

CI runs the same checks on every push and pull request.

## Guidelines

- Read `CLAUDE.md` for architecture and the security boundaries. Pull requests that weaken them (unrestricted shell, reading other apps' credentials, silent clipboard monitoring, admin rights) won't be merged.
- Every tool call must go through `ToolHub::execute` so permissions, approvals and the activity log apply.
- Keep pure logic testable (Rust modules, `src/lib/*`) and add tests next to the code.
- Keep commits focused; describe what you tested in the PR.
