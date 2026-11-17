// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Browser native-messaging host and Claude Code MCP helper modes run
    // without starting the UI or the single-instance logic.
    if windows_ai_lib::run_special_mode(&args) {
        return;
    }
    windows_ai_lib::run();
}
