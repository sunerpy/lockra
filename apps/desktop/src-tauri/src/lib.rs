//! Lockra desktop shell: Tauri commands → `lockra-bridge` → `lockra-core`.
//!
//! The shell owns wiring only: the keychain, clipboard, updater and sync storage adapters, the
//! native file dialogs, the code stream channel, the event forwarding, drag and drop,
//! screen-capture protection, the tray and the quick search's global shortcut. Every
//! command is `async` (a sync command runs on the main thread and would freeze the webview while
//! Argon2 works). Everything but [`run`] is generic over the Tauri runtime, so `tests/ipc.rs`
//! drives the real command layer on `tauri::test::MockRuntime` without a window.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod background;
pub mod biometrics;
pub mod browser;
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
pub mod quick_access;
pub mod sync;
pub mod updater;

use std::path::PathBuf;
use std::sync::Arc;

use lockra_bridge::{UiCommand, dispatch};
use lockra_core::ports::{
    Biometrics, Clipboard, CodeSink, MemorySecretStore, NoSecretStore, NoUpdater, PortError, SecretStore, SyncTransport, SystemClock, Updater,
};
use lockra_core::ui::{BiometricKind, CodesFrame, Features, Phase, Platform, UI_EVENT_NAME, UiEvent};
use lockra_core::{Core, CoreConfig, CoreError, ErrorCode, ItemsFormat, KdfCost, Ports};
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
pub const COMMANDS: [&str; 21] = [
    "lockra_dispatch",
    "codes_subscribe",
    "codes_unsubscribe",
    "import_pick_files",
    "import_pick_key_file",
    "entry_add_image",
    "backup_save",
    "backup_pick_dir",
    "restore_pick",
    "export_otpauth_file",
    "export_items_file",
    "sync_key_save",
    "sync_pick_folder",
    "browser_enable",
    "browser_disable",
    "browser_status",
    "browser_extension",
    "browser_extension_copy",
    "browser_extension_reveal",
    "browser_extension_download",
    "quick_access_done",
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
    /// Where the browser channel listens (default: `browser::socket::place` of the app's folders).
    pub browser_socket: Option<PathBuf>,
    /// Show the tray icon (off in the mock-runtime tests: tray-icon builds a native one).
    pub tray: bool,
    /// Register the quick search's shortcut with the system (off in the mock-runtime tests, which
    /// then report every shortcut unavailable, as a Wayland desktop does).
    pub global_shortcuts: bool,
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
            browser_socket: None,
            tray: true,
            global_shortcuts: true,
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
            Some("logins") => builder.add_filter("Password exports", &["csv", "json", "1pux", "kdbx"]),
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

/// Choose the key file of the KeePass database waiting in the import: the open dialog (only when
/// one waits), then the core reads the file and keeps the key it brings, never the file or its
/// path. `false` when cancelled.
#[tauri::command]
async fn import_pick_key_file<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>) -> Result<bool, CoreError> {
    core.import_awaits_key_file()?;
    let dialog = app.dialog().clone();
    let picked = on_dialog_thread(move || dialog.file().blocking_pick_file()).await?;
    let Some(path) = picked.and_then(to_path) else { return Ok(false) };
    core.import_key_file_from(path).await?;
    Ok(true)
}

/// The one account the QR code in an image holds (a screenshot of a site's two-factor setup): the
/// open dialog, then the core reads the image and adds the account at once, for a login's form to
/// link it. Its id, or `None` when cancelled.
#[tauri::command]
async fn entry_add_image<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>) -> Result<Option<Uuid>, CoreError> {
    let dialog = app.dialog().clone();
    let picked = on_dialog_thread(move || dialog.file().add_filter("Images", &["png", "jpg", "jpeg", "webp"]).blocking_pick_file()).await?;
    let Some(path) = picked.and_then(to_path) else { return Ok(None) };
    core.add_from_image(path).await.map(Some)
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

/// Save the logins `ids` as a KeePass database or Bitwarden's plain CSV: the core checks the user
/// is there (the master password, or without it Touch ID or Windows Hello asked with `reason`)
/// and what was chosen, and makes the file, before the save dialog opens; the webview hears the
/// file's name only. `None` when the dialog was cancelled.
#[tauri::command]
#[allow(clippy::too_many_arguments, reason = "a command's arguments are the webview's named fields")]
async fn export_items_file<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
    ids: Vec<Uuid>,
    format: ItemsFormat,
    password: Option<Zeroizing<String>>,
    reason: Option<String>,
    acknowledged: bool,
    kdbx_password: Option<Zeroizing<String>>,
) -> Result<Option<String>, CoreError> {
    let file = core.export_items(&ids, format, password, reason, acknowledged, kdbx_password).await?;
    let (filter, extension) = match file.format() {
        ItemsFormat::Kdbx => ("KeePass", "kdbx"),
        ItemsFormat::Csv => ("CSV", "csv"),
    };
    let name = file.name();
    let dialog = app.dialog().clone();
    let picked = on_dialog_thread(move || dialog.file().add_filter(filter, &[extension]).set_file_name(name).blocking_save_file()).await?;
    let Some(path) = picked.and_then(to_path) else { return Ok(None) };
    Ok(Some(core.export_items_write(file, path).await?))
}

/// Forward every core event to the webview; a vault that is no longer unlocked lifts the
/// screen-capture protection (auto-lock locks with a secret view open).
async fn forward_events<R: Runtime>(app: AppHandle<R>, core: Core, tray: bool) {
    let mut events = core.subscribe();
    loop {
        match events.recv().await {
            Ok(event) => {
                if let UiEvent::State { state } = &event {
                    if state.phase != Phase::Unlocked {
                        protect(&app, false);
                        // A palette the quick search opened went with the lock.
                        quick_access::forget(&app);
                    }
                    // The tray's menu follows the language, the lock and what the build offers.
                    if tray {
                        background::sync_tray(&app, background::TrayModel::of(state, core.quick_access_offered(), sys_locale::get_locale().as_deref()));
                    }
                    // The quick search's shortcut follows it too.
                    quick_access::sync(&app, state.quick_access.shortcut.as_deref());
                }
                if let UiEvent::Notice { notice } = &event
                    && background::brings_window_up(notice)
                {
                    background::show_main(&app);
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

/// While Lockra runs in the background, closing the window hides it (the tray's icon and its 打开
/// bring it back); otherwise the window closes and Lockra quits, as it always has.
fn watch_close<R: Runtime>(window: &WebviewWindow<R>, core: Core) {
    let hidden = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event
            && core.state().settings.run_in_background
        {
            api.prevent_close();
            let _ = hidden.hide();
        }
    });
}

/// The app with every command, the core and its wiring; `run` adds the real context.
pub fn build_app<R: Runtime>(builder: tauri::Builder<R>, options: ShellOptions) -> tauri::Builder<R> {
    let builder = if options.single_instance {
        builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| match quick_access::start_reason(&args) {
            // `lockra-desktop --palette` (bound in the desktop where apps get no global shortcut):
            // the window with the command palette.
            quick_access::Start::Palette => quick_access::summon(app),
            // Started again for the browser (the host's `start_app`): Lockra runs already, and
            // the window stays as it is.
            quick_access::Start::Browser => {}
            quick_access::Start::Window => background::show_main(app),
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
            import_pick_key_file,
            entry_add_image,
            backup_save,
            backup_pick_dir,
            restore_pick,
            export_otpauth_file,
            export_items_file,
            sync_key_save,
            sync_pick_folder,
            browser::service::browser_enable,
            browser::service::browser_disable,
            browser::service::browser_status,
            browser::service::browser_extension,
            browser::service::browser_extension_copy,
            browser::service::browser_extension_reveal,
            browser::service::browser_extension_download,
            quick_access::quick_access_done
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
            let socket = options.browser_socket.clone().unwrap_or_else(|| browser::socket::place(&data_dir, &config_dir));
            let secrets = options.secrets.clone().unwrap_or_else(secret_store);
            let clipboard = options.clipboard.clone().unwrap_or_else(|| Arc::new(clipboard::ArboardClipboard::spawn()));
            let config = CoreConfig {
                data_dir,
                config_dir,
                app_version: app.package_info().version.to_string(),
                kdf: options.kdf,
                platform: Platform::current(),
                features: Features::default(),
            };
            let updater: Arc<dyn Updater> = match options.updater.clone() {
                Some(updater) => updater,
                None if options.plugin_updates => Arc::new(updater::PluginUpdater::new(app.handle().clone(), updater::install_method())),
                None => Arc::new(NoUpdater),
            };
            let sync: Arc<dyn SyncTransport> = options.sync.clone().unwrap_or_else(|| Arc::new(sync::Storages));
            app.manage(browser::service::PathClipboard(clipboard.clone()));
            let ports = Ports { secrets, clipboard, clock: Arc::new(SystemClock), updater, sync, biometrics: biometric_check() };
            // The core's scheduler is a tokio task: start it inside Tauri's runtime.
            let core = tauri::async_runtime::block_on(async move { Core::start(config, ports) });
            app.manage(core.clone());
            app.manage(quick_access::QuickAccess::new(options.global_shortcuts));
            let service = browser::service::BrowserService::new(socket);
            app.manage(service.clone());
            tauri::async_runtime::spawn(browser::service::resume(core.clone(), service));
            let state = core.state();
            let run_in_background = state.settings.run_in_background;
            // The tray, there while Lockra runs (closing the window hides it only while it runs in
            // the background: `watch_close`).
            if options.tray {
                app.manage(background::TrayState::default());
                background::sync_tray(app.handle(), background::TrayModel::of(&state, core.quick_access_offered(), sys_locale::get_locale().as_deref()));
            }
            // The quick search's shortcut, from the start: it works while the vault is locked too
            // (the window comes up with the unlock screen).
            quick_access::sync(app.handle(), state.quick_access.shortcut.as_deref());
            tauri::async_runtime::spawn(forward_events(app.handle().clone(), core.clone(), options.tray));
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                watch_focus(&window, core.clone());
                watch_close(&window, core.clone());
                watch_drops(&window, core);
                // Declared invisible so it never flashes white before the theme is applied. Started
                // for the browser while Lockra runs in the background, it stays in the tray;
                // started with `--palette`, the quick search asks the core for the palette (which
                // the vault, locked at a start, keeps shut: the unlock screen shows).
                let start = quick_access::start_reason(&std::env::args_os().collect::<Vec<_>>());
                if !(start == quick_access::Start::Browser && run_in_background) {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                if start == quick_access::Start::Palette {
                    quick_access::summon(app.handle());
                }
            }
            Ok(())
        })
}

/// The desktop entry point.
pub fn run() {
    // The browser started this binary as its native-messaging host: it relays, and nothing of the
    // app starts (stdout is the protocol's; the log goes to stderr alone).
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let host = browser::host::is_host_mode(&args);
    if host {
        logging::init_stderr();
    } else {
        logging::init();
    }
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
    if host {
        std::process::exit(browser::host::run_host());
    }
    let options = ShellOptions { plugin_updates: true, ..ShellOptions::default() };
    #[cfg_attr(not(windows), allow(unused_mut, reason = "only Windows adjusts the context"))]
    let mut context = tauri::generate_context!();
    // Windows: the update's installer runs silently when this copy came from the NSIS installer,
    // passively when it came from the MSI (updater::windows_install_mode).
    #[cfg(windows)]
    updater::set_windows_install_mode(&mut context.config_mut().plugins, updater::windows_install_mode(tauri::utils::platform::bundle_type()));
    let app = match build_app(tauri::Builder::default(), options).build(context) {
        Ok(app) => app,
        Err(error) => {
            tracing::error!(%error, "Lockra could not start");
            std::process::exit(1);
        }
    };
    app.run(|app, event| {
        // macOS: the Dock's icon brings back a window hidden in the background.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            background::show_main(app);
        }
        let _ = (app, event);
    });
}
