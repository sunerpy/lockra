//! The browser extension's side of the shell (docs/architecture.md, "The browser extension"):
//!
//! - [`host`]: host mode. The browser starts this binary for the extension; it relays between the
//!   browser's stdio and the app's socket (`lockra_browser::host`) and starts Lockra when asked.
//! - [`socket`]: where the app listens (a Unix socket in a folder of the user's alone, a named
//!   pipe on Windows) and how the host reaches it.
//! - [`manifest`]: the native-messaging manifests that tell each browser where the host is.
//! - [`service`]: the app's side, `lockra_browser::serve` on the socket while the integration is
//!   on, and the shell commands that turn it on and off.
//!
//! Nothing here reads what passes: Noise is end to end between the extension and the core.

pub mod host;
pub mod manifest;
pub mod service;
pub mod socket;

use std::path::PathBuf;

/// The app's identifier (`tauri.conf.json`): its data and config folders are named after it, and
/// host mode, which starts before any Tauri context, finds the socket by it.
pub const IDENTIFIER: &str = "dev.lockra.desktop";

/// Lockra was started for the browser (the host's `start_app`): with the background mode on it
/// stays in the tray, and a running Lockra keeps its window as it is.
pub const FROM_BROWSER: &str = "--from-browser";

/// The app's data and config folders, as Tauri resolves them (`app_data_dir`, `app_config_dir`):
/// host mode has no Tauri context to ask.
pub fn app_dirs() -> Option<(PathBuf, PathBuf)> {
    Some((dirs::data_dir()?.join(IDENTIFIER), dirs::config_dir()?.join(IDENTIFIER)))
}

/// The program the manifests point at and the host starts: the AppImage itself when Lockra runs
/// from one (the binary inside is mounted anew at each start), else this binary.
pub fn program() -> std::io::Result<PathBuf> {
    if let Some(image) = std::env::var_os("APPIMAGE").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(image));
    }
    std::env::current_exe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identifier_is_the_apps() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], IDENTIFIER);
    }

    #[test]
    fn the_folders_are_named_after_the_app() {
        let (data, config) = app_dirs().unwrap();
        assert!(data.ends_with(IDENTIFIER) && config.ends_with(IDENTIFIER));
    }
}
