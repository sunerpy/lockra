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
    // Static CRT (.cargo/config.toml `+crt-static`): Rust std and the bundled SQLite then need the
    // static UCRT. `cargo xwin build` adds it by itself, but the Tauri CLI runs `cargo-xwin build …`,
    // where that step does not run, and its hybrid-CRT `/DEFAULTLIB:ucrt.lib` resolves nothing under
    // lld-link: the link failed on `strlen` and `round` (measured 2026-10-01). Naming the archive works
    // in both forms and with MSVC's own link.exe, because the ucrt lib directory is on the link path.
    if target_os == "windows" && std::env::var("CARGO_CFG_TARGET_FEATURE").is_ok_and(|f| f.split(',').any(|x| x == "crt-static")) {
        println!("cargo:rustc-link-arg=libucrt.lib");
    }
}
