//! Windows 11 top-level File Explorer context menu for Windows AI.
//!
//! An `IExplorerCommand` with sub-commands (Ask / Explain / Summarize /
//! Analyze). Registered through a sparse MSIX package (see ../package),
//! which is what Windows 11 requires for the modern menu. Invoking a
//! command launches `windows-ai.exe --context-action <a> --file <p>...`
//! from the same folder as this DLL; the running app handles the rest.

#![allow(non_snake_case)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::{implement, Interface, Ref, GUID, HRESULT, PWSTR, Result, BOOL};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_NOTIMPL, E_POINTER, HMODULE, S_FALSE, S_OK};
use windows::Win32::System::Com::{IBindCtx, IClassFactory, IClassFactory_Impl, CoTaskMemAlloc};
use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT};
use windows::Win32::UI::Shell::{
    IEnumExplorerCommand, IEnumExplorerCommand_Impl, IExplorerCommand, IExplorerCommand_Impl, IShellItemArray, ECS_ENABLED, ECF_DEFAULT, ECF_HASSUBCOMMANDS,
    SIGDN_FILESYSPATH,
};

/// CLSID registered in AppxManifest.xml.
pub const CLSID_WINDOWS_AI_MENU: GUID = GUID::from_u128(0x5e6f1988_6cfc_44eb_bc01_a5e4a629b26d);

static LOCKS: AtomicU32 = AtomicU32::new(0);

pub const ACTIONS: &[(&str, &str)] = &[("ask", "Ask Windows AI…"), ("explain", "Explain"), ("summarize", "Summarize"), ("analyze", "Analyze")];

fn alloc_wstr(s: &str) -> Result<PWSTR> {
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let p = CoTaskMemAlloc(wide.len() * 2) as *mut u16;
        if p.is_null() {
            return Err(windows::Win32::Foundation::E_OUTOFMEMORY.into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        Ok(PWSTR(p))
    }
}

/// Directory containing this DLL (the app's install folder).
fn module_dir() -> Option<std::path::PathBuf> {
    unsafe {
        let mut h = HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            windows::core::PCWSTR(module_dir as *const u16),
            &mut h,
        )
        .ok()?;
        let mut buf = [0u16; 1024];
        let n = GetModuleFileNameW(Some(h), &mut buf) as usize;
        let p = std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n]));
        p.parent().map(|d| d.to_path_buf())
    }
}

/// Builds the command line arguments for a set of paths.
pub fn build_args(action: &str, paths: &[String]) -> Vec<String> {
    let mut a = vec!["--context-action".to_string(), action.to_string()];
    for p in paths.iter().take(100) {
        a.push("--file".into());
        a.push(p.clone());
    }
    a
}

fn selected_paths(items: Ref<IShellItemArray>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(arr) = items.as_ref() else { return out };
    unsafe {
        let n = arr.GetCount().unwrap_or(0);
        for i in 0..n {
            if let Ok(item) = arr.GetItemAt(i) {
                if let Ok(p) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                    if let Ok(s) = p.to_string() {
                        out.push(s);
                    }
                    windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const c_void));
                }
            }
        }
    }
    out
}

#[implement(IExplorerCommand)]
struct RootCommand;

#[implement(IExplorerCommand)]
struct ActionCommand {
    action: &'static str,
    title: &'static str,
}

#[implement(IEnumExplorerCommand)]
struct SubEnum {
    index: AtomicU32,
}

impl IExplorerCommand_Impl for RootCommand_Impl {
    fn GetTitle(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        alloc_wstr("Windows AI")
    }
    fn GetIcon(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        let exe = module_dir().map(|d| d.join("windows-ai.exe")).unwrap_or_default();
        alloc_wstr(&format!("{},0", exe.display()))
    }
    fn GetToolTip(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(CLSID_WINDOWS_AI_MENU)
    }
    fn GetState(&self, _items: Ref<IShellItemArray>, _slow: BOOL) -> Result<u32> {
        Ok(ECS_ENABLED.0 as u32)
    }
    fn Invoke(&self, _items: Ref<IShellItemArray>, _ctx: Ref<IBindCtx>) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_HASSUBCOMMANDS.0 as u32)
    }
    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Ok(SubEnum { index: AtomicU32::new(0) }.into())
    }
}

impl IExplorerCommand_Impl for ActionCommand_Impl {
    fn GetTitle(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        alloc_wstr(self.title)
    }
    fn GetIcon(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetToolTip(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(GUID::zeroed())
    }
    fn GetState(&self, _items: Ref<IShellItemArray>, _slow: BOOL) -> Result<u32> {
        Ok(ECS_ENABLED.0 as u32)
    }
    fn Invoke(&self, items: Ref<IShellItemArray>, _ctx: Ref<IBindCtx>) -> Result<()> {
        let paths = selected_paths(items);
        if paths.is_empty() {
            return Ok(());
        }
        let exe = module_dir().map(|d| d.join("windows-ai.exe")).ok_or_else(|| windows::core::Error::from(E_POINTER))?;
        std::process::Command::new(exe).args(build_args(self.action, &paths)).spawn().map_err(|_| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
        Ok(())
    }
    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }
    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

impl IEnumExplorerCommand_Impl for SubEnum_Impl {
    fn Next(&self, celt: u32, out: *mut Option<IExplorerCommand>, fetched: *mut u32) -> HRESULT {
        if out.is_null() {
            return E_POINTER;
        }
        let mut n = 0u32;
        while n < celt {
            let i = self.index.fetch_add(1, Ordering::SeqCst) as usize;
            let Some((action, title)) = ACTIONS.get(i) else { break };
            let cmd: IExplorerCommand = ActionCommand { action, title }.into();
            unsafe { out.add(n as usize).write(Some(cmd)) };
            n += 1;
        }
        if !fetched.is_null() {
            unsafe { *fetched = n };
        }
        if n == celt { S_OK } else { S_FALSE }
    }
    fn Skip(&self, celt: u32) -> Result<()> {
        self.index.fetch_add(celt, Ordering::SeqCst);
        Ok(())
    }
    fn Reset(&self) -> Result<()> {
        self.index.store(0, Ordering::SeqCst);
        Ok(())
    }
    fn Clone(&self) -> Result<IEnumExplorerCommand> {
        Ok(SubEnum { index: AtomicU32::new(self.index.load(Ordering::SeqCst)) }.into())
    }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<windows::core::IUnknown>, riid: *const GUID, ppv: *mut *mut c_void) -> Result<()> {
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        if ppv.is_null() || riid.is_null() {
            return Err(E_POINTER.into());
        }
        let cmd: IExplorerCommand = RootCommand.into();
        unsafe { cmd.query(riid, ppv).ok() }
    }
    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            LOCKS.fetch_add(1, Ordering::SeqCst);
        } else {
            LOCKS.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(rclsid: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return E_POINTER;
    }
    *ppv = std::ptr::null_mut();
    if *rclsid != CLSID_WINDOWS_AI_MENU {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let f: IClassFactory = Factory.into();
    f.query(riid, ppv)
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if LOCKS.load(Ordering::SeqCst) == 0 { S_OK } else { S_FALSE }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_for_multi_select() {
        let a = build_args("summarize", &["C:\\a b\\x.txt".into(), "C:\\y".into()]);
        assert_eq!(a, vec!["--context-action", "summarize", "--file", "C:\\a b\\x.txt", "--file", "C:\\y"]);
    }

    #[test]
    fn class_factory_and_enum() {
        unsafe {
            let mut ppv: *mut c_void = std::ptr::null_mut();
            assert_eq!(DllGetClassObject(&GUID::zeroed(), &IClassFactory::IID, &mut ppv), CLASS_E_CLASSNOTAVAILABLE);
            assert_eq!(DllGetClassObject(&CLSID_WINDOWS_AI_MENU, &IClassFactory::IID, &mut ppv), S_OK);
            let factory = IClassFactory::from_raw(ppv);
            let cmd: IExplorerCommand = factory.CreateInstance(None).unwrap();
            assert_eq!(cmd.GetFlags().unwrap(), ECF_HASSUBCOMMANDS.0 as u32);
            let en = cmd.EnumSubCommands().unwrap();
            let mut items: [Option<IExplorerCommand>; 8] = Default::default();
            let mut n = 0u32;
            let _ = en.Next(&mut items, Some(&mut n));
            assert_eq!(n as usize, ACTIONS.len());
            let title = items[1].as_ref().unwrap().GetTitle(None).unwrap();
            assert_eq!(title.to_string().unwrap(), "Explain");
            assert_eq!(DllCanUnloadNow(), S_OK);
        }
    }
}
