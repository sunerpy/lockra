//! Fakes behind the ports, for this crate's tests and, through the `fakes` feature, for the
//! bridge's and the desktop shell's.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use parking_lot::Mutex;
use tokio::time::Instant;
use zeroize::Zeroizing;

use crate::ports::{
    Clipboard, ClipboardImage, Clock, CodeSink, KeychainStatus, MemorySecretStore, PortError, Release, SecretStore, UpdateFailure, UpdateFuture,
    UpdateProgress, Updater,
};
use crate::ui::{CodesFrame, InstallMethod};

/// Wall-clock time that moves with tokio's clock, so `tokio::time::advance` under
/// `start_paused` moves both the timers and the codes.
#[derive(Debug)]
pub struct FakeClock {
    base_ms: u64,
    start: Instant,
}

impl FakeClock {
    /// A clock that reads `base_ms` now.
    pub fn new(base_ms: u64) -> Self {
        Self { base_ms, start: Instant::now() }
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.base_ms + u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// An in-memory keychain that can be switched off or made to fail.
#[derive(Debug, Default)]
pub struct FakeKeychain {
    inner: MemorySecretStore,
    /// Report the keychain as missing.
    pub unavailable: AtomicBool,
    /// Make `set` fail.
    pub fail_set: AtomicBool,
    /// Make `get` fail.
    pub fail_get: AtomicBool,
}

impl SecretStore for FakeKeychain {
    fn status(&self) -> KeychainStatus {
        if self.unavailable.load(Ordering::SeqCst) { KeychainStatus::Unavailable } else { KeychainStatus::Available }
    }

    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, PortError> {
        if self.fail_get.load(Ordering::SeqCst) {
            return Err(PortError("get failed".into()));
        }
        self.inner.get(account)
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), PortError> {
        if self.fail_set.load(Ordering::SeqCst) {
            return Err(PortError("set failed".into()));
        }
        self.inner.set(account, secret)
    }

    fn delete(&self, account: &str) -> Result<(), PortError> {
        self.inner.delete(account)
    }
}

/// A clipboard in memory.
#[derive(Debug, Default)]
pub struct FakeClipboard {
    text: Mutex<Option<String>>,
    image: Mutex<Option<ClipboardImage>>,
    /// Make every call fail.
    pub fail: AtomicBool,
    /// How many times a code was written with the secret flags.
    pub secret_writes: AtomicUsize,
}

impl FakeClipboard {
    /// Put text on the clipboard, as another application would.
    pub fn put_text(&self, text: &str) {
        *self.text.lock() = Some(text.to_owned());
        *self.image.lock() = None;
    }

    /// Put an image on the clipboard, as a screenshot tool would.
    pub fn put_image(&self, image: ClipboardImage) {
        *self.image.lock() = Some(image);
        *self.text.lock() = None;
    }

    /// The clipboard's text.
    pub fn current(&self) -> Option<String> {
        self.text.lock().clone()
    }

    fn check(&self) -> Result<(), PortError> {
        if self.fail.load(Ordering::SeqCst) { Err(PortError("clipboard failed".into())) } else { Ok(()) }
    }
}

impl Clipboard for FakeClipboard {
    fn set_secret_text(&self, text: &str) -> Result<(), PortError> {
        self.check()?;
        self.secret_writes.fetch_add(1, Ordering::SeqCst);
        self.put_text(text);
        Ok(())
    }

    fn text(&self) -> Result<Option<Zeroizing<String>>, PortError> {
        self.check()?;
        Ok(self.text.lock().clone().map(Zeroizing::new))
    }

    fn image(&self) -> Result<Option<ClipboardImage>, PortError> {
        self.check()?;
        Ok(self.image.lock().clone())
    }

    fn clear_if(&self, expected: &str) -> Result<bool, PortError> {
        self.check()?;
        let mut text = self.text.lock();
        if text.as_deref() == Some(expected) {
            *text = None;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Records every code frame; can be closed to simulate a webview that went away.
#[derive(Debug, Default)]
pub struct RecordingSink {
    frames: Mutex<Vec<CodesFrame>>,
    /// Refuse further frames.
    pub closed: AtomicBool,
}

impl RecordingSink {
    /// Every frame received so far.
    pub fn frames(&self) -> Vec<CodesFrame> {
        self.frames.lock().clone()
    }
}

impl CodeSink for RecordingSink {
    fn send(&self, frame: &CodesFrame) -> Result<(), PortError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(PortError("closed".into()));
        }
        self.frames.lock().push(frame.clone());
        Ok(())
    }
}

/// An update source the test scripts: what `check`, `download` and `install` answer, the
/// progress the download reports, and every call in order.
#[derive(Debug)]
pub struct FakeUpdater {
    /// How an update installs; `None`: this copy cannot update itself.
    pub method: Mutex<Option<InstallMethod>>,
    /// What `check` answers.
    pub check: Mutex<Result<Option<Release>, UpdateFailure>>,
    /// The progress `download` reports, in order.
    pub progress: Mutex<Vec<(u64, Option<u64>)>>,
    /// What `download` answers.
    pub download: Mutex<Result<(), UpdateFailure>>,
    /// What `install` answers.
    pub install: Mutex<Result<(), UpdateFailure>>,
    /// Every call so far: `check`, `download`, `install`.
    pub calls: Mutex<Vec<&'static str>>,
    /// Make `check` wait for [`Self::resume`], to watch a run in flight.
    pub hold: AtomicBool,
    resume: tokio::sync::Notify,
}

impl Default for FakeUpdater {
    fn default() -> Self {
        Self {
            method: Mutex::new(None),
            check: Mutex::new(Ok(None)),
            progress: Mutex::new(Vec::new()),
            download: Mutex::new(Ok(())),
            install: Mutex::new(Ok(())),
            calls: Mutex::new(Vec::new()),
            hold: AtomicBool::new(false),
            resume: tokio::sync::Notify::new(),
        }
    }
}

impl FakeUpdater {
    /// A copy installed by `method`, with nothing newer out.
    pub fn installed(method: InstallMethod) -> Self {
        Self { method: Mutex::new(Some(method)), ..Self::default() }
    }

    /// A release with notes and a date.
    pub fn release(version: &str) -> Release {
        Release { version: version.to_owned(), notes: Some(format!("## {version}\n\n- What changed")), date: Some("2026-10-02T08:00:00Z".to_owned()) }
    }

    /// Let a held `check` answer.
    pub fn resume(&self) {
        self.resume.notify_one();
    }

    /// The calls so far.
    pub fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().clone()
    }
}

impl Updater for FakeUpdater {
    fn method(&self) -> Option<InstallMethod> {
        *self.method.lock()
    }

    fn check(&self) -> UpdateFuture<'_, Option<Release>> {
        Box::pin(async move {
            self.calls.lock().push("check");
            if self.hold.load(Ordering::SeqCst) {
                self.resume.notified().await;
            }
            self.check.lock().clone()
        })
    }

    fn download(&self, mut progress: UpdateProgress) -> UpdateFuture<'_, ()> {
        Box::pin(async move {
            self.calls.lock().push("download");
            let steps = self.progress.lock().clone();
            for (received, total) in steps {
                progress(received, total);
                tokio::task::yield_now().await;
            }
            *self.download.lock()
        })
    }

    fn install(&self) -> UpdateFuture<'_, ()> {
        Box::pin(async move {
            self.calls.lock().push("install");
            *self.install.lock()
        })
    }
}
