//! The phone's clipboard as the core's `Clipboard` port, through `ClipboardPlugin.kt` on Android:
//! a code goes on marked sensitive (`EXTRA_IS_SENSITIVE`: the system's paste preview and the
//! keyboard's clipboard history do not show it) and comes off again only while it is still that
//! code. Other builds of this crate (the host tests) have no phone clipboard and say so.

use lockra_core::ports::{Clipboard, ClipboardImage, PortError};
use tauri::{AppHandle, Runtime};
use zeroize::Zeroizing;

/// Why there is no clipboard on this build.
pub const CLIPBOARD_UNAVAILABLE: &str = "clipboard: this build has no phone clipboard";

/// The Android plugin (`ClipboardPlugin.kt`).
#[cfg(target_os = "android")]
struct Plugin<R: Runtime>(tauri::plugin::PluginHandle<R>);

/// Registers the Android plugin; a no-op elsewhere.
pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("lockra-clipboard")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager as _;
                let handle = _api.register_android_plugin("dev.lockra.mobile", "ClipboardPlugin")?;
                _app.manage(Plugin(handle));
            }
            Ok(())
        })
        .build()
}

/// The core's clipboard on the phone.
pub struct PhoneClipboard<R: Runtime> {
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    app: AppHandle<R>,
}

impl<R: Runtime> PhoneClipboard<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }

    /// Run a plugin command; it waits for the activity's thread, so it is never called from the
    /// setup (which runs there).
    fn call(&self, command: &str, args: serde_json::Value) -> Result<serde_json::Value, PortError> {
        #[cfg(target_os = "android")]
        {
            use tauri::Manager as _;
            let plugin = self.app.try_state::<Plugin<R>>().ok_or_else(|| PortError("clipboard: plugin missing".into()))?;
            plugin.0.run_mobile_plugin(command, args).map_err(|e| PortError(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (command, args);
            Err(PortError(CLIPBOARD_UNAVAILABLE.into()))
        }
    }
}

impl<R: Runtime> Clipboard for PhoneClipboard<R> {
    fn set_secret_text(&self, text: &str) -> Result<(), PortError> {
        self.call("writeSecret", serde_json::json!({ "text": text })).map(|_| ())
    }

    fn text(&self) -> Result<Option<Zeroizing<String>>, PortError> {
        let answer = self.call("readText", serde_json::json!({}))?;
        Ok(text_of(&answer).map(Zeroizing::new))
    }

    /// Images come from the photo picker on the phone, not the clipboard.
    fn image(&self) -> Result<Option<ClipboardImage>, PortError> {
        Ok(None)
    }

    fn clear_if(&self, expected: &str) -> Result<bool, PortError> {
        let answer = self.call("clearIf", serde_json::json!({ "expected": expected }))?;
        Ok(answer.get("cleared").and_then(serde_json::Value::as_bool).unwrap_or(false))
    }
}

/// The plugin's answer `{ "text": "…" | null }`: an empty clipboard is no text.
pub fn text_of(answer: &serde_json::Value) -> Option<String> {
    answer.get("text").and_then(serde_json::Value::as_str).filter(|t| !t.is_empty()).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugin_answer_reads_as_text_or_none() {
        assert_eq!(text_of(&serde_json::json!({ "text": "otpauth://totp/A?secret=JBSWY3DP" })).as_deref(), Some("otpauth://totp/A?secret=JBSWY3DP"));
        assert_eq!(text_of(&serde_json::json!({ "text": "" })), None);
        assert_eq!(text_of(&serde_json::json!({ "text": null })), None);
        assert_eq!(text_of(&serde_json::json!({})), None);
    }

    #[test]
    fn without_a_phone_clipboard_every_call_says_so() {
        let app = tauri::test::mock_app();
        let clipboard = PhoneClipboard::new(app.handle().clone());
        let unavailable = |result: Result<(), PortError>| matches!(result, Err(PortError(m)) if m == CLIPBOARD_UNAVAILABLE);
        assert!(unavailable(clipboard.set_secret_text("123456")));
        assert!(unavailable(clipboard.text().map(|_| ())));
        assert!(unavailable(clipboard.clear_if("123456").map(|_| ())));
        assert!(matches!(clipboard.image(), Ok(None)));
    }
}
