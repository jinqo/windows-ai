//! Universal selection capture, triggered only by the user's shortcut.
//!
//! 1. UI Automation: read the selection of the focused element via
//!    TextPattern (no clipboard involvement). Works in most native apps,
//!    Office, Edge/Chrome, and many editors.
//! 2. Fallback (optional): snapshot the clipboard, send Ctrl+C to the
//!    foreground app, read the text, then restore the original clipboard.
//!    Only used when UIA gives nothing; can be disabled in settings.
//!
//! Limitations: elevated (admin) windows cannot be read or sent input from
//! a normal-integrity process (UIPI) — we do not bypass this. Apps that
//! block copying, or remote/virtualized windows, may return nothing.
//! Windows clipboard history (Win+V) may record the temporary copy.

use super::context::CapturedContext;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ForegroundInfo {
    pub hwnd: isize,
    pub title: String,
    pub process_path: String,
    pub process_name: String,
}

const CODE_APPS: &[&str] = &[
    "code.exe", "code - insiders.exe", "cursor.exe", "devenv.exe", "idea64.exe", "pycharm64.exe", "webstorm64.exe", "rider64.exe", "clion64.exe",
    "goland64.exe", "sublime_text.exe", "notepad++.exe", "windowsterminal.exe", "wt.exe", "zed.exe", "robloxstudiobeta.exe",
];
const BROWSERS: &[&str] = &["chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe"];

/// Heuristic: is this snippet likely code?
pub fn looks_like_code(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return false;
    }
    let signals = ["{", "}", ";", "=>", "fn ", "def ", "function ", "class ", "import ", "#include", "const ", "let ", "return ", "</", "::"];
    let hits = lines.iter().filter(|l| signals.iter().any(|s| l.contains(s))).count();
    let indented = lines.iter().filter(|l| l.starts_with("    ") || l.starts_with('\t')).count();
    (hits * 2 + indented) as f32 / lines.len() as f32 >= 1.0
}

pub fn classify(process_name: &str, text: &str) -> &'static str {
    let p = process_name.to_lowercase();
    if CODE_APPS.contains(&p.as_str()) || looks_like_code(text) {
        "code"
    } else {
        "text"
    }
}

pub fn friendly_app(process_name: &str) -> String {
    let p = process_name.to_lowercase();
    let known = [
        ("msedge.exe", "Microsoft Edge"),
        ("chrome.exe", "Google Chrome"),
        ("firefox.exe", "Firefox"),
        ("code.exe", "Visual Studio Code"),
        ("winword.exe", "Microsoft Word"),
        ("outlook.exe", "Outlook"),
        ("olk.exe", "Outlook"),
        ("excel.exe", "Excel"),
        ("notepad.exe", "Notepad"),
        ("explorer.exe", "File Explorer"),
        ("teams.exe", "Teams"),
        ("ms-teams.exe", "Teams"),
        ("slack.exe", "Slack"),
        ("discord.exe", "Discord"),
    ];
    known.iter().find(|(k, _)| *k == p).map(|(_, v)| v.to_string()).unwrap_or_else(|| process_name.trim_end_matches(".exe").to_string())
}

pub fn is_browser(process_name: &str) -> bool {
    BROWSERS.contains(&process_name.to_lowercase().as_str())
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows::core::{Interface, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, POINT};
    use windows::Win32::System::Com::*;
    use windows::Win32::System::Threading::*;
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    pub fn foreground_info() -> ForegroundInfo {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return ForegroundInfo::default();
            }
            let mut buf = [0u16; 512];
            let n = GetWindowTextW(hwnd, &mut buf);
            let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let mut process_path = String::new();
            if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                let mut pbuf = [0u16; 1024];
                let mut len = pbuf.len() as u32;
                if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(pbuf.as_mut_ptr()), &mut len).is_ok() {
                    process_path = String::from_utf16_lossy(&pbuf[..len as usize]);
                }
                let _ = CloseHandle(h);
            }
            let process_name = std::path::Path::new(&process_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            ForegroundInfo { hwnd: hwnd.0 as isize, title, process_path, process_name }
        }
    }

    pub fn cursor_position() -> (i32, i32) {
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        (p.x, p.y)
    }

    pub fn uia_selection() -> Option<String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let el = uia.GetFocusedElement().ok()?;
            let unk = el.GetCurrentPattern(UIA_TextPatternId).ok()?;
            let pattern: IUIAutomationTextPattern = unk.cast().ok()?;
            let ranges = pattern.GetSelection().ok()?;
            let mut parts = Vec::new();
            for i in 0..ranges.Length().ok()? {
                if let Ok(r) = ranges.GetElement(i) {
                    if let Ok(t) = r.GetText(-1) {
                        let s = t.to_string();
                        if !s.is_empty() {
                            parts.push(s);
                        }
                    }
                }
            }
            let text = parts.join("\n");
            if text.trim().is_empty() {
                None
            } else {
                Some(text)
            }
        }
    }

    fn key_down(vk: VIRTUAL_KEY) -> bool {
        unsafe { (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0 }
    }

    /// Waits until the user has released the hotkey's modifiers, so the
    /// synthetic Ctrl+C is not combined with e.g. Shift (Ctrl+Shift+C).
    pub fn wait_modifiers_released(max_ms: u64) -> bool {
        let start = std::time::Instant::now();
        loop {
            let held = [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN, VK_SPACE].iter().any(|k| key_down(*k));
            if !held {
                return true;
            }
            if start.elapsed().as_millis() as u64 > max_ms {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    }

    fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 },
            },
        }
    }

    pub fn send_copy() -> bool {
        let c = VIRTUAL_KEY(b'C' as u16);
        let inputs = [key(VK_CONTROL, false), key(c, false), key(c, true), key(VK_CONTROL, true)];
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize == inputs.len() }
    }
}

#[cfg(windows)]
pub use win::{cursor_position, foreground_info};

/// Captures the current selection in the foreground app.
#[cfg(windows)]
pub fn capture_selection(allow_clipboard_fallback: bool, max_chars: usize) -> AppResult<CapturedContext> {
    use super::clipboard;
    let info = win::foreground_info();
    let own = std::env::current_exe().ok().map(|p| p.to_string_lossy().to_lowercase());
    if own.as_deref() == Some(info.process_path.to_lowercase().as_str()) {
        return Err(AppError::InvalidInput("Windows AI itself is focused; select text in another app first".into()));
    }

    let mut method = "ui_automation";
    let mut text = win::uia_selection();
    if text.is_none() && allow_clipboard_fallback {
        if !win::wait_modifiers_released(1500) {
            return Err(AppError::Internal("release the shortcut keys to capture the selection".into()));
        }
        let snap = clipboard::snapshot().ok();
        let before = clipboard::sequence_number();
        if win::send_copy() {
            let start = std::time::Instant::now();
            while clipboard::sequence_number() == before && start.elapsed().as_millis() < 500 {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            if clipboard::sequence_number() != before {
                // Give the source app a moment to finish rendering all formats.
                std::thread::sleep(std::time::Duration::from_millis(30));
                text = clipboard::read_text().ok().flatten().filter(|t| !t.trim().is_empty());
                method = "clipboard";
            }
        }
        if let Some(s) = &snap {
            if clipboard::sequence_number() != before {
                let _ = clipboard::restore(s);
            }
        }
    }
    let text = text.ok_or_else(|| AppError::NotFound("No selected text found in the active window".into()))?;
    let kind = classify(&info.process_name, &text);
    let mut c = CapturedContext::new("selection", kind, text);
    c.app = Some(friendly_app(&info.process_name));
    c.title = Some(info.title.clone()).filter(|t| !t.is_empty());
    c.method = Some(method.into());
    c.sanitize(max_chars).map_err(AppError::InvalidInput)
}

#[cfg(not(windows))]
pub fn capture_selection(_a: bool, _m: usize) -> AppResult<CapturedContext> {
    Err(AppError::Internal("selection capture is only available on Windows".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_code_and_text() {
        assert!(looks_like_code("fn main() {\n    println!(\"hi\");\n}"));
        assert!(!looks_like_code("The quick brown fox jumps over the lazy dog.\nIt was a sunny day."));
        assert_eq!(classify("Code.exe", "hello"), "code");
        assert_eq!(classify("winword.exe", "Dear team, thanks."), "text");
    }

    #[test]
    fn friendly_names() {
        assert_eq!(friendly_app("msedge.exe"), "Microsoft Edge");
        assert_eq!(friendly_app("foo.exe"), "foo");
        assert!(is_browser("Chrome.exe"));
    }

    #[cfg(windows)]
    #[test]
    fn foreground_info_does_not_panic() {
        let _ = foreground_info();
        let _ = cursor_position();
    }
}
