//! Fakes behind the ports, for this crate's tests and, through the `fakes` feature, for the
//! bridge's and the desktop shell's.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use lockra_sync::{MemoryRemote, ObjectMeta, PutCondition, RemoteFuture, RemoteStore, StorageConfig, SyncError};
use parking_lot::Mutex;
use tokio::time::Instant;
use zeroize::Zeroizing;

use crate::ports::{
    BiometricError, Biometrics, Clipboard, ClipboardImage, Clock, CodeSink, HubServe, KeychainStatus, LanClientConfig, LanEvent, LanEvents, LanFuture,
    LanJoining, LanKey, LanService, MemorySecretStore, PairOffer, PortError, Release, SecretStore, SyncTransport, UpdateFailure, UpdateFuture, UpdateProgress,
    Updater,
};
use crate::ui::{BiometricKind, CodesFrame, InstallMethod};
use uuid::Uuid;

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

/// Touch ID that answers what the test says, and remembers every reason it was shown.
#[derive(Debug)]
pub struct FakeBiometrics {
    kind: Mutex<Option<BiometricKind>>,
    answer: Mutex<Result<(), BiometricError>>,
    reasons: Mutex<Vec<String>>,
}

impl Default for FakeBiometrics {
    fn default() -> Self {
        Self { kind: Mutex::new(Some(BiometricKind::TouchId)), answer: Mutex::new(Ok(())), reasons: Mutex::new(Vec::new()) }
    }
}

impl FakeBiometrics {
    /// What the computer offers from now on.
    pub fn set_kind(&self, kind: Option<BiometricKind>) {
        *self.kind.lock() = kind;
    }

    /// How every check answers from now on.
    pub fn answer(&self, answer: Result<(), BiometricError>) {
        *self.answer.lock() = answer;
    }

    /// The reasons of the checks asked so far.
    pub fn reasons(&self) -> Vec<String> {
        self.reasons.lock().clone()
    }
}

impl Biometrics for FakeBiometrics {
    fn availability(&self) -> Option<BiometricKind> {
        *self.kind.lock()
    }

    fn verify(&self, reason: &str) -> Result<(), BiometricError> {
        self.reasons.lock().push(reason.to_owned());
        self.answer.lock().clone()
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

/// Sync storage in memory, shared by the cores of one test: a configuration names one
/// [`MemoryRemote`] by its address (S3: endpoint and bucket, with conditional writes; WebDAV: the
/// URL, without), and any secret but [`FakeTransport::SECRET`] is refused on every request. The
/// LAN is one hub ([`FakeLan`]): its copy of the space and its server, for the hub and its clients
/// alike.
#[derive(Default)]
pub struct FakeTransport {
    stores: Mutex<BTreeMap<String, Arc<MemoryRemote>>>,
    /// The LAN.
    pub hub: Arc<FakeLan>,
    /// How many times a storage was opened.
    pub opened: AtomicUsize,
}

impl std::fmt::Debug for FakeTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeTransport").finish_non_exhaustive()
    }
}

impl FakeTransport {
    /// The storage secret (S3 secret key, WebDAV password) the fake accepts.
    pub const SECRET: &'static str = "storage secret";

    /// The store `config` names, created empty on first use.
    pub fn store(&self, config: &StorageConfig) -> Arc<MemoryRemote> {
        let (address, conditional) = match config {
            StorageConfig::S3 { endpoint, bucket, .. } => (format!("s3:{endpoint}/{bucket}"), true),
            StorageConfig::Webdav { url, .. } => (format!("dav:{url}"), false),
        };
        Arc::clone(self.stores.lock().entry(address).or_insert_with(|| Arc::new(MemoryRemote::new(conditional))))
    }

    /// The hub's copy of the space, created empty on first use.
    pub fn lan(&self) -> Arc<MemoryRemote> {
        self.hub.copy()
    }

    /// From now on the hub answers its clients' requests with `failure` (away from it:
    /// `Network`), or as it should.
    pub fn set_lan_failure(&self, failure: Option<SyncError>) {
        *self.hub.failure.lock() = failure;
    }

    /// The requests the hub's clients made, reaching it or not.
    pub fn lan_attempts(&self) -> usize {
        self.hub.attempts.load(Ordering::SeqCst)
    }
}

impl SyncTransport for FakeTransport {
    fn open(&self, config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        let secret = match config {
            StorageConfig::S3 { secret_access_key, .. } => secret_access_key,
            StorageConfig::Webdav { password, .. } => password,
        };
        if secret.as_str() != Self::SECRET {
            return Ok(Arc::new(Failing(SyncError::Denied)));
        }
        Ok(self.store(config))
    }

    fn open_hub_store(&self, _space_id: Uuid) -> Result<Arc<dyn RemoteStore>, SyncError> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        Ok(self.lan())
    }

    fn open_lan_client(&self, config: &LanClientConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        let mut key = Zeroizing::new([0u8; 32]);
        if config.psk.len() != key.len() {
            return Err(SyncError::WrongCredentials);
        }
        key.copy_from_slice(&config.psk);
        Ok(Arc::new(LanClient { lan: Arc::clone(&self.hub), key }))
    }

    fn lan(&self) -> Option<&dyn LanService> {
        Some(&*self.hub)
    }
}

/// What the hub answers a pairing request: the welcome, or none.
type Welcome = Option<Zeroizing<Vec<u8>>>;

/// A hub's server in memory: what it serves, and where its events go; a pairing request waits
/// for its answer. The code both sides show is [`FakeLan::CODE`].
#[derive(Default)]
pub struct FakeLan {
    copy: Mutex<Option<Arc<MemoryRemote>>>,
    server: Mutex<Option<(HubServe, LanEvents)>>,
    waiting: Mutex<Option<tokio::sync::oneshot::Sender<Welcome>>>,
    failure: Mutex<Option<SyncError>>,
    attempts: AtomicUsize,
    /// A port another program holds: a server asked for it listens on the next one.
    pub taken_port: Mutex<Option<u16>>,
    /// How many times a server was started.
    pub started: AtomicUsize,
}

impl FakeLan {
    /// The check code of every pairing.
    pub const CODE: &'static str = "246813";

    fn copy(&self) -> Arc<MemoryRemote> {
        Arc::clone(self.copy.lock().get_or_insert_with(|| Arc::new(MemoryRemote::new(true))))
    }

    /// What the server serves now.
    pub fn served(&self) -> Option<HubServe> {
        self.server.lock().as_ref().map(|(config, _)| config.clone())
    }

    /// Tell the hub's core `event`, outside the server's lock (the core answers with updates).
    fn tell(&self, event: LanEvent) {
        let events = self.server.lock().as_ref().map(|(_, events)| Arc::clone(events));
        if let Some(events) = events {
            events(event);
        }
    }

    /// Which paired device `key` is, or how the hub turns it away.
    fn device(&self, key: &LanKey) -> Result<(Uuid, Option<String>), SyncError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        if let Some(failure) = self.failure.lock().clone() {
            return Err(failure);
        }
        let server = self.server.lock();
        let Some((config, _)) = server.as_ref() else { return Err(SyncError::Network("the hub is not serving".into())) };
        if let Some(peer) = config.peers.iter().find(|peer| *peer.key == **key) {
            return Ok((peer.peer_id, peer.tag.clone()));
        }
        if config.removed.iter().any(|removed| **removed == **key) {
            return Err(SyncError::WrongCredentials);
        }
        Err(SyncError::Network("the hub dropped the connection".into()))
    }

    /// A paired device wrote under `tag`: the server binds it (once) and says so.
    fn wrote(&self, peer_id: Uuid, bound: Option<String>, tag: &str) {
        if bound.as_deref() != Some(tag) {
            if let Some((config, _)) = self.server.lock().as_mut()
                && let Some(peer) = config.peers.iter_mut().find(|peer| peer.peer_id == peer_id)
            {
                peer.tag = Some(tag.to_owned());
            }
            self.tell(LanEvent::PeerTag { peer_id, tag: tag.to_owned() });
        }
        self.tell(LanEvent::PeerWrote { peer_id });
    }
}

impl LanService for FakeLan {
    fn serve(&self, config: HubServe, events: LanEvents) -> LanFuture<'_, u16> {
        Box::pin(async move {
            self.started.fetch_add(1, Ordering::SeqCst);
            let mut config = config;
            if *self.taken_port.lock() == Some(config.port) {
                config.port += 1;
            }
            let port = config.port;
            *self.server.lock() = Some((config, events));
            Ok(port)
        })
    }

    fn update(&self, config: HubServe) {
        if let Some((served, _)) = self.server.lock().as_mut() {
            let port = served.port;
            *served = HubServe { port, ..config };
        }
    }

    fn answer(&self, welcome: Option<Zeroizing<Vec<u8>>>) {
        let welcomed = welcome.is_some();
        if let Some(waiting) = self.waiting.lock().take() {
            let _ = waiting.send(welcome);
        }
        self.tell(if welcomed { LanEvent::PairWelcomed } else { LanEvent::PairEnded });
    }

    fn stop(&self) {
        *self.server.lock() = None;
    }

    fn addresses(&self) -> Vec<IpAddr> {
        vec![IpAddr::from([192, 168, 1, 20])]
    }

    fn join<'a>(&'a self, offer: &'a PairOffer, name: &'a str, platform: &'a str) -> LanFuture<'a, Box<dyn LanJoining>> {
        Box::pin(async move {
            // The offer's key opens one handshake, while it stands.
            let taken = {
                let mut server = self.server.lock();
                let Some((config, _)) = server.as_mut() else { return Err(SyncError::Network("the hub was not found".into())) };
                match &config.pairing {
                    Some((key, until)) if **key == *offer.key && Instant::now() < *until => {
                        config.pairing = None;
                        true
                    }
                    _ => false,
                }
            };
            if !taken {
                return Err(SyncError::Network("the hub was not found, or its offer was taken".into()));
            }
            let (answered, answer) = tokio::sync::oneshot::channel();
            *self.waiting.lock() = Some(answered);
            self.tell(LanEvent::PairRequest { name: name.to_owned(), platform: platform.to_owned(), code: Self::CODE.to_owned() });
            Ok(Box::new(FakeJoining(answer)) as Box<dyn LanJoining>)
        })
    }
}

struct FakeJoining(tokio::sync::oneshot::Receiver<Welcome>);

impl LanJoining for FakeJoining {
    fn code(&self) -> String {
        FakeLan::CODE.to_owned()
    }

    fn answer(self: Box<Self>) -> LanFuture<'static, Option<Zeroizing<Vec<u8>>>> {
        Box::pin(async move { self.0.await.map_err(|_| SyncError::Network("the hub left".into())) })
    }
}

/// A client's way to its hub: each request is checked against what the hub serves (a device it
/// no longer pairs with is told so), or fails as the test said.
struct LanClient {
    lan: Arc<FakeLan>,
    key: LanKey,
}

/// The tag of the object at `path`.
fn tag_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or_default().trim_end_matches(".lks")
}

impl RemoteStore for LanClient {
    fn conditional_puts(&self) -> bool {
        true
    }

    fn list<'a>(&'a self, dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>> {
        Box::pin(async move {
            self.lan.device(&self.key)?;
            self.lan.copy().list(dir).await
        })
    }

    fn get<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, Option<(Vec<u8>, Option<String>)>> {
        Box::pin(async move {
            self.lan.device(&self.key)?;
            self.lan.copy().get(path).await
        })
    }

    fn put<'a>(&'a self, path: &'a str, bytes: Vec<u8>, condition: PutCondition) -> RemoteFuture<'a, Option<String>> {
        Box::pin(async move {
            let (peer_id, bound) = self.lan.device(&self.key)?;
            let written = self.lan.copy().put(path, bytes, condition).await?;
            self.lan.wrote(peer_id, bound, tag_of(path));
            Ok(written)
        })
    }

    fn delete<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, ()> {
        Box::pin(async move {
            let (peer_id, bound) = self.lan.device(&self.key)?;
            self.lan.copy().delete(path).await?;
            self.lan.wrote(peer_id, bound, tag_of(path));
            Ok(())
        })
    }
}

/// A storage that answers every request with one error.
struct Failing(SyncError);

impl RemoteStore for Failing {
    fn conditional_puts(&self) -> bool {
        true
    }

    fn list<'a>(&'a self, _dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>> {
        Box::pin(async { Err(self.0.clone()) })
    }

    fn get<'a>(&'a self, _path: &'a str) -> RemoteFuture<'a, Option<(Vec<u8>, Option<String>)>> {
        Box::pin(async { Err(self.0.clone()) })
    }

    fn put<'a>(&'a self, _path: &'a str, _bytes: Vec<u8>, _condition: PutCondition) -> RemoteFuture<'a, Option<String>> {
        Box::pin(async { Err(self.0.clone()) })
    }

    fn delete<'a>(&'a self, _path: &'a str) -> RemoteFuture<'a, ()> {
        Box::pin(async { Err(self.0.clone()) })
    }
}
