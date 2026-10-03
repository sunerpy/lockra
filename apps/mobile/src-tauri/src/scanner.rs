//! The camera, through `ScannerPlugin.kt` and `ScannerActivity.kt` on Android: the first QR code
//! read comes back to Rust, never to the webview, and goes into the import preview (or, for an
//! invitation, into joining its sync space: src/sync.rs). Leaving the app while the camera is open
//! locks the vault at once. Other builds of this crate (the host tests) have no camera and say so.

use lockra_core::ports::PortError;
use lockra_core::{Core, CoreError, ErrorCode};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};
use zeroize::Zeroizing;

/// Why there is no camera on this build.
pub const CAMERA_UNAVAILABLE: &str = "scanner: this build has no camera";

/// What a scan ended with.
#[derive(Debug, PartialEq, Eq)]
pub enum Scan {
    /// A QR code's text.
    Code(Zeroizing<String>),
    /// Left without one: the back gesture or the cancel button.
    Left,
    /// The app went into the background while the camera was open.
    Away,
    /// The camera permission was refused.
    Denied,
    /// There is no camera, or it could not be opened.
    NoCamera,
}

/// The words on the camera's page, from the webview's dictionaries.
#[derive(Debug, Clone, Serialize)]
pub struct ScanTexts {
    /// What to do, over the picture.
    pub prompt: String,
    /// The button that leaves.
    pub cancel: String,
}

/// The Android plugin (`ScannerPlugin.kt`).
#[cfg(target_os = "android")]
struct Plugin<R: Runtime>(tauri::plugin::PluginHandle<R>);

/// Registers the Android plugin; a no-op elsewhere.
pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("lockra-scanner")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager as _;
                let handle = _api.register_android_plugin("dev.lockra.mobile", "ScannerPlugin")?;
                _app.manage(Plugin(handle));
            }
            Ok(())
        })
        .build()
}

/// The phone's camera.
pub struct Scanner<R: Runtime> {
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    app: AppHandle<R>,
}

impl<R: Runtime> Scanner<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }

    /// Open the camera and wait for what it ends with. It waits on the activity's thread, so it
    /// is run off the async runtime.
    pub fn scan(&self, texts: &ScanTexts) -> Result<Scan, PortError> {
        #[cfg(target_os = "android")]
        {
            use tauri::Manager as _;
            let plugin = self.app.try_state::<Plugin<R>>().ok_or_else(|| PortError("scanner: plugin missing".into()))?;
            let answer: Value = plugin.0.run_mobile_plugin("scan", texts.clone()).map_err(|e| PortError(e.to_string()))?;
            Ok(scan_of(&answer))
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = texts;
            Err(PortError(CAMERA_UNAVAILABLE.into()))
        }
    }
}

/// The plugin's answer: `{ text }`, else `{ away }`, `{ denied }` or `{ noCamera }`; anything else
/// was left.
pub fn scan_of(answer: &Value) -> Scan {
    if let Some(text) = answer.get("text").and_then(Value::as_str) {
        return Scan::Code(Zeroizing::new(text.to_owned()));
    }
    let said = |name: &str| answer.get(name).and_then(Value::as_bool).unwrap_or(false);
    if said("away") {
        Scan::Away
    } else if said("denied") {
        Scan::Denied
    } else if said("noCamera") {
        Scan::NoCamera
    } else {
        Scan::Left
    }
}

/// What a scan read: the QR code's text; left, nothing; the app left meanwhile, nothing, and the
/// vault locks now, whatever the webview still runs.
pub fn read(core: &Core, scan: Result<Scan, PortError>) -> Result<Option<Zeroizing<String>>, CoreError> {
    match scan {
        Ok(Scan::Code(text)) => Ok(Some(text)),
        Ok(Scan::Left) => Ok(None),
        Ok(Scan::Away) => {
            core.lock_vault();
            Ok(None)
        }
        Ok(Scan::Denied) => Err(ErrorCode::CameraDenied.into()),
        Ok(Scan::NoCamera) => Err(ErrorCode::CameraUnavailable.into()),
        Err(error) => {
            tracing::warn!(%error, "the camera did not answer");
            Err(ErrorCode::CameraUnavailable.into())
        }
    }
}

/// What a scan means for the import: the code read goes into the preview (`true`); otherwise as
/// [`read`] (`false`).
pub fn import(core: &Core, scan: Result<Scan, PortError>) -> Result<bool, CoreError> {
    match read(core, scan)? {
        Some(text) => core.import_scanned(&text).map(|()| true),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugin_answer_reads_as_how_the_scan_ended() {
        let code = "otpauth://totp/A?secret=JBSWY3DP";
        assert_eq!(scan_of(&serde_json::json!({ "text": code })), Scan::Code(Zeroizing::new(code.into())));
        assert_eq!(scan_of(&serde_json::json!({ "away": true })), Scan::Away);
        assert_eq!(scan_of(&serde_json::json!({ "denied": true })), Scan::Denied);
        assert_eq!(scan_of(&serde_json::json!({ "noCamera": true })), Scan::NoCamera);
        assert_eq!(scan_of(&serde_json::json!({ "left": true })), Scan::Left);
        assert_eq!(scan_of(&serde_json::json!({})), Scan::Left);
    }

    #[test]
    fn without_a_camera_the_scan_says_so() {
        let app = tauri::test::mock_app();
        let texts = ScanTexts { prompt: "Point at a QR code".into(), cancel: "Cancel".into() };
        assert!(matches!(Scanner::new(app.handle().clone()).scan(&texts), Err(PortError(m)) if m == CAMERA_UNAVAILABLE));
    }
}
