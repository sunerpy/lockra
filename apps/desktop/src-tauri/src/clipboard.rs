//! The system clipboard through `arboard`, owned by one worker thread: on Linux (X11 and Wayland)
//! the process that set the clipboard serves its content, so the handle must outlive every call.
//! Codes are written with the "do not keep" hints each platform offers: excluded from clipboard
//! history everywhere, and on Windows also from cloud sync and from clipboard monitors.

use std::sync::mpsc::{self, Sender, SyncSender};
use std::time::Duration;

use lockra_core::ports::{Clipboard, ClipboardImage, PortError};
use parking_lot::Mutex;
use zeroize::Zeroizing;

const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

type Reply<T> = SyncSender<Result<T, String>>;

enum Request {
    SetSecret(Zeroizing<String>, Reply<()>),
    Text(Reply<Option<Zeroizing<String>>>),
    Image(Reply<Option<ClipboardImage>>),
    ClearIf(Zeroizing<String>, Reply<bool>),
}

/// The clipboard behind a worker thread.
pub struct ArboardClipboard {
    requests: Mutex<Sender<Request>>,
}

impl ArboardClipboard {
    /// Start the worker; the clipboard itself is opened on first use.
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Request>();
        let spawned = std::thread::Builder::new().name("lockra-clipboard".into()).spawn(move || {
            let mut clipboard: Option<arboard::Clipboard> = None;
            for request in rx {
                if clipboard.is_none() {
                    clipboard = arboard::Clipboard::new().map_err(|e| tracing::warn!(error = %e, "clipboard unavailable")).ok();
                }
                serve(clipboard.as_mut(), request);
            }
        });
        if let Err(error) = spawned {
            tracing::error!(%error, "could not start the clipboard thread");
        }
        Self { requests: Mutex::new(tx) }
    }

    fn ask<T>(&self, make: impl FnOnce(Reply<T>) -> Request) -> Result<T, PortError> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.requests.lock().send(make(tx)).map_err(|_| PortError("clipboard thread gone".into()))?;
        rx.recv_timeout(REPLY_TIMEOUT).map_err(|_| PortError("clipboard did not answer".into()))?.map_err(PortError)
    }
}

fn unavailable<T>() -> Result<T, String> {
    Err("clipboard unavailable".to_owned())
}

fn serve(clipboard: Option<&mut arboard::Clipboard>, request: Request) {
    let Some(clipboard) = clipboard else {
        match request {
            Request::SetSecret(_, reply) => drop(reply.send(unavailable())),
            Request::Text(reply) => drop(reply.send(unavailable())),
            Request::Image(reply) => drop(reply.send(unavailable())),
            Request::ClearIf(_, reply) => drop(reply.send(unavailable())),
        }
        return;
    };
    match request {
        Request::SetSecret(text, reply) => {
            let _ = reply.send(set_secret(clipboard, &text).map_err(|e| e.to_string()));
        }
        Request::Text(reply) => {
            let _ = reply.send(match clipboard.get_text() {
                Ok(text) => Ok(Some(Zeroizing::new(text))),
                Err(arboard::Error::ContentNotAvailable) => Ok(None),
                Err(e) => Err(e.to_string()),
            });
        }
        Request::Image(reply) => {
            let _ = reply.send(match clipboard.get_image() {
                Ok(image) => Ok(Some(ClipboardImage {
                    width: u32::try_from(image.width).unwrap_or(0),
                    height: u32::try_from(image.height).unwrap_or(0),
                    rgba: image.bytes.into_owned(),
                })),
                Err(arboard::Error::ContentNotAvailable) => Ok(None),
                Err(e) => Err(e.to_string()),
            });
        }
        Request::ClearIf(expected, reply) => {
            let _ = reply.send(match clipboard.get_text() {
                Ok(current) if current == *expected => clipboard.clear().map(|()| true).map_err(|e| e.to_string()),
                Ok(_) | Err(arboard::Error::ContentNotAvailable) => Ok(false),
                Err(e) => Err(e.to_string()),
            });
        }
    }
}

#[cfg(target_os = "windows")]
fn set_secret(clipboard: &mut arboard::Clipboard, text: &str) -> Result<(), arboard::Error> {
    use arboard::SetExtWindows;
    clipboard.set().exclude_from_history().exclude_from_cloud().exclude_from_monitoring().text(text)
}

#[cfg(target_os = "macos")]
fn set_secret(clipboard: &mut arboard::Clipboard, text: &str) -> Result<(), arboard::Error> {
    use arboard::SetExtApple;
    clipboard.set().exclude_from_history().text(text)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn set_secret(clipboard: &mut arboard::Clipboard, text: &str) -> Result<(), arboard::Error> {
    use arboard::SetExtLinux;
    clipboard.set().exclude_from_history().text(text)
}

impl Clipboard for ArboardClipboard {
    fn set_secret_text(&self, text: &str) -> Result<(), PortError> {
        self.ask(|reply| Request::SetSecret(Zeroizing::new(text.to_owned()), reply))
    }

    fn text(&self) -> Result<Option<Zeroizing<String>>, PortError> {
        self.ask(Request::Text)
    }

    fn image(&self) -> Result<Option<ClipboardImage>, PortError> {
        self.ask(Request::Image)
    }

    fn clear_if(&self, expected: &str) -> Result<bool, PortError> {
        self.ask(|reply| Request::ClearIf(Zeroizing::new(expected.to_owned()), reply))
    }
}
