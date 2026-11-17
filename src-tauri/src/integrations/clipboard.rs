//! Clipboard snapshot/restore used only by the explicit selection-capture
//! fallback. The clipboard is never read or monitored otherwise.
//!
//! A snapshot copies every HGLOBAL-backed format (text, HTML, RTF, DIB,
//! file lists, custom formats). GDI-handle formats (CF_BITMAP, metafiles,
//! palettes) are skipped; Windows re-synthesizes CF_BITMAP from CF_DIB.

#![cfg(windows)]

use crate::error::{AppError, AppResult};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;

const CF_BITMAP: u32 = 2;
const CF_METAFILEPICT: u32 = 3;
const CF_PALETTE: u32 = 9;
const CF_ENHMETAFILE: u32 = 14;
const CF_OWNERDISPLAY: u32 = 0x80;
const CF_DSPBITMAP: u32 = 0x82;
const CF_DSPMETAFILEPICT: u32 = 0x83;
const CF_DSPENHMETAFILE: u32 = 0x8E;
pub const CF_UNICODETEXT: u32 = 13;

fn is_handle_format(f: u32) -> bool {
    matches!(f, CF_BITMAP | CF_METAFILEPICT | CF_PALETTE | CF_ENHMETAFILE | CF_OWNERDISPLAY | CF_DSPBITMAP | CF_DSPMETAFILEPICT | CF_DSPENHMETAFILE)
        || (0x0300..=0x03FF).contains(&f) // CF_GDIOBJFIRST..LAST
}

pub struct Snapshot {
    pub formats: Vec<(u32, Vec<u8>)>,
}

struct ClipboardGuard;
impl ClipboardGuard {
    fn open() -> AppResult<Self> {
        for _ in 0..10 {
            if unsafe { OpenClipboard(Some(HWND::default())) }.is_ok() {
                return Ok(ClipboardGuard);
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        Err(AppError::Internal("clipboard is busy".into()))
    }
}
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

pub fn sequence_number() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub fn snapshot() -> AppResult<Snapshot> {
    let _g = ClipboardGuard::open()?;
    let mut formats = Vec::new();
    let mut f = 0u32;
    loop {
        f = unsafe { EnumClipboardFormats(f) };
        if f == 0 {
            break;
        }
        if is_handle_format(f) {
            continue;
        }
        unsafe {
            let Ok(h) = GetClipboardData(f) else { continue };
            let hg = HGLOBAL(h.0);
            let size = GlobalSize(hg);
            if size == 0 || size > 64 * 1024 * 1024 {
                continue;
            }
            let p = GlobalLock(hg) as *const u8;
            if p.is_null() {
                continue;
            }
            let bytes = std::slice::from_raw_parts(p, size).to_vec();
            let _ = GlobalUnlock(hg);
            formats.push((f, bytes));
        }
    }
    Ok(Snapshot { formats })
}

pub fn restore(s: &Snapshot) -> AppResult<()> {
    let _g = ClipboardGuard::open()?;
    unsafe {
        EmptyClipboard().map_err(|e| AppError::Internal(format!("EmptyClipboard: {e}")))?;
        for (f, bytes) in &s.formats {
            let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, bytes.len()) else { continue };
            let p = GlobalLock(h) as *mut u8;
            if p.is_null() {
                let _ = GlobalFree(Some(h));
                continue;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
            let _ = GlobalUnlock(h);
            if SetClipboardData(*f, Some(HANDLE(h.0))).is_err() {
                let _ = GlobalFree(Some(h));
            }
        }
    }
    Ok(())
}

pub fn read_text() -> AppResult<Option<String>> {
    let _g = ClipboardGuard::open()?;
    unsafe {
        let Ok(h) = GetClipboardData(CF_UNICODETEXT) else { return Ok(None) };
        let hg = HGLOBAL(h.0);
        let p = GlobalLock(hg) as *const u16;
        if p.is_null() {
            return Ok(None);
        }
        let max = GlobalSize(hg) / 2;
        let mut len = 0;
        while len < max && *p.add(len) != 0 {
            len += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
        let _ = GlobalUnlock(hg);
        Ok(Some(s))
    }
}

pub fn write_text(text: &str) -> AppResult<()> {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let bytes: Vec<u8> = wide.iter().flat_map(|w| w.to_le_bytes()).collect();
    restore(&Snapshot { formats: vec![(CF_UNICODETEXT, bytes)] })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real clipboard; restores it afterwards.
    #[test]
    #[ignore = "uses the system clipboard; run with --ignored"]
    fn snapshot_restore_round_trip() {
        let original = snapshot().unwrap();
        write_text("windows-ai test ✓").unwrap();
        assert_eq!(read_text().unwrap().as_deref(), Some("windows-ai test ✓"));
        let snap = snapshot().unwrap();
        write_text("other").unwrap();
        restore(&snap).unwrap();
        assert_eq!(read_text().unwrap().as_deref(), Some("windows-ai test ✓"));
        restore(&original).unwrap();
    }

    #[test]
    fn handle_formats_are_skipped() {
        assert!(is_handle_format(CF_BITMAP));
        assert!(is_handle_format(0x0305));
        assert!(!is_handle_format(CF_UNICODETEXT));
        assert!(!is_handle_format(0xC123)); // registered formats are HGLOBAL
    }
}
