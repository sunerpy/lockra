//! The phone's pickers, through `FilesPlugin.kt` on Android: the photo picker and the system's file
//! picker. What they pick comes to Rust as bytes (no path, and nothing of it reaches the webview)
//! and goes into the import preview or the restore; a backup goes out the same way, as bytes saved
//! where the user picks. Other builds of this crate (the host tests) have no picker and say so.

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

/// The name a backup is offered under, and its type.
pub const BACKUP_NAME: &str = "lockra-backup.lockrabackup";
pub const BACKUP_MIME: &str = "application/octet-stream";

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
        self.call("pickImages", serde_json::json!({ "limit": PICK_LIMIT, "maxBytes": lockra_core::MAX_IMPORT_BYTES })).and_then(|answer| picked_of(&answer))
    }

    /// Open the system's file picker on every kind of file and wait, as `pick_images` does: an
    /// otpauth list, a Lockra backup, Microsoft's database and its log.
    pub fn pick_files(&self) -> Result<Vec<PickedFile>, PortError> {
        self.call("pickFiles", serde_json::json!({ "maxBytes": lockra_core::MAX_IMPORT_BYTES })).and_then(|answer| picked_of(&answer))
    }

    /// Open one file with the system's file picker: what it holds, `None` when it was left.
    pub fn pick_file(&self) -> Result<Option<PickedFile>, PortError> {
        let answer = self.call("pickFile", serde_json::json!({ "maxBytes": lockra_core::MAX_IMPORT_BYTES }))?;
        Ok(picked_of(&answer)?.into_iter().next())
    }

    /// Save `bytes` where the user picks, offering `name`: the name the file got, `None` when the
    /// picker was left.
    pub fn save(&self, name: &str, mime: &str, bytes: &[u8]) -> Result<Option<String>, PortError> {
        let answer = self.call("saveFile", serde_json::json!({ "name": name, "mime": mime, "data": BASE64.encode(bytes) }))?;
        Ok(saved_of(&answer))
    }

    /// Run a plugin command; it waits on the activity's thread, so it is run off the async runtime.
    fn call(&self, command: &str, args: Value) -> Result<Value, PortError> {
        #[cfg(target_os = "android")]
        {
            use tauri::Manager as _;
            let plugin = self.app.try_state::<Plugin<R>>().ok_or_else(|| PortError("files: plugin missing".into()))?;
            plugin.0.run_mobile_plugin(command, args).map_err(|e| PortError(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (command, args);
            Err(PortError(PICKER_UNAVAILABLE.into()))
        }
    }
}

/// The save picker's answer: `{ name }`, or nothing when it was left.
pub fn saved_of(answer: &Value) -> Option<String> {
    answer.get("name").and_then(Value::as_str).map(str::to_owned)
}

/// What a backup's save means: recorded under its name (the notice, the time of the last backup),
/// or nothing when the picker was left.
pub fn saved(core: &Core, saved: Result<Option<String>, PortError>) -> Result<Option<String>, CoreError> {
    let name = saved.map_err(|error| {
        tracing::warn!(%error, "the backup could not be saved");
        CoreError::from(ErrorCode::IoFailed)
    })?;
    if let Some(name) = &name {
        core.backup_recorded(name);
    }
    Ok(name)
}

/// What a backup picked means: open for restoring (`true`), or nothing picked (`false`).
pub async fn restore(core: &Core, picked: Result<Option<PickedFile>, PortError>) -> Result<bool, CoreError> {
    let file = picked.map_err(|error| {
        tracing::warn!(%error, "the file picker did not answer");
        CoreError::from(ErrorCode::IoFailed)
    })?;
    let Some(file) = file else { return Ok(false) };
    core.restore_open_bytes(file.name, file.bytes).await.map(|()| true)
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
        assert!(matches!(Files::new(app.handle().clone()).pick_file(), Err(PortError(m)) if m == PICKER_UNAVAILABLE));
        assert!(matches!(Files::new(app.handle().clone()).save(BACKUP_NAME, BACKUP_MIME, b"x"), Err(PortError(m)) if m == PICKER_UNAVAILABLE));
    }

    #[test]
    fn the_save_answer_reads_as_the_name_given() {
        assert_eq!(saved_of(&serde_json::json!({ "name": "lockra-backup (1).lockrabackup" })).as_deref(), Some("lockra-backup (1).lockrabackup"));
        assert_eq!(saved_of(&serde_json::json!({})), None);
    }
}
