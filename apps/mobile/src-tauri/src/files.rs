//! The phone's pickers for the import, through `FilesPlugin.kt` on Android: the photo picker and
//! the system's file picker. What they pick comes to Rust as bytes (no path, and nothing of it
//! reaches the webview) and goes into the import preview. Other builds of this crate (the host
//! tests) have no picker and say so.

use data_encoding::BASE64;
use lockra_core::ports::PortError;
use lockra_core::{Core, CoreError, ErrorCode, PickedFile};
use serde_json::Value;
use tauri::{AppHandle, Runtime};
use zeroize::Zeroizing;

/// Why there is no photo picker on this build.
pub const PICKER_UNAVAILABLE: &str = "files: this build has no photo picker";

/// At most this many images at once.
pub const PICK_LIMIT: u32 = 10;

/// The Android plugin (`FilesPlugin.kt`).
#[cfg(target_os = "android")]
struct Plugin<R: Runtime>(tauri::plugin::PluginHandle<R>);

/// Registers the Android plugin; a no-op elsewhere.
pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("lockra-files")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager as _;
                let handle = _api.register_android_plugin("dev.lockra.mobile", "FilesPlugin")?;
                _app.manage(Plugin(handle));
            }
            Ok(())
        })
        .build()
}

/// The phone's pickers.
pub struct Files<R: Runtime> {
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    app: AppHandle<R>,
}

impl<R: Runtime> Files<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }

    /// Open the photo picker and wait: the images picked, none when it was left. A file past the
    /// import's size comes one byte too long, so the core says it cannot be read. It waits on the
    /// activity's thread, so it is run off the async runtime.
    pub fn pick_images(&self) -> Result<Vec<PickedFile>, PortError> {
        #[cfg(target_os = "android")]
        {
            use tauri::Manager as _;
            let plugin = self.app.try_state::<Plugin<R>>().ok_or_else(|| PortError("files: plugin missing".into()))?;
            let args = serde_json::json!({ "limit": PICK_LIMIT, "maxBytes": lockra_core::MAX_IMPORT_BYTES });
            let answer: Value = plugin.0.run_mobile_plugin("pickImages", args).map_err(|e| PortError(e.to_string()))?;
            picked_of(&answer)
        }
        #[cfg(not(target_os = "android"))]
        {
            Err(PortError(PICKER_UNAVAILABLE.into()))
        }
    }

    /// Open the system's file picker on every kind of file and wait, as `pick_images` does: an
    /// otpauth list, a Lockra backup, Microsoft's database and its log.
    pub fn pick_files(&self) -> Result<Vec<PickedFile>, PortError> {
        #[cfg(target_os = "android")]
        {
            use tauri::Manager as _;
            let plugin = self.app.try_state::<Plugin<R>>().ok_or_else(|| PortError("files: plugin missing".into()))?;
            let args = serde_json::json!({ "maxBytes": lockra_core::MAX_IMPORT_BYTES });
            let answer: Value = plugin.0.run_mobile_plugin("pickFiles", args).map_err(|e| PortError(e.to_string()))?;
            picked_of(&answer)
        }
        #[cfg(not(target_os = "android"))]
        {
            Err(PortError(PICKER_UNAVAILABLE.into()))
        }
    }
}

/// The plugin's answer `{ files: [{ name, data }] }`, the data in Base64.
pub fn picked_of(answer: &Value) -> Result<Vec<PickedFile>, PortError> {
    let Some(files) = answer.get("files").and_then(Value::as_array) else { return Ok(Vec::new()) };
    files
        .iter()
        .map(|file| {
            let name = file.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
            let data = file.get("data").and_then(Value::as_str).ok_or_else(|| PortError(format!("files: no data for {name}")))?;
            let bytes = BASE64.decode(data.as_bytes()).map_err(|e| PortError(format!("files: {name}: {e}")))?;
            Ok(PickedFile { name, bytes: Zeroizing::new(bytes) })
        })
        .collect()
}

/// What a pick means for the import: the images go into the preview (`true`); none, nothing
/// (`false`).
pub async fn import(core: &Core, picked: Result<Vec<PickedFile>, PortError>) -> Result<bool, CoreError> {
    let files = picked.map_err(|error| {
        tracing::warn!(%error, "the photo picker did not answer");
        CoreError::from(ErrorCode::IoFailed)
    })?;
    if files.is_empty() {
        return Ok(false);
    }
    core.import_picked(files).await.map(|()| true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugin_answer_reads_as_the_files_picked() {
        let answer = serde_json::json!({ "files": [
            { "name": "export.png", "data": BASE64.encode(b"\x89PNG") },
            { "name": "empty.png", "data": "" },
        ] });
        let picked = picked_of(&answer).unwrap();
        let names: Vec<(&str, &[u8])> = picked.iter().map(|f| (f.name.as_str(), f.bytes.as_slice())).collect();
        assert_eq!(names, [("export.png", b"\x89PNG".as_slice()), ("empty.png", b"".as_slice())]);
        assert!(picked_of(&serde_json::json!({ "files": [] })).unwrap().is_empty());
        assert!(picked_of(&serde_json::json!({})).unwrap().is_empty(), "left: nothing picked");
        assert!(picked_of(&serde_json::json!({ "files": [{ "name": "x.png", "data": "not base64!" }] })).is_err());
        assert!(picked_of(&serde_json::json!({ "files": [{ "name": "x.png" }] })).is_err());
    }

    #[test]
    fn without_a_picker_the_pick_says_so() {
        let app = tauri::test::mock_app();
        assert!(matches!(Files::new(app.handle().clone()).pick_images(), Err(PortError(m)) if m == PICKER_UNAVAILABLE));
        assert!(matches!(Files::new(app.handle().clone()).pick_files(), Err(PortError(m)) if m == PICKER_UNAVAILABLE));
    }
}
