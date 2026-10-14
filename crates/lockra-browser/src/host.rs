//! The host process. The browser starts `lockra-desktop` in host mode for the extension and talks
//! to it over stdio (native messaging, the length in the machine's byte order); the host passes
//! every frame on to the app's local socket (little-endian) and back. It answers by itself only
//! what needs no app (docs/formats.md §10): that Lockra is not running,
//! `{"op":"state","state":"not_running"}`, when a frame finds no app to go to and as the app's
//! connection ends; and the extension's asking to start it, `start_app`, which gets no answer. It
//! reads nothing it passes on: Noise is end to end between the extension and the app, so the host
//! and the socket are only a pipe.

use std::future::Future;
use std::io;

use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::frame::{FrameError, Order, read_frame, write_frame};
use crate::protocol::{AppPlain, AppState, ClientPlain, Envelope, Kind};

/// What the host needs of the machine: the app's socket, and starting the app.
pub trait HostPorts: Send + 'static {
    /// A connection to the app.
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// Connect to the running app; an error when it is not running (or its socket is not one the
    /// host trusts).
    fn connect(&self) -> impl Future<Output = io::Result<Self::Stream>> + Send;

    /// Start Lockra, its window up when `show`, else as the browser's (`--from-browser`).
    fn start_app(&self, show: bool) -> io::Result<()>;
}

/// The host's own answer when there is no app to pass a frame to.
pub fn not_running() -> Vec<u8> {
    // A plain message is plain data: serializing it cannot fail.
    #[allow(clippy::expect_used)]
    let plain = serde_json::to_vec(&AppPlain::State { state: AppState::NotRunning }).expect("a plain message serializes");
    Envelope::new(Kind::Hello, &plain).frame()
}

/// The extension's `start_app` (with its `show`), if `frame` is one.
fn start_request(frame: &[u8]) -> Option<bool> {
    let envelope: Envelope = serde_json::from_slice(frame).ok()?;
    if envelope.t != Kind::Hello {
        return None;
    }
    match serde_json::from_slice::<ClientPlain>(&envelope.bytes()?).ok()? {
        ClientPlain::StartApp { show } => Some(show),
        _ => None,
    }
}

/// The connection to the app: its writing half, and the task passing its frames to the browser.
struct App<S> {
    writer: WriteHalf<S>,
    pump: JoinHandle<()>,
}

impl<S> Drop for App<S> {
    fn drop(&mut self) {
        self.pump.abort();
    }
}

/// Pass frames between the browser (`input`, `output`) and the app until the browser closes its
/// end (`Ok`), or sends what is not a frame (`Err`: the browser sees the host go).
pub async fn relay<I, O, P>(mut input: I, output: O, ports: P) -> Result<(), FrameError>
where
    I: AsyncRead + Unpin,
    O: AsyncWrite + Unpin + Send + 'static,
    P: HostPorts,
{
    // One writer for the browser's end, for the app's frames and the host's own.
    let (to_browser, outbox) = mpsc::channel::<Vec<u8>>(16);
    let writer = tokio::spawn(write_all(output, outbox));
    let mut app: Option<App<P::Stream>> = None;
    let ended = loop {
        let frame = match read_frame(&mut input, Order::Native).await {
            Ok(Some(frame)) => frame,
            Ok(None) => break Ok(()),
            Err(error) => break Err(error),
        };
        if let Some(show) = start_request(&frame) {
            if let Err(error) = ports.start_app(show) {
                tracing::warn!(%error, "Lockra could not be started for the browser");
            }
            continue;
        }
        // The app's connection ended since the last frame (its pump said so): connect again.
        if app.as_ref().is_some_and(|app| app.pump.is_finished()) {
            app = None;
        }
        if app.is_none() {
            match ports.connect().await {
                Ok(stream) => {
                    let (reader, writer) = tokio::io::split(stream);
                    app = Some(App { writer, pump: tokio::spawn(pump(reader, to_browser.clone())) });
                }
                Err(_) => {
                    let _ = to_browser.send(not_running()).await;
                    continue;
                }
            }
        }
        if let Some(open) = app.as_mut()
            && write_frame(&mut open.writer, Order::Little, &frame).await.is_err()
        {
            // The app went as the frame was on its way: say so once (its pump no longer will).
            app = None;
            let _ = to_browser.send(not_running()).await;
        }
    };
    drop(app);
    drop(to_browser);
    let _ = writer.await;
    ended
}

/// The app's frames, passed to the browser; as the connection ends, that Lockra is not running.
async fn pump<S: AsyncRead>(mut reader: ReadHalf<S>, to_browser: mpsc::Sender<Vec<u8>>) {
    loop {
        match read_frame(&mut reader, Order::Little).await {
            Ok(Some(frame)) => {
                if to_browser.send(frame).await.is_err() {
                    return;
                }
            }
            Ok(None) | Err(_) => {
                let _ = to_browser.send(not_running()).await;
                return;
            }
        }
    }
}

async fn write_all<O: AsyncWrite + Unpin>(mut output: O, mut outbox: mpsc::Receiver<Vec<u8>>) {
    while let Some(frame) = outbox.recv().await {
        if write_frame(&mut output, Order::Native, &frame).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests;
