//! File Explorer right-click integration.
//!
//! Registers a per-user (HKCU) cascading "Windows AI" menu on files and
//! folders with Ask / Explain / Summarize / Analyze. No admin rights needed.
//!
//! Platform limitation: on Windows 11 these classic verbs appear under
//! "Show more options" (Shift+F10). Entries in the new top-level menu
//! require an IExplorerCommand handler shipped in a signed MSIX/sparse
//! package with package identity, which this build does not include.
//!
//! With multiple items selected, Explorer starts the command once per item;
//! the running instance receives each via single-instance forwarding and
//! batches arrivals within a short window into one capture.

use super::context::{CapturedContext, FileRef};
use crate::error::{AppError, AppResult};
use std::path::Path;

pub const MENU_KEY: &str = "WindowsAI";
pub const TARGETS: &[&str] = &[r"Software\Classes\*\shell", r"Software\Classes\Directory\shell"];
pub const VERBS: &[(&str, &str, &str)] = &[
    ("1ask", "Ask Windows AI…", "ask"),
    ("2explain", "Explain", "explain"),
    ("3summarize", "Summarize", "summarize"),
    ("4analyze", "Analyze", "analyze"),
];

/// Command line registered for a verb.
pub fn verb_command(exe: &Path, action: &str) -> String {
    format!("\"{}\" --context-action {} --file \"%1\"", exe.display(), action)
}

#[derive(Debug, Clone, PartialEq)]
pub enum CliCommand {
    /// Show the overlay.
    Show,
    /// Start without showing (e.g. autostart, native host launch).
    Background,
    /// Explorer verb: action + file paths (classic verbs pass one; the
    /// Windows 11 menu handler passes the whole selection).
    FileAction { action: String, paths: Vec<String> },
    /// Capture the current selection (for custom hotkey tools).
    CaptureSelection,
}

/// Parses process arguments (including those forwarded by single-instance).
pub fn parse_cli(args: &[String]) -> Option<CliCommand> {
    let mut it = args.iter().skip(1);
    let mut action = None;
    let mut files: Vec<String> = Vec::new();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--context-action" => action = it.next().cloned(),
            "--file" => {
                if let Some(f) = it.next().filter(|f| !f.is_empty()) {
                    files.push(f.clone());
                }
            }
            "--background" | "--autostart" | "--minimized" => return Some(CliCommand::Background),
            "--show" => return Some(CliCommand::Show),
            "--capture-selection" => return Some(CliCommand::CaptureSelection),
            _ => {}
        }
    }
    match action {
        Some(action) if super::context::ACTIONS.contains(&action.as_str()) && !files.is_empty() => Some(CliCommand::FileAction { action, paths: files }),
        _ => None,
    }
}

const TEXT_EXTS: &[&str] = &[
    "txt", "md", "markdown", "rs", "ts", "tsx", "js", "jsx", "json", "toml", "yaml", "yml", "xml", "html", "htm", "css", "scss", "py", "java", "kt", "c", "h",
    "cpp", "hpp", "cs", "go", "rb", "php", "swift", "sql", "sh", "ps1", "bat", "cmd", "ini", "cfg", "conf", "log", "csv", "tsv", "lua", "luau", "vue", "svelte",
    "gradle", "dart", "r", "tex", "env.example", "gitignore", "dockerfile",
];

pub fn looks_textual(p: &Path) -> bool {
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if TEXT_EXTS.contains(&ext.as_str()) {
        return true;
    }
    let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    ["readme", "license", "makefile", "dockerfile"].contains(&name.as_str())
}

/// Builds a capture for files chosen in Explorer. The user explicitly
/// picked these files, so small text files are included (up to
/// `max_file_bytes` each and `max_total` overall); folders and binaries
/// are listed by name and size only.
pub fn build_file_context(paths: &[String], action: &str, max_file_bytes: u64, max_total: usize) -> AppResult<CapturedContext> {
    if paths.is_empty() {
        return Err(AppError::InvalidInput("no files".into()));
    }
    let mut files = Vec::new();
    let mut content = String::new();
    let mut truncated = false;
    for p in paths.iter().take(100) {
        let path = Path::new(p);
        let md = std::fs::metadata(path).map_err(|e| AppError::NotFound(format!("{p}: {e}")))?;
        let mut fr = FileRef { path: p.clone(), size: md.len(), is_dir: md.is_dir(), included: false };
        if md.is_dir() {
            let mut names: Vec<String> = std::fs::read_dir(path)
                .map(|rd| rd.filter_map(Result::ok).take(200).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                .unwrap_or_default();
            names.sort();
            content.push_str(&format!("\n--- folder: {p} ({} entries shown) ---\n{}\n", names.len(), names.join("\n")));
        } else if looks_textual(path) && content.len() < max_total {
            match crate::tools::builtin_read_text(path, max_file_bytes) {
                Ok((text, cut)) => {
                    truncated |= cut;
                    fr.included = true;
                    content.push_str(&format!("\n--- file: {p} ---\n{text}\n"));
                }
                Err(_) => {}
            }
        }
        files.push(fr);
    }
    if content.len() > max_total {
        let mut cut = max_total;
        while !content.is_char_boundary(cut) {
            cut -= 1;
        }
        content.truncate(cut);
        truncated = true;
    }
    let mut c = CapturedContext::new("explorer", "files", content.trim().to_string());
    c.files = files;
    c.action = Some(action.to_string());
    c.truncated = truncated;
    c.app = Some("File Explorer".into());
    c.method = Some("context_menu".into());
    Ok(c)
}

#[cfg(windows)]
pub fn install() -> AppResult<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let exe = std::env::current_exe()?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for target in TARGETS {
        let (root, _) = hkcu.create_subkey(format!(r"{target}\{MENU_KEY}"))?;
        root.set_value("MUIVerb", &"Windows AI")?;
        root.set_value("SubCommands", &"")?;
        root.set_value("Icon", &format!("\"{}\",0", exe.display()))?;
        root.set_value("MultiSelectModel", &"Player")?;
        for (key, label, action) in VERBS {
            let (verb, _) = root.create_subkey(format!(r"shell\{key}"))?;
            verb.set_value("MUIVerb", label)?;
            let (cmd, _) = verb.create_subkey("command")?;
            cmd.set_value("", &verb_command(&exe, action))?;
        }
    }
    notify_shell();
    Ok(())
}

#[cfg(windows)]
pub fn uninstall() -> AppResult<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for target in TARGETS {
        match hkcu.delete_subkey_all(format!(r"{target}\{MENU_KEY}")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    notify_shell();
    Ok(())
}

/// True when the menu is registered and points at this executable.
#[cfg(windows)]
pub fn is_installed() -> bool {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let Ok(exe) = std::env::current_exe() else { return false };
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(format!(r"{}\{MENU_KEY}\shell\{}\command", TARGETS[0], VERBS[0].0))
        .and_then(|k| k.get_value::<String, _>(""))
        .map(|v| v == verb_command(&exe, VERBS[0].2))
        .unwrap_or(false)
}

#[cfg(windows)]
fn notify_shell() {
    // SHChangeNotify(SHCNE_ASSOCCHANGED) is not needed for shell verbs; Explorer
    // reads HKCU\Software\Classes on each menu open.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        std::iter::once("windows-ai.exe").chain(a.iter().copied()).map(String::from).collect()
    }

    #[test]
    fn parses_cli() {
        assert_eq!(
            parse_cli(&args(&["--context-action", "summarize", "--file", "C:\\a b\\x.txt"])),
            Some(CliCommand::FileAction { action: "summarize".into(), paths: vec!["C:\\a b\\x.txt".into()] })
        );
        assert_eq!(
            parse_cli(&args(&["--context-action", "explain", "--file", "C:\\a", "--file", "C:\\b"])),
            Some(CliCommand::FileAction { action: "explain".into(), paths: vec!["C:\\a".into(), "C:\\b".into()] })
        );
        assert_eq!(parse_cli(&args(&["--context-action", "format_c", "--file", "x"])), None);
        assert_eq!(parse_cli(&args(&["--background"])), Some(CliCommand::Background));
        assert_eq!(parse_cli(&args(&[])), None);
    }

    #[test]
    fn verb_command_quotes_paths() {
        let c = verb_command(Path::new("C:\\Program Files\\Windows AI\\windows-ai.exe"), "explain");
        assert_eq!(c, "\"C:\\Program Files\\Windows AI\\windows-ai.exe\" --context-action explain --file \"%1\"");
    }

    #[test]
    fn builds_file_context() {
        let d = tempfile::tempdir().unwrap();
        let t = d.path().join("notes.md");
        std::fs::write(&t, "# Title\nbody").unwrap();
        let b = d.path().join("image.png");
        std::fs::write(&b, [0u8; 10]).unwrap();
        let c = build_file_context(&[t.to_string_lossy().into(), b.to_string_lossy().into(), d.path().to_string_lossy().into()], "summarize", 1000, 10_000).unwrap();
        assert_eq!(c.source, "explorer");
        assert_eq!(c.files.len(), 3);
        assert!(c.files[0].included);
        assert!(!c.files[1].included, "binary files are listed, not read");
        assert!(c.files[2].is_dir);
        assert!(c.content.contains("# Title"));
        assert!(c.content.contains("notes.md"));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "writes to HKCU; run manually with --ignored"]
    fn install_uninstall_round_trip() {
        install().unwrap();
        assert!(is_installed());
        uninstall().unwrap();
        assert!(!is_installed());
    }
}
