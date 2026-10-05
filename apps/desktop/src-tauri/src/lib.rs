//! Lockra desktop shell: Tauri commands → `lockra-bridge` → `lockra-core`.
//!
//! The shell owns wiring only: the keychain, clipboard, updater and sync storage adapters, the
//! native file dialogs, the code stream channel, the event forwarding, drag and drop, and
//! screen-capture protection. Every
//! command is `async` (a sync command runs on the main thread and would freeze the webview while
//! Argon2 works). Everything but [`run`] is generic over the Tauri runtime, so `tests/ipc.rs`
//! drives the real command layer on `tauri::test::MockRuntime` without a window.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod biometrics;
pub mod clipboard;
pub mod code_identity;
#[cfg(unix)]
pub mod handoff;
pub mod keychain;
#[cfg(target_os = "macos")]
pub mod keychain_handoff;
pub mod logging;
#[cfg(target_os = "macos")]
pub mod macos_keychain;
pub mod per_build;
pub mod sync;
pub mod updater;

use std::path::PathBuf;
use std::sync::Arc;

use lockra_bridge::{UiCommand, dispatch};
use lockra_core::ports::{
    Biometrics, Clipboard, CodeSink, MemorySecretStore, NoSecretStore, NoUpdater, PortError, SecretStore, SyncTransport, SystemClock, Updater,
};
use lockra_core::ui::{BiometricKind, CodesFrame, Phase, Platform, UI_EVENT_NAME, UiEvent};
use lockra_core::{Core, CoreConfig, CoreError, ErrorCode, KdfCost, Ports};
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, DragDropEvent, Emitter as _, Manager as _, Runtime, State, WebviewWindow, WindowEvent};
use tauri_plugin_dialog::{DialogExt as _, FilePath};
use uuid::Uuid;
use zeroize::Zeroizing;

/// The keychain service; entries are named by vault id.
pub const KEYCHAIN_SERVICE: &str = "dev.lockra.desktop";
/// The event that tells the webview a file drag is over the window (no paths: the shell imports).
pub const DRAG_EVENT_NAME: &str = "lockra://drag";
/// Lets a **debug** build run without an OS keychain (headless smoke runs under Xvfb): `memory`.
/// Release builds ignore it and never fall back to anything weaker than the OS keychain.
pub const DEV_SECRET_STORE_ENV: &str = "LOCKRA_DEV_SECRET_STORE";
/// Lets a **debug** build stand in a check that always passes for Touch ID or Windows Hello
/// (headless smoke runs have no sensor): `touch_id` or `windows_hello`. Release builds ignore it.
pub const DEV_BIOMETRIC_ENV: &str = "LOCKRA_DEV_BIOMETRIC";
/// The main window's label.
pub const MAIN_WINDOW: &str = "main";

/// The shell's Tauri commands, in registration order (lockra-bridge `SHELL_COMMANDS`).
pub const COMMANDS: [&str; 10] = [
    "lockra_dispatch",
    "codes_subscribe",
    "codes_unsubscribe",
    "import_pick_files",
    "backup_save",
    "backup_pick_dir",
    "restore_pick",
    "export_otpauth_file",
    "sync_key_save",
    "sync_pick_folder",
];

/// `true` only when a debug build was explicitly asked for the in-memory keychain.
pub fn dev_memory_store_requested(value: Option<&str>, debug_build: bool) -> bool {
    debug_build && value == Some("memory")
}

/// The check a debug build was explicitly asked to stand in; never one in a release build.
pub fn dev_biometric_requested(value: Option<&str>, debug_build: bool) -> Option<BiometricKind> {
    match value.filter(|_| debug_build)? {
        "touch_id" => Some(BiometricKind::TouchId),
        "windows_hello" => Some(BiometricKind::WindowsHello),
        _ => None,
    }
}

/// What the shell wires into the core; tests replace the platform parts with fakes.
pub struct ShellOptions {
    /// The keychain (default: the OS keychain, or none).
    pub secrets: Option<Arc<dyn SecretStore>>,
    /// The clipboard (default: arboard on a worker thread).
    pub clipboard: Option<Arc<dyn Clipboard>>,
    /// Where the vault lives (default: the app data directory).
    pub data_dir: Option<PathBuf>,
    /// Where `settings.json` lives (default: the app config directory).
    pub config_dir: Option<PathBuf>,
    /// Argon2id cost for new password slots.
    pub kdf: KdfCost,
    /// Refuse a second process (off in the mock-runtime tests: the plugin talks to D-Bus on Linux).
    pub single_instance: bool,
    /// The update source (default: tauri-plugin-updater with [`Self::plugin_updates`], otherwise
    /// none).
    pub updater: Option<Arc<dyn Updater>>,
    /// Register tauri-plugin-updater, which needs `plugins.updater` in the context: `run` does; the
    /// mock-runtime tests' context has none.
    pub plugin_updates: bool,
    /// The sync storage (default: lockra-remote over HTTPS).
    pub sync: Option<Arc<dyn SyncTransport>>,
}

impl Default for ShellOptions {
    fn default() -> Self {
        Self {
            secrets: None,
            clipboard: None,
            data_dir: None,
            config_dir: None,
            kdf: KdfCost::DEFAULT,
            single_instance: true,
            updater: None,
            plugin_updates: false,
            sync: None,
        }
    }
}

/// The keychain for this build: the OS keychain when the probe reaches it; in a debug build the
/// in-memory store when [`DEV_SECRET_STORE_ENV`] asks for it; otherwise none.
pub fn secret_store() -> Arc<dyn SecretStore> {
    if dev_memory_store_requested(std::env::var(DEV_SECRET_STORE_ENV).ok().as_deref(), cfg!(debug_assertions)) {
        tracing::warn!("debug build: the keychain is in memory ({DEV_SECRET_STORE_ENV}=memory)");
        return Arc::new(MemorySecretStore::default());
    }
    // A macOS release keeps its items in ones it created itself, so an update does not ask again
    // (keychain_handoff.rs); local and CI builds are ad hoc and keep the keyring store.
    #[cfg(target_os = "macos")]
    if let Some(store) = keychain_handoff::release_store() {
        return store;
    }
    let store = keychain::KeyringStore::probe(KEYCHAIN_SERVICE);
    if store.status() == lockra_core::ports::KeychainStatus::Available { Arc::new(store) } else { Arc::new(NoSecretStore) }
}

/// The check before "remember on this device" unlocks: the platform's; in a debug build the
/// stand-in [`DEV_BIOMETRIC_ENV`] asks for.
pub fn biometric_check() -> Arc<dyn Biometrics> {
    if let Some(kind) = dev_biometric_requested(std::env::var(DEV_BIOMETRIC_ENV).ok().as_deref(), cfg!(debug_assertions)) {
        tracing::warn!("debug build: a stand-in {kind:?} check that always passes ({DEV_BIOMETRIC_ENV})");
        return Arc::new(biometrics::StandIn(kind));
    }
    Arc::new(biometrics::PlatformBiometrics::default())
}

/// Code frames into a Tauri channel.
struct ChannelSink(Channel<CodesFrame>);

impl CodeSink for ChannelSink {
    fn send(&self, frame: &CodesFrame) -> Result<(), PortError> {
        self.0.send(frame.clone()).map_err(|e| PortError(e.to_string()))
    }
}

fn protect<R: Runtime>(app: &AppHandle<R>, on: bool) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW)
        && let Err(error) = window.set_content_protected(on)
    {
        tracing::warn!(%error, on, "content protection not applied");
    }
}

/// Every core command: the bridge runs it; screen-capture protection follows the secret views.
#[tauri::command]
async fn lockra_dispatch<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>, command: UiCommand) -> Result<Value, CoreError> {
    let (shows, hides) = (command.shows_secret(), command.hides_secret());
    let result = dispatch(&core, command).await;
    if result.is_ok() && shows {
        protect(&app, true);
    }
    if hides {
        protect(&app, false);
    }
    result
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

fn to_path(file: FilePath) -> Option<PathBuf> {
    file.into_path().ok()
}

async fn on_dialog_thread<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, CoreError> {
    tokio::task::spawn_blocking(work).await.map_err(|_| CoreError::from(ErrorCode::Internal))
}

/// Pick files to import (filtered by source); `false` when cancelled.
#[tauri::command]
async fn import_pick_files<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>, kind: Option<String>) -> Result<bool, CoreError> {
    let dialog = app.dialog().clone();
    let picked = on_dialog_thread(move || {
        let builder = dialog.file();
        let builder = match kind.as_deref() {
            Some("images") => builder.add_filter("Images", &["png", "jpg", "jpeg", "webp"]),
            Some("text") => builder.add_filter("Text", &["txt"]),
            Some("backup") => builder.add_filter("Lockra", &["lockrabackup", "lockra"]),
            _ => builder,
        };
        builder.blocking_pick_files()
    })
    .await?;
    let Some(files) = picked else { return Ok(false) };
    let paths: Vec<PathBuf> = files.into_iter().filter_map(to_path).collect();
    core.import_files(paths).await?;
    Ok(true)
}

/// Save a backup through a save dialog; the file name, or `None` when cancelled.
#[tauri::command]
async fn backup_save<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>, separate_password: Option<Zeroizing<String>>) -> Result<Option<String>, CoreError> {
    let dialog = app.dialog().clone();
    let picked =
        on_dialog_thread(move || dialog.file().add_filter("Lockra", &["lockrabackup"]).set_file_name("lockra-backup.lockrabackup").blocking_save_file())
            .await?;
    let Some(path) = picked.and_then(to_path) else { return Ok(None) };
    Ok(Some(core.backup_to(path, separate_password).await?))
}

/// Save the sync key to a file the user chooses: the save dialog first, then the core checks the
/// user is there (the master password, or without it the biometric check) and writes `template`
/// with the key in its slot. `false` when the dialog was cancelled.
#[tauri::command]
async fn sync_key_save<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
    password: Option<Zeroizing<String>>,
    reason: Option<String>,
    file_name: String,
    template: String,
) -> Result<bool, CoreError> {
    let dialog = app.dialog().clone();
    let picked = on_dialog_thread(move || dialog.file().add_filter("Text", &["txt"]).set_file_name(&file_name).blocking_save_file()).await?;
    let Some(path) = picked.and_then(to_path) else { return Ok(false) };
    core.sync_key_save(password, reason, &template, path).await?;
    Ok(true)
}

/// Choose the folder for a sync space (one a cloud drive keeps in sync): the folder dialog, then the
/// core keeps it for the next setup, join or move that asks for "the folder chosen". The folder, to
/// show, or `None` when cancelled.
#[tauri::command]
async fn sync_pick_folder<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>) -> Result<Option<String>, CoreError> {
    let dialog = app.dialog().clone();
    let picked = on_dialog_thread(move || dialog.file().blocking_pick_folder()).await?;
    let Some(dir) = picked.and_then(to_path) else { return Ok(None) };
    core.sync_choose_folder(&dir)?;
    Ok(Some(dir.display().to_string()))
}

/// Choose the automatic backup folder; the folder, or `None` when cancelled.
#[tauri::command]
async fn backup_pick_dir<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>) -> Result<Option<String>, CoreError> {
    let dialog = app.dialog().clone();
    let picked = on_dialog_thread(move || dialog.file().blocking_pick_folder()).await?;
    let Some(dir) = picked.and_then(to_path) else { return Ok(None) };
    core.set_auto_backup_dir(&dir)?;
    Ok(Some(dir.display().to_string()))
}

/// Open a backup for restoring; `false` when cancelled.
#[tauri::command]
async fn restore_pick<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>) -> Result<bool, CoreError> {
    let dialog = app.dialog().clone();
    let picked = on_dialog_thread(move || dialog.file().add_filter("Lockra", &["lockrabackup", "lockra"]).blocking_pick_file()).await?;
    let Some(path) = picked.and_then(to_path) else { return Ok(false) };
    core.restore_open(path).await?;
    Ok(true)
}

/// Write a plain otpauth list: the password is checked before the save dialog opens.
#[tauri::command]
async fn export_otpauth_file<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
    entry_ids: Vec<Uuid>,
    password: Zeroizing<String>,
) -> Result<Option<String>, CoreError> {
    core.verify_password(password.clone()).await?;
    let dialog = app.dialog().clone();
    let picked = on_dialog_thread(move || dialog.file().add_filter("Text", &["txt"]).set_file_name("lockra-export.txt").blocking_save_file()).await?;
    let Some(path) = picked.and_then(to_path) else { return Ok(None) };
    Ok(Some(core.export_otpauth_file(&entry_ids, password, path).await?))
}

/// Forward every core event to the webview; a vault that is no longer unlocked lifts the
/// screen-capture protection (auto-lock locks with a secret view open).
async fn forward_events<R: Runtime>(app: AppHandle<R>, core: Core) {
    let mut events = core.subscribe();
    loop {
        match events.recv().await {
            Ok(event) => {
                if let UiEvent::State { state } = &event
                    && state.phase != Phase::Unlocked
                {
                    protect(&app, false);
                }
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

/// Dropped files go straight to the import; the webview only hears that a drag is over it.
fn watch_drops<R: Runtime>(window: &WebviewWindow<R>, core: Core) {
    let app = window.app_handle().clone();
    window.on_window_event(move |event| {
        let WindowEvent::DragDrop(drag) = event else { return };
        match drag {
            DragDropEvent::Enter { .. } => {
                let _ = app.emit(DRAG_EVENT_NAME, "enter");
            }
            DragDropEvent::Leave => {
                let _ = app.emit(DRAG_EVENT_NAME, "leave");
            }
            DragDropEvent::Drop { paths, .. } => {
                let _ = app.emit(DRAG_EVENT_NAME, "leave");
                let core = core.clone();
                let paths = paths.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = core.import_files(paths).await {
                        tracing::info!(?error, "a drop imported nothing");
                    }
                });
            }
            _ => {}
        }
    });
}

/// The window's focus is the app being in front, where sync runs more often and coming back runs
/// it (the core decides when). Watched before the window shows, so its first focus is heard.
fn watch_focus<R: Runtime>(window: &WebviewWindow<R>, core: Core) {
    core.set_foreground(window.is_focused().unwrap_or(true));
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(focused) = event {
            core.set_foreground(*focused);
        }
    });
}

/// The app with every command, the core and its wiring; `run` adds the real context.
pub fn build_app<R: Runtime>(builder: tauri::Builder<R>, options: ShellOptions) -> tauri::Builder<R> {
    let builder = if options.single_instance {
        builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
    } else {
        builder
    };
    let builder = if options.plugin_updates { builder.plugin(tauri_plugin_updater::Builder::new().build()) } else { builder };
    builder
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            lockra_dispatch,
            codes_subscribe,
            codes_unsubscribe,
            import_pick_files,
            backup_save,
            backup_pick_dir,
            restore_pick,
            export_otpauth_file,
            sync_key_save,
            sync_pick_folder
        ])
        .setup(move |app| {
            let data_dir = match options.data_dir.clone() {
                Some(dir) => dir,
                None => app.path().app_data_dir()?,
            };
            let config_dir = match options.config_dir.clone() {
                Some(dir) => dir,
                None => app.path().app_config_dir()?,
            };
            let secrets = options.secrets.clone().unwrap_or_else(secret_store);
            let clipboard = options.clipboard.clone().unwrap_or_else(|| Arc::new(clipboard::ArboardClipboard::spawn()));
            let config =
                CoreConfig { data_dir, config_dir, app_version: app.package_info().version.to_string(), kdf: options.kdf, platform: Platform::current() };
            let updater: Arc<dyn Updater> = match options.updater.clone() {
                Some(updater) => updater,
                None if options.plugin_updates => Arc::new(updater::PluginUpdater::new(app.handle().clone(), updater::install_method())),
                None => Arc::new(NoUpdater),
            };
            let sync: Arc<dyn SyncTransport> = options.sync.clone().unwrap_or_else(|| Arc::new(sync::Storages));
            let ports = Ports { secrets, clipboard, clock: Arc::new(SystemClock), updater, sync, biometrics: biometric_check() };
            // The core's scheduler is a tokio task: start it inside Tauri's runtime.
            let core = tauri::async_runtime::block_on(async move { Core::start(config, ports) });
            app.manage(core.clone());
            tauri::async_runtime::spawn(forward_events(app.handle().clone(), core.clone()));
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                watch_focus(&window, core.clone());
                watch_drops(&window, core);
                // Declared invisible so it never flashes white before the theme is applied.
                let _ = window.show();
                let _ = window.set_focus();
            }
            Ok(())
        })
}

/// The desktop entry point.
pub fn run() {
    logging::init();
    // A staged update started by the running build to take its keychain entries: that, and nothing
    // else, before any window or the single-instance check.
    #[cfg(target_os = "macos")]
    if keychain_handoff::asked_for_handoff() {
        std::process::exit(keychain_handoff::take_handoff());
    }
    #[cfg(all(target_os = "macos", debug_assertions))]
    if let Some(entry) = keychain_handoff::asked_for_probe() {
        std::process::exit(keychain_handoff::probe(&entry));
    }
    let options = ShellOptions { plugin_updates: true, ..ShellOptions::default() };
    let app = build_app(tauri::Builder::default(), options).run(tauri::generate_context!());
    if let Err(error) = app {
        tracing::error!(%error, "Lockra could not start");
        std::process::exit(1);
    }
}
