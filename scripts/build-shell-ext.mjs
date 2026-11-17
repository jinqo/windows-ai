// Builds the Windows 11 context-menu DLL and copies it to shell-ext/bin so the
// Tauri bundler can ship it next to windows-ai.exe.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = join(root, "shell-ext", "Cargo.toml");
// Separate target dir so this never races the main app build.
const targetDir = process.env.CARGO_TARGET_DIR ? `${process.env.CARGO_TARGET_DIR}-shell` : join(root, "shell-ext", "target");
execFileSync("cargo", ["build", "--release", "--manifest-path", manifest], { stdio: "inherit", env: { ...process.env, CARGO_TARGET_DIR: targetDir } });
const dll = join(targetDir, "release", "windows_ai_shell.dll");
mkdirSync(join(root, "shell-ext", "bin"), { recursive: true });
copyFileSync(dll, join(root, "shell-ext", "bin", "windows_ai_shell.dll"));
console.log("shell extension ready:", dll);
