//! Build script: Tauri codegen plus an application manifest embedded into
//! every target (app, unit tests) so Common-Controls v6 APIs resolve and the
//! process always runs asInvoker (no elevation).

use std::path::PathBuf;

fn main() {
    let attrs = tauri_build::Attributes::new().windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    tauri_build::try_build(attrs).expect("tauri build");
    embed_manifest();
}

fn embed_manifest() {
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("windows-app-manifest.xml");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        return;
    }
    // GNU toolchain: compile a resource object with windres and link it.
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = out.join("manifest.rc");
    let obj = out.join("manifest.o");
    std::fs::write(&rc, format!("1 24 \"{}\"\n", manifest.display().to_string().replace(std::path::MAIN_SEPARATOR, "/"))).unwrap();
    let windres = ["windres", "x86_64-w64-mingw32-windres"]
        .into_iter()
        .find(|w| std::process::Command::new(w).arg("--version").output().is_ok())
        .expect("windres not found (install a MinGW toolchain or use the MSVC target)");
    let status = std::process::Command::new(windres)
        .args(["--input", rc.to_str().unwrap(), "--output-format=coff", "--output", obj.to_str().unwrap()])
        .status()
        .expect("windres failed to start");
    assert!(status.success(), "windres failed");
    println!("cargo:rustc-link-arg={}", obj.display());
}
