//! Lockra mobile shell (Android): Tauri commands → `lockra-bridge` → `lockra-core`, as on the
//! desktop (apps/desktop/src-tauri), with the phone's own adapters. Each native capability is a
//! small Tauri plugin whose Kotlin half lives in `gen/android` (the clipboard, the camera, the
//! photo picker); its answers stay in Rust. The webview gets the state and the codes, never a
//! secret it did not ask to reveal.
//! Everything but [`run`] is generic over the Tauri runtime, so `tests/ipc.rs` drives the real
//! command layer on `tauri::test::MockRuntime` on the host.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod clipboard;
pub mod files;
pub mod scanner;

use std::path::PathBuf;
use std::sync::Arc;

use lockra_bridge::{UiCommand, dispatch};
use lockra_core::ports::{
    Biometrics, Clipboard, CodeSink, NoBiometrics, NoSecretStore, NoUpdater, PortError, RemoteStore, SecretStore, SyncError, SyncTransport, SystemClock,
};
use lockra_core::ui::{CodesFrame, Platform, UI_EVENT_NAME, UiEvent};
use lockra_core::{Core, CoreConfig, CoreError, ErrorCode, KdfCost, Ports, StorageConfig};
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter as _, Manager as _, Runtime, State};

/// The shell's Tauri commands, in registration order (lockra-bridge `PHONE_COMMANDS`).
pub const COMMANDS: [&str; 5] = ["lockra_dispatch", "codes_subscribe", "codes_unsubscribe", "import_pick_files", "import_scan"];

/// What the shell wires into the core; tests replace the platform parts with fakes.
pub struct ShellOptions {
    /// The keychain (default: none yet; the Android Keystore comes with the fingerprint).
    pub secrets: Option<Arc<dyn SecretStore>>,
    /// The clipboard (default: the phone's, src/clipboard.rs).
    pub clipboard: Option<Arc<dyn Clipboard>>,
    /// The check before the remembered key unlocks (default: none yet).
    pub biometrics: Option<Arc<dyn Biometrics>>,
    /// Where the vault lives (default: the app's private data directory).
    pub data_dir: Option<PathBuf>,
    /// Where `settings.json` lives (default: the app's private config directory).
    pub config_dir: Option<PathBuf>,
    /// Argon2id cost for new password slots.
    pub kdf: KdfCost,
}

impl Default for ShellOptions {
    fn default() -> Self {
        Self { secrets: None, clipboard: None, biometrics: None, data_dir: None, config_dir: None, kdf: KdfCost::DEFAULT }
    }
}

/// No sync storage on the phone yet: a space is set up on the desktop.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSync;

impl SyncTransport for NoSync {
    fn open(&self, _config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        Err(SyncError::Storage("sync is not available on this phone yet".into()))
    }
}

/// Code frames into a Tauri channel.
struct ChannelSink(Channel<CodesFrame>);

impl CodeSink for ChannelSink {
    fn send(&self, frame: &CodesFrame) -> Result<(), PortError> {
        self.0.send(frame.clone()).map_err(|e| PortError(e.to_string()))
    }
}

/// Every core command: the bridge runs it. The window never shows in screenshots or the recent
/// apps (FLAG_SECURE, MainActivity.kt), so a secret view needs nothing more here.
#[tauri::command]
async fn lockra_dispatch(core: State<'_, Core>, command: UiCommand) -> Result<Value, CoreError> {
    dispatch(&core, command).await
}

/// Stream code frames to `on_frame`, replacing any previous stream.
#[tauri::command]
async fn codes_subscribe(core: State<'_, Core>, on_frame: Channel<CodesFrame>) -> Result<(), CoreError> {
    core.subscribe_codes(Arc::new(ChannelSink(on_frame)));
    Ok(())
}

/// Stop the code stream.
#[tauri::command]
async fn codes_unsubscribe(core: State<'_, Core>) -> Result<(), CoreError> {
    core.unsubscribe_codes();
    Ok(())
}

/// Pick files to import: images with the photo picker (`kind` "images"), any other kind with the
/// system's file picker; `false` when none was picked.
#[tauri::command]
async fn import_pick_files<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>, kind: Option<String>) -> Result<bool, CoreError> {
    let picker = files::Files::new(app);
    let images = kind.as_deref() == Some("images");
    let picked = tauri::async_runtime::spawn_blocking(move || if images { picker.pick_images() } else { picker.pick_files() })
        .await
        .map_err(|_| CoreError::from(ErrorCode::Internal))?;
    files::import(&core, picked).await
}

/// Scan a QR code with the camera into the import preview; `false` when left without one.
/// `prompt` and `cancel` are the camera page's words, in the webview's language.
#[tauri::command]
async fn import_scan<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>, prompt: String, cancel: String) -> Result<bool, CoreError> {
    let camera = scanner::Scanner::new(app);
    let texts = scanner::ScanTexts { prompt, cancel };
    let scan = tauri::async_runtime::spawn_blocking(move || camera.scan(&texts)).await.map_err(|_| CoreError::from(ErrorCode::Internal))?;
    scanner::import(&core, scan)
}

/// Forward every core event to the webview.
async fn forward_events<R: Runtime>(app: AppHandle<R>, core: Core) {
    let mut events = core.subscribe();
    loop {
        match events.recv().await {
            Ok(event) => {
                if let Err(error) = app.emit(UI_EVENT_NAME, &event) {
                    tracing::warn!(%error, "event not delivered");
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "events dropped; resending the state");
                let _ = app.emit(UI_EVENT_NAME, &UiEvent::State { state: Box::new(core.state()) });
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// The app with its commands, plugins and core.
pub fn build_app<R: Runtime>(builder: tauri::Builder<R>, options: ShellOptions) -> tauri::Builder<R> {
    builder
        .plugin(clipboard::init())
        .plugin(files::init())
        .plugin(scanner::init())
        .invoke_handler(tauri::generate_handler![lockra_dispatch, codes_subscribe, codes_unsubscribe, import_pick_files, import_scan])
        .setup(move |app| {
            let data_dir = match options.data_dir.clone() {
                Some(dir) => dir,
                None => app.path().app_data_dir()?,
            };
            let config_dir = match options.config_dir.clone() {
                Some(dir) => dir,
                None => app.path().app_config_dir()?,
            };
            let secrets: Arc<dyn SecretStore> = options.secrets.clone().unwrap_or_else(|| Arc::new(NoSecretStore));
            let clipboard: Arc<dyn Clipboard> = options.clipboard.clone().unwrap_or_else(|| Arc::new(clipboard::PhoneClipboard::new(app.handle().clone())));
            let biometrics: Arc<dyn Biometrics> = options.biometrics.clone().unwrap_or_else(|| Arc::new(NoBiometrics));
            let config =
                CoreConfig { data_dir, config_dir, app_version: app.package_info().version.to_string(), kdf: options.kdf, platform: Platform::current() };
            let ports = Ports { secrets, clipboard, clock: Arc::new(SystemClock), updater: Arc::new(NoUpdater), sync: Arc::new(NoSync), biometrics };
            // The core's scheduler is a tokio task: start it inside Tauri's runtime.
            let core = tauri::async_runtime::block_on(async move { Core::start(config, ports) });
            app.manage(core.clone());
            tauri::async_runtime::spawn(forward_events(app.handle().clone(), core));
            Ok(())
        })
}

/// The phone's entry point (and `cargo run` on a desktop, for development).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ =
        tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "lockra=info".into())).try_init();
    let app = build_app(tauri::Builder::default(), ShellOptions::default()).run(tauri::generate_context!());
    if let Err(error) = app {
        tracing::error!(%error, "Lockra could not start");
        std::process::exit(1);
    }
}
