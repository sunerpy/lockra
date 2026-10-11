use std::path::PathBuf;

fn main() {
    tauri_build::build();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    // tauri-build links the Windows resource (icon, version, and the manifest that asks for Common
    // Controls v6) into the application binary only. tauri-plugin-dialog's message dialogs import
    // `TaskDialogIndirect`, which only Common Controls v6 has: tests/ipc.rs would not even start
    // without the manifest (STATUS_ENTRYPOINT_NOT_FOUND). The test binaries get the same resource.
    if target_os == "windows" {
        let resource = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("resource.lib");
        if resource.exists() {
            println!("cargo:rustc-link-arg-tests={}", resource.display());
        }
    }
    // Static CRT (.cargo/config.toml `+crt-static`) and the Tauri CLI's hybrid CRT on top of it: the
    // vcruntime static, the UCRT from Windows (`/DEFAULTLIB:ucrt.lib`). MSVC's link.exe on Windows takes
    // that as it is. A Linux host cross-building with cargo-xwin links with lld-link, where the hybrid
    // `/DEFAULTLIB:ucrt.lib` resolves nothing (the link failed on `strlen` and `round`), so only there
    // the static UCRT is named. Naming it on Windows as well defines the UCRT twice: link.exe stopped
    // with LNK2005 on `__p___argc` in the v0.1.0 release run (2026-10-01).
    let cross_from_unix = std::env::var("HOST").is_ok_and(|host| !host.contains("windows"));
    let crt_static = std::env::var("CARGO_CFG_TARGET_FEATURE").is_ok_and(|f| f.split(',').any(|x| x == "crt-static"));
    if target_os == "windows" && crt_static && cross_from_unix {
        println!("cargo:rustc-link-arg=libucrt.lib");
    }
}
