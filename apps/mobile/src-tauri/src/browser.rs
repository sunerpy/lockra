//! The phone's browser for one page, a Lockra release's, through `BrowserPlugin.kt` on Android (an
//! `ACTION_VIEW` intent): Rust names the address, the webview cannot. Other builds of this crate
//! (the host tests) have no browser and say so.

use lockra_core::ports::PortError;
use tauri::{AppHandle, Runtime};

/// Why there is no browser on this build.
pub const BROWSER_UNAVAILABLE: &str = "browser: this build has no browser";

/// The Android plugin (`BrowserPlugin.kt`).
#[cfg(target_os = "android")]
struct Plugin<R: Runtime>(tauri::plugin::PluginHandle<R>);

/// Registers the Android plugin; a no-op elsewhere.
pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("lockra-browser")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager as _;
                let handle = _api.register_android_plugin("dev.lockra.mobile", "BrowserPlugin")?;
                _app.manage(Plugin(handle));
            }
            Ok(())
        })
        .build()
}

/// The phone's browser.
pub struct Browser<R: Runtime> {
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    app: AppHandle<R>,
}

impl<R: Runtime> Browser<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }

    /// Open `url` in the browser; `false` when no app could. It waits for the activity's thread, so
    /// it is run off the async runtime.
    pub fn open(&self, url: &str) -> Result<bool, PortError> {
        #[cfg(target_os = "android")]
        {
            use tauri::Manager as _;
            let plugin = self.app.try_state::<Plugin<R>>().ok_or_else(|| PortError("browser: plugin missing".into()))?;
            let answer: serde_json::Value = plugin.0.run_mobile_plugin("open", serde_json::json!({ "url": url })).map_err(|e| PortError(e.to_string()))?;
            Ok(answer.get("opened").and_then(serde_json::Value::as_bool).unwrap_or(false))
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = url;
            Err(PortError(BROWSER_UNAVAILABLE.into()))
        }
    }
}
