//! Chrome/Edge native-messaging host.
//!
//! The browser launches `windows-ai.exe chrome-extension://<id>/` with
//! stdio pipes. Messages are a 4-byte little-endian length followed by
//! UTF-8 JSON. The host forwards captures to the running app over the
//! authenticated named-pipe bridge, starting the app if needed.

use super::bridge;
use crate::error::{AppError, AppResult};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const HOST_NAME: &str = "com.windowsai.bridge";
/// ID of the bundled extension (fixed by the `key` in extensions/browser/manifest.json).
pub const EXTENSION_ID: &str = include_str!("../../../extensions/browser/EXTENSION_ID").trim_ascii();
const MAX_IN: u32 = 16 * 1024 * 1024;

pub fn is_native_host_invocation(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a.starts_with("chrome-extension://"))
}

pub fn read_message<R: Read>(r: &mut R) -> AppResult<Option<Value>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let n = u32::from_le_bytes(len);
    if n > MAX_IN {
        return Err(AppError::InvalidInput(format!("native message too large ({n} bytes)")));
    }
    let mut buf = vec![0u8; n as usize];
    r.read_exact(&mut buf)?;
    Ok(Some(serde_json::from_slice(&buf)?))
}

pub fn write_message<W: Write>(w: &mut W, v: &Value) -> AppResult<()> {
    let bytes = serde_json::to_vec(v)?;
    // Host → browser messages are limited to 1 MB.
    if bytes.len() > 1024 * 1024 {
        return Err(AppError::InvalidInput("response too large".into()));
    }
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(&bytes)?;
    w.flush()?;
    Ok(())
}

fn ensure_app_running() {
    if bridge::send_request(json!({"type": "ping"})).is_ok() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("--background").spawn();
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            if bridge::send_request(json!({"type": "ping"})).is_ok() {
                return;
            }
        }
    }
}

/// Translates one extension message into a bridge request.
pub fn handle(msg: &Value, forward: &dyn Fn(Value) -> AppResult<Value>) -> Value {
    match msg["type"].as_str() {
        Some("ping") => forward(json!({"type": "ping"})).unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()})),
        Some("capture") => {
            let mut payload = msg["payload"].clone();
            // The browser host can only ever report browser content.
            payload["source"] = json!("browser");
            forward(json!({"type": "capture", "payload": payload})).unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()}))
        }
        _ => json!({"ok": false, "error": "unsupported message"}),
    }
}

/// Entry point for native-host mode. Never returns an error to the browser
/// as a crash: failures are reported as `{ok:false}` messages.
pub fn run() {
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut started = false;
    while let Ok(Some(msg)) = read_message(&mut stdin) {
        if !started {
            ensure_app_running();
            started = true;
        }
        let resp = handle(&msg, &bridge::send_request);
        if write_message(&mut stdout, &resp).is_err() {
            break;
        }
    }
}

pub fn manifest_json(exe: &Path, extension_ids: &[String]) -> Value {
    json!({
        "name": HOST_NAME,
        "description": "Windows AI browser bridge",
        "path": exe.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": extension_ids.iter().map(|id| format!("chrome-extension://{id}/")).collect::<Vec<_>>(),
    })
}

pub const BROWSER_KEYS: &[(&str, &str)] = &[
    ("chrome", r"Software\Google\Chrome\NativeMessagingHosts"),
    ("edge", r"Software\Microsoft\Edge\NativeMessagingHosts"),
];

pub fn manifest_path() -> PathBuf {
    bridge::bridge_dir().join("native-host").join(format!("{HOST_NAME}.json"))
}

/// Registers the host for Chrome and Edge (per-user, HKCU; no admin).
#[cfg(windows)]
pub fn install(extra_ids: &[String]) -> AppResult<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let exe = std::env::current_exe()?;
    let mut ids = vec![EXTENSION_ID.to_string()];
    ids.extend(extra_ids.iter().filter(|i| i.len() == 32 && i.chars().all(|c| ('a'..='p').contains(&c))).cloned());
    let path = manifest_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_string_pretty(&manifest_json(&exe, &ids))?)?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (_, key) in BROWSER_KEYS {
        let (k, _) = hkcu.create_subkey(format!(r"{key}\{HOST_NAME}"))?;
        k.set_value("", &path.to_string_lossy().to_string())?;
    }
    Ok(())
}

#[cfg(windows)]
pub fn uninstall() -> AppResult<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (_, key) in BROWSER_KEYS {
        let _ = hkcu.delete_subkey_all(format!(r"{key}\{HOST_NAME}"));
    }
    let _ = std::fs::remove_file(manifest_path());
    Ok(())
}

#[cfg(windows)]
pub fn status() -> Value {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut out = serde_json::Map::new();
    for (browser, key) in BROWSER_KEYS {
        let registered = hkcu.open_subkey(format!(r"{key}\{HOST_NAME}")).and_then(|k| k.get_value::<String, _>("")).map(|p| Path::new(&p).exists()).unwrap_or(false);
        out.insert((*browser).into(), json!(registered));
    }
    out.insert("extension_id".into(), json!(EXTENSION_ID));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn framing_round_trip() {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({"a": "é"})).unwrap();
        assert_eq!(u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize, buf.len() - 4);
        let mut c = Cursor::new(buf);
        assert_eq!(read_message(&mut c).unwrap().unwrap()["a"], "é");
        assert!(read_message(&mut c).unwrap().is_none(), "EOF ends cleanly");
    }

    #[test]
    fn rejects_oversized_frames() {
        let mut c = Cursor::new((MAX_IN + 1).to_le_bytes().to_vec());
        assert!(read_message(&mut c).is_err());
    }

    #[test]
    fn forces_browser_source() {
        let seen = std::sync::Mutex::new(Value::Null);
        let fwd = |v: Value| {
            *seen.lock().unwrap() = v;
            Ok(json!({"ok": true}))
        };
        let r = handle(&json!({"type": "capture", "payload": {"source": "explorer", "kind": "page", "content": "x"}}), &fwd);
        assert_eq!(r["ok"], true);
        assert_eq!(seen.lock().unwrap()["payload"]["source"], "browser");
        assert_eq!(handle(&json!({"type": "run_tool"}), &fwd)["ok"], false);
    }

    #[test]
    fn detects_invocation_and_builds_manifest() {
        assert!(is_native_host_invocation(&["x.exe".into(), "chrome-extension://abc/".into()]));
        assert!(!is_native_host_invocation(&["x.exe".into(), "--show".into()]));
        let m = manifest_json(Path::new("C:\\a\\w.exe"), &["abcdefghijklmnopabcdefghijklmnop".into()]);
        assert_eq!(m["allowed_origins"][0], "chrome-extension://abcdefghijklmnopabcdefghijklmnop/");
        assert_eq!(m["type"], "stdio");
        assert_eq!(EXTENSION_ID.len(), 32);
    }
}
