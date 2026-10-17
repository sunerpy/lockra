//! The app's side of the browser channel (plan §一, "生命周期"). While the integration is on
//! (`settings.json`'s `browser_integration`, readable while locked), [`lockra_browser::serve`]
//! runs on the socket from the start, answering `hello` and `unlock_request` even while locked;
//! the keys it needs come from the core once unlocked, and as the vault locks every session ends.
//! Turning it on writes the chosen browsers' manifests and listens; turning it off stops
//! listening (no new connection, every session ended, the socket removed) and removes them. Both
//! the start and the turning on first put this build's copy of the extension in its folder
//! ([`super::extension`]), so that an extension that connects finds it there.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use lockra_core::ports::Clipboard;
use lockra_core::{Core, CoreError, ErrorCode};
use tauri::State;
use tokio::sync::{Mutex, oneshot};

use super::extension::{self, ExtensionFolder, Synced};
use super::manifest::{Browser, BrowserStatus, Manifests, Places};
use super::socket;

/// The service on the socket, when it runs.
struct Running {
    stop: oneshot::Sender<()>,
    done: tauri::async_runtime::JoinHandle<()>,
}

/// The browser channel of this app: where it listens, and whether it does.
pub struct BrowserService {
    socket: PathBuf,
    running: Mutex<Option<Running>>,
}

impl BrowserService {
    pub fn new(socket: PathBuf) -> Arc<Self> {
        Arc::new(Self { socket, running: Mutex::new(None) })
    }

    /// Listen on the socket and serve the extensions; listening already is fine.
    pub async fn start(&self, core: Core) -> io::Result<()> {
        let mut running = self.running.lock().await;
        if running.as_ref().is_some_and(|r| !r.done.inner().is_finished()) {
            return Ok(());
        }
        let listener = socket::Socket::bind(&self.socket)?;
        let (stop, stopped) = oneshot::channel::<()>();
        let done = tauri::async_runtime::spawn(lockra_browser::serve(listener, core, async {
            let _ = stopped.await;
        }));
        *running = Some(Running { stop, done });
        Ok(())
    }

    /// Stop: no new connection, every session ended, then the socket goes (with the listener).
    pub async fn stop(&self) {
        let running = self.running.lock().await.take();
        if let Some(running) = running {
            let _ = running.stop.send(());
            let _ = running.done.await;
        }
    }

    /// Whether it listens.
    pub async fn listening(&self) -> bool {
        self.running.lock().await.as_ref().is_some_and(|r| !r.done.inner().is_finished())
    }
}

/// At start, with the integration on: the manifests written again (the program may have moved),
/// the extension's folder brought to this build's copy, and the socket listened on.
pub async fn resume(core: Core, service: Arc<BrowserService>) {
    if !core.browser_enabled() {
        return;
    }
    let refreshed = tauri::async_runtime::spawn_blocking(|| Manifests::current().and_then(|m| m.refresh())).await;
    if !matches!(refreshed, Ok(Ok(()))) {
        tracing::warn!("the browsers' manifests could not be written again");
    }
    sync_extension().await;
    if let Err(error) = service.start(core).await {
        tracing::warn!(%error, "the browser channel could not listen");
    }
}

/// The folder of this user's machine.
fn extension_folder() -> io::Result<PathBuf> {
    let places = Places::current(cfg!(debug_assertions)).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home folder"))?;
    Ok(extension::folder(&places))
}

/// The extension's folder made to hold this build's copy, off the async threads. A failure is only
/// logged: the folder keeps what it had, the integration works with any copy of the extension, and
/// the next start tries again.
async fn sync_extension() {
    let Some(bundle) = extension::carried() else { return };
    match tauri::async_runtime::spawn_blocking(|| extension::sync(&extension_folder()?, bundle)).await {
        Ok(Ok(Synced::Written)) => tracing::info!(version = bundle.version(), "the extension's folder holds this build's copy"),
        Ok(Ok(Synced::Unchanged)) => {}
        Ok(Err(error)) => tracing::warn!(%error, "the extension's folder could not be written"),
        Err(error) => tracing::warn!(%error, "the extension's folder could not be written"),
    }
}

fn offered(core: &Core) -> Result<(), CoreError> {
    if core.browser_offered() { Ok(()) } else { Err(ErrorCode::BrowserUnavailable.into()) }
}

async fn on_blocking<T: Send + 'static>(work: impl FnOnce() -> io::Result<T> + Send + 'static) -> Result<T, CoreError> {
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            tracing::warn!(%error, "the browsers' manifests");
            Err(ErrorCode::BrowserManifestFailed.into())
        }
        Err(_) => Err(ErrorCode::Internal.into()),
    }
}

/// Turn the integration on for `browsers` (their ids; others are left out): their manifests
/// written, the socket listened on, then the flag set. Every browser's status.
#[tauri::command]
pub async fn browser_enable(core: State<'_, Core>, service: State<'_, Arc<BrowserService>>, browsers: Vec<String>) -> Result<Vec<BrowserStatus>, CoreError> {
    offered(&core)?;
    let chosen: Vec<Browser> = browsers.iter().filter_map(|id| Browser::from_id(id)).collect();
    let statuses = on_blocking(move || {
        let manifests = Manifests::current()?;
        for browser in chosen {
            manifests.write(browser)?;
        }
        Ok(manifests.statuses())
    })
    .await?;
    sync_extension().await;
    service.start(core.inner().clone()).await.map_err(|error| {
        tracing::warn!(%error, "the browser channel could not listen");
        CoreError::from(ErrorCode::BrowserUnavailable)
    })?;
    core.browser_set_enabled(true)?;
    Ok(statuses)
}

/// Turn the integration off: no longer listening, every manifest removed, the flag cleared.
#[tauri::command]
pub async fn browser_disable(core: State<'_, Core>, service: State<'_, Arc<BrowserService>>) -> Result<Vec<BrowserStatus>, CoreError> {
    offered(&core)?;
    service.stop().await;
    let statuses = on_blocking(|| {
        let manifests = Manifests::current()?;
        for browser in Browser::ALL {
            manifests.remove(browser)?;
        }
        Ok(manifests.statuses())
    })
    .await?;
    core.browser_set_enabled(false)?;
    Ok(statuses)
}

/// Each browser's status: its profiles here, Lockra's manifest in place.
#[tauri::command]
pub async fn browser_status(core: State<'_, Core>) -> Result<Vec<BrowserStatus>, CoreError> {
    offered(&core)?;
    on_blocking(|| Ok(Manifests::current()?.statuses())).await
}

/// The clipboard the folder's path is copied to (the core's, `ShellOptions::clipboard`).
pub struct PathClipboard(pub Arc<dyn Clipboard>);

/// The extension's folder as Settings › 浏览器 shows it: where it is, the version this build
/// carries, whether the folder holds it. None when this build carries no extension (one made
/// before the extension was built).
#[tauri::command]
pub async fn browser_extension(core: State<'_, Core>) -> Result<Option<ExtensionFolder>, CoreError> {
    offered(&core)?;
    let Some(bundle) = extension::carried() else { return Ok(None) };
    on_blocking(|| Ok(Some(extension::status(&extension_folder()?, bundle)))).await
}

/// The folder's path on the clipboard, for the browser's 「加载已解压的扩展程序」 dialog.
#[tauri::command]
pub async fn browser_extension_copy(core: State<'_, Core>, clipboard: State<'_, PathClipboard>) -> Result<(), CoreError> {
    offered(&core)?;
    let folder = on_blocking(extension_folder).await?;
    let clipboard = clipboard.0.clone();
    match tauri::async_runtime::spawn_blocking(move || clipboard.set_secret_text(&folder.display().to_string())).await {
        Ok(Ok(())) => Ok(()),
        _ => Err(ErrorCode::ClipboardFailed.into()),
    }
}

/// The folder in the file manager (Explorer and Finder with it selected). Whether one started.
#[tauri::command]
pub async fn browser_extension_reveal(core: State<'_, Core>) -> Result<bool, CoreError> {
    offered(&core)?;
    on_blocking(|| Ok(extension::reveal(&extension_folder()?))).await
}
