//! The app's side of the browser channel (plan §一, "生命周期"). While the integration is on
//! (`settings.json`'s `browser_integration`, readable while locked), [`lockra_browser::serve`]
//! runs on the socket from the start, answering `hello` and `unlock_request` even while locked;
//! the keys it needs come from the core once unlocked, and as the vault locks every session ends.
//! Turning it on writes the chosen browsers' manifests and listens; turning it off stops
//! listening (no new connection, every session ended, the socket removed) and removes them.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use lockra_core::{Core, CoreError, ErrorCode};
use tauri::State;
use tokio::sync::{Mutex, oneshot};

use super::manifest::{Browser, BrowserStatus, Manifests};
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

/// At start, with the integration on: the manifests written again (the program may have moved)
/// and the socket listened on.
pub async fn resume(core: Core, service: Arc<BrowserService>) {
    if !core.browser_enabled() {
        return;
    }
    let refreshed = tauri::async_runtime::spawn_blocking(|| Manifests::current().and_then(|m| m.refresh())).await;
    if !matches!(refreshed, Ok(Ok(()))) {
        tracing::warn!("the browsers' manifests could not be written again");
    }
    if let Err(error) = service.start(core).await {
        tracing::warn!(%error, "the browser channel could not listen");
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
