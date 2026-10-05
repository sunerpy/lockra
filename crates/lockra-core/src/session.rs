//! The core: one session state machine (no vault → locked → unlocked) and every command.
//!
//! State sits behind one `parking_lot` mutex that is held only for in-memory work and the small
//! vault write; Argon2, QR decoding and SQLite run on the blocking pool with the lock released.
//! One scheduler task owns every timer (code windows, auto-lock, clipboard clearing, the backup
//! debounce, export expiry): it sleeps until the earliest deadline and is woken on every change.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use data_encoding::BASE64;
use lockra_otp::{OtpKind, base32, hotp, totp_window, uri};
use lockra_sync::{
    Invite, Outcome as SyncOutcome, RemoteStore, Space, SpaceKeys, StorageConfig, SyncError, SyncKey, SyncState, find_space, open_keyring, open_space,
    remove_device, seal_keyring, step, step_with,
};
use lockra_transfer::{Item, Origin, detect, microsoft, qr, text};
use lockra_vault::{DeviceCheck, DeviceKey, DeviceSlot, FileKind, KdfCost, Opened, Sealed, read_header, write_atomic};
use parking_lot::{Mutex, MutexGuard};
use tokio::sync::{Notify, broadcast};
use tokio::time::Instant;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::backup::{auto_file_name, pre_restore_file_name, prune};
use crate::entry::{Entry, EntryDraft, EntryPatch, VaultData, clean_mark, clean_name, random_device};
use crate::error::{CoreError, CoreResult, ErrorCode};
use crate::export::{self, EXPORT_IDLE, ExportSession};
use crate::import::{AwaitingBackup, Choice, ImportSession, Outcome};
use crate::ports::{BiometricError, Biometrics, Clipboard, Clock, CodeSink, KeychainStatus, SecretStore, SyncTransport, Updater};
use crate::settings::{Settings, SettingsStore};
use crate::sync::{
    Brought, LanLocal, SYNC_DEBOUNCE, SYNC_FOCUS_MIN, SYNC_INTERVAL, SYNC_INTERVAL_FOREGROUND, SYNC_INTERVAL_LAN_FOREGROUND, Store, StoreKey, SyncLocal,
    Working, config_error, device_name, merge_entries, storage_view, sync_error,
};
use crate::ui::{
    BackupFailure, BackupView, BiometricKind, BiometricView, CodeView, CodesFrame, DeviceUnlockView, ExportPage, ExportStarted, ExportTarget, ImportSource,
    InstallMethod, JoinSource, LanPeerView, LanView, LockView, Notice, Phase, Platform, RestoreView, Revealed, SyncCreated, SyncInvite, SyncSpaceView,
    SyncStatus, SyncView, TransportKind, TransportView, UiEvent, UiState, UpdateStatus, UpdateView,
};
use crate::update::{Pending, ProgressGate, UpdateRun, UpdateState, clear_marker, failure_code, read_marker, write_marker};

/// The vault's file name in the data directory.
pub const VAULT_FILE: &str = "vault.lockra";
/// This installation's id, beside the settings.
const INSTALL_ID_FILE: &str = "install-id";

mod lan;

pub use lan::{LAN_OFFER_TIME, LAN_PORT, MAX_LAN_PEERS};
/// The shortest master password accepted (characters).
pub const MIN_PASSWORD_CHARS: usize = 8;
/// The largest file an import or a restore reads.
pub const MAX_IMPORT_BYTES: u64 = 64 * 1024 * 1024;
/// Delay between the last change and the automatic backup it causes.
pub const AUTO_BACKUP_DEBOUNCE: Duration = Duration::from_secs(3);
/// Wrong passwords allowed in a row before each further attempt has to wait.
pub const FREE_ATTEMPTS: u32 = 3;
const MAX_RETRY_DELAY_MS: u64 = 30_000;

/// Where the core keeps its files and how it derives keys.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    /// `vault.lockra`, its `.prev` copy and the pre-restore backups.
    pub data_dir: PathBuf,
    /// `settings.json`.
    pub config_dir: PathBuf,
    /// Shown in Settings › About.
    pub app_version: String,
    /// Argon2id cost for new password slots.
    pub kdf: KdfCost,
    /// The host.
    pub platform: Platform,
}

/// The platform services.
#[derive(Clone)]
pub struct Ports {
    /// The OS keychain.
    pub secrets: Arc<dyn SecretStore>,
    /// The clipboard.
    pub clipboard: Arc<dyn Clipboard>,
    /// Wall-clock time.
    pub clock: Arc<dyn Clock>,
    /// The in-app update.
    pub updater: Arc<dyn Updater>,
    /// The storage of a sync space.
    pub sync: Arc<dyn SyncTransport>,
    /// Touch ID or Windows Hello, before "remember on this device" unlocks.
    pub biometrics: Arc<dyn Biometrics>,
}

/// How a backup is restored into an unlocked vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreMode {
    /// Its entries go into the import preview, to be merged entry by entry.
    Merge,
    /// The vault's entries are replaced by the backup's (after a pre-restore backup).
    Replace,
}

/// A handle on the core; clones share it.
#[derive(Clone)]
pub struct Core {
    shared: Arc<Shared>,
}

struct Shared {
    config: CoreConfig,
    ports: Ports,
    settings_store: SettingsStore,
    /// This installation (`install-id` beside the settings).
    install_id: Uuid,
    state: Mutex<State>,
    events: broadcast::Sender<UiEvent>,
    wake: Notify,
}

struct State {
    phase: PhaseState,
    settings: Settings,
    /// From the vault file's header, readable while locked.
    vault_id: Option<Uuid>,
    device_slot: bool,
    /// What the device slot asks for first, as the header said at start or the keys at the last
    /// lock (while unlocked, the keys say).
    device_check: Option<DeviceCheck>,
    /// What the platform offers, as last asked: at start and at every lock.
    biometric: Option<BiometricKind>,
    failed_attempts: u32,
    retry_at_ms: Option<u64>,
    last_backup_ms: Option<u64>,
    last_auto_file: Option<String>,
    last_auto_error: Option<BackupFailure>,
    codes: Option<Arc<dyn CodeSink>>,
    /// The earliest `valid_until_ms` of the last frame pushed: once it has passed, a frame is due.
    codes_until: Option<u64>,
    clipboard: Option<(Zeroizing<String>, Instant)>,
    auto_backup_at: Option<Instant>,
    restore: Option<Restore>,
    last_activity: Instant,
    update: UpdateState,
    sync: SyncRuntime,
    /// The app is in front (the desktop's window has the focus); a phone app is, while it runs.
    foreground: bool,
    /// The LAN hub's server and pairing.
    lan: lan::LanRuntime,
}

/// The sync as it runs; the vault keeps the space itself.
#[derive(Default)]
struct SyncRuntime {
    /// A run is in progress; it takes the storages that became due meanwhile before it ends.
    busy: bool,
    lan: StoreRuntime,
    cloud: StoreRuntime,
}

impl SyncRuntime {
    fn store(&self, store: Store) -> &StoreRuntime {
        match store {
            Store::Lan => &self.lan,
            Store::Cloud => &self.cloud,
        }
    }

    fn store_mut(&mut self, store: Store) -> &mut StoreRuntime {
        match store {
            Store::Lan => &mut self.lan,
            Store::Cloud => &mut self.cloud,
        }
    }

    /// Everything forgotten (the vault locked, sync turned off); a run in progress finds the
    /// vault changed and keeps nothing.
    fn reset(&mut self) {
        *self = Self { busy: self.busy, ..Self::default() };
    }
}

/// The runs on one storage of the space.
#[derive(Default)]
struct StoreRuntime {
    /// When its next run is due.
    at: Option<Instant>,
    /// Due: the next run takes it.
    queued: bool,
    /// No run on its own (a newer format there, a hub that no longer knows this device) until
    /// the user asks for one.
    held: bool,
    /// When its last run ended.
    last_run: Option<Instant>,
    /// What its last run did.
    status: SyncStatus,
    /// Its last run did not reach the hub: away from it, which is no failure.
    offline: bool,
    /// Its last run's refused rollbacks and unreadable snapshots (device tags).
    rolled_back: Vec<String>,
    unreadable: Vec<String>,
    /// The storage, opened once per space and settings.
    remote: Option<(Uuid, StoreKey, Arc<dyn RemoteStore>)>,
}

/// What one run on one storage needs, taken from the vault when it starts.
struct SyncJob {
    space_id: Uuid,
    remote: Option<Arc<dyn RemoteStore>>,
    key: StoreKey,
    keys: SpaceKeys,
    device: u64,
    device_name: String,
    keyring: Vec<u8>,
    seq_floor: u64,
    state: SyncState,
    working: Working,
}

enum PhaseState {
    NoVault,
    Locked,
    Unlocked(Box<Session>),
}

struct Session {
    sealed: Sealed,
    data: VaultData,
    import: Option<ImportSession>,
    exports: Vec<ExportSession>,
}

struct Restore {
    name: String,
    bytes: Zeroizing<Vec<u8>>,
    view: RestoreView,
}

impl Core {
    /// Open the core on `config` and start its scheduler; call from inside a tokio runtime.
    pub fn start(config: CoreConfig, ports: Ports) -> Self {
        let settings_store = SettingsStore::new(&config.config_dir);
        let settings = settings_store.load();
        let install_id = install_id(&config.config_dir);
        let vault_path = config.data_dir.join(VAULT_FILE);
        let (phase, vault_id, device_slot, device_check) = match fs::read(&vault_path) {
            Ok(bytes) => match read_header(&bytes) {
                Ok(info) => (PhaseState::Locked, Some(info.vault_id), info.has_device_slot, info.device_check),
                // A damaged header still means "there is a vault": unlocking reports the damage.
                Err(_) => (PhaseState::Locked, None, false, None),
            },
            Err(_) => (PhaseState::NoVault, None, false, None),
        };
        let update = UpdateState::new(settings.auto_update && ports.updater.method().is_some_and(InstallMethod::installs));
        let state = State {
            phase,
            settings,
            vault_id,
            device_slot,
            device_check,
            biometric: None,
            failed_attempts: 0,
            retry_at_ms: None,
            last_backup_ms: None,
            last_auto_file: None,
            last_auto_error: None,
            codes: None,
            codes_until: None,
            clipboard: None,
            auto_backup_at: None,
            restore: None,
            last_activity: Instant::now(),
            update,
            sync: SyncRuntime::default(),
            foreground: true,
            lan: lan::LanRuntime::default(),
        };
        let (events, _) = broadcast::channel(64);
        let shared = Arc::new(Shared { config, ports, settings_store, install_id, state: Mutex::new(state), events, wake: Notify::new() });
        tokio::spawn(scheduler(Arc::clone(&shared)));
        let core = Self { shared };
        core.refresh_biometrics();
        core
    }

    /// Ask the platform what it offers, off the runtime: at start and at every lock, so a finger
    /// enrolled meanwhile shows up.
    fn refresh_biometrics(&self) {
        let core = self.clone();
        tokio::spawn(async move {
            let biometrics = Arc::clone(&core.shared.ports.biometrics);
            let Ok(kind) = tokio::task::spawn_blocking(move || biometrics.availability()).await else { return };
            let changed = {
                let mut st = core.lock();
                std::mem::replace(&mut st.biometric, kind) != kind
            };
            if changed {
                core.changed();
            }
        });
    }

    /// Every state change and notice, in order.
    pub fn subscribe(&self) -> broadcast::Receiver<UiEvent> {
        self.shared.events.subscribe()
    }

    /// The state the webview renders.
    pub fn state(&self) -> UiState {
        let st = self.lock();
        self.view(&st)
    }

    // ---- vault lifecycle ------------------------------------------------------------------

    /// Create the vault with `password` and open it.
    pub async fn create_vault(&self, password: Zeroizing<String>) -> CoreResult<()> {
        check_password(&password)?;
        if !matches!(self.lock().phase, PhaseState::NoVault) {
            return Err(ErrorCode::VaultExists.into());
        }
        let (cost, now, data_dir) = (self.shared.config.kdf, self.now_ms(), self.shared.config.data_dir.clone());
        let sealed = blocking(move || {
            let sealed = Sealed::create(password.as_bytes(), cost, now)?;
            fs::create_dir_all(&data_dir)?;
            restrict_dir(&data_dir);
            Ok(sealed)
        })
        .await?;
        {
            let mut st = self.lock();
            if !matches!(st.phase, PhaseState::NoVault) {
                return Err(ErrorCode::VaultExists.into());
            }
            let session = Session { sealed, data: VaultData::new(), import: None, exports: Vec::new() };
            self.write_vault(&session)?;
            st.vault_id = Some(session.sealed.vault_id());
            st.device_slot = false;
            st.phase = PhaseState::Unlocked(Box::new(session));
            st.last_activity = Instant::now();
        }
        self.changed();
        Ok(())
    }

    /// Unlock with the master password. Three wrong passwords are free; after that each attempt
    /// waits twice as long as the one before, up to 30 seconds.
    pub async fn unlock(&self, password: Zeroizing<String>) -> CoreResult<()> {
        let now = self.now_ms();
        {
            let st = self.lock();
            match st.phase {
                PhaseState::NoVault => return Err(ErrorCode::NoVault.into()),
                PhaseState::Unlocked(_) => return Ok(()),
                PhaseState::Locked => {}
            }
            if let Some(at) = st.retry_at_ms
                && now < at
            {
                return Err(CoreError { code: ErrorCode::RateLimited, retry_at_ms: Some(at) });
            }
        }
        let path = self.vault_path();
        let opened = blocking(move || Ok(Sealed::open_with_password(&read_limited(&path)?, password.as_bytes())?)).await;
        match opened {
            Ok(opened) => self.enter(opened),
            Err(error) if error.code == ErrorCode::WrongPassword => {
                let retry_at = {
                    let mut st = self.lock();
                    st.failed_attempts += 1;
                    let delay = retry_delay_ms(st.failed_attempts);
                    st.retry_at_ms = (delay > 0).then(|| self.now_ms() + delay);
                    st.retry_at_ms
                };
                self.changed();
                Err(CoreError { code: ErrorCode::WrongPassword, retry_at_ms: retry_at })
            }
            Err(error) => Err(error),
        }
    }

    /// Unlock with the key "remember on this device" left in the keychain, after Touch ID or
    /// Windows Hello when the vault asks for it (`reason` is the webview's words for the prompt).
    pub async fn unlock_with_device(&self, reason: Option<String>) -> CoreResult<()> {
        let vault_id = {
            let st = self.lock();
            match st.phase {
                PhaseState::NoVault => return Err(ErrorCode::NoVault.into()),
                PhaseState::Unlocked(_) => return Ok(()),
                PhaseState::Locked => {}
            }
            if !st.device_slot {
                return Err(ErrorCode::DeviceUnlockOff.into());
            }
            st.vault_id.ok_or(ErrorCode::VaultCorrupted)?
        };
        let path = self.vault_path();
        let bytes = blocking(move || read_limited(&path)).await?;
        // The check the file asks for comes before the keychain is read. A file that no longer asks
        // for it does not open: its header is authenticated with the payload.
        match read_header(&bytes)?.device_check {
            None => {}
            Some(DeviceCheck::Biometric) => self.check_user(reason).await?,
            Some(DeviceCheck::Other(_)) => return Err(ErrorCode::BiometricUnavailable.into()),
        }
        let key = self.device_key(vault_id)?;
        let opened = blocking(move || Ok(Sealed::open_with_device_key(&bytes, &key)?)).await?;
        self.enter(opened)
    }

    /// Touch ID or Windows Hello, with `reason` in the system's prompt (Lockra's own words when the
    /// webview gave none).
    async fn check_user(&self, reason: Option<String>) -> CoreResult<()> {
        let reason = reason.map(|r| clean_name(&r)).filter(|r| !r.is_empty()).unwrap_or_else(|| "unlock Lockra".to_owned());
        let biometrics = Arc::clone(&self.shared.ports.biometrics);
        let answer = tokio::task::spawn_blocking(move || biometrics.verify(&reason)).await.map_err(|_| CoreError::from(ErrorCode::Internal))?;
        answer.map_err(|error| {
            CoreError::from(match error {
                BiometricError::Cancelled => ErrorCode::BiometricCancelled,
                BiometricError::Unavailable => ErrorCode::BiometricUnavailable,
                BiometricError::Failed(why) => {
                    tracing::warn!("the biometric check failed: {why}");
                    ErrorCode::BiometricFailed
                }
            })
        })
    }

    /// Make "remember on this device" ask for Touch ID or Windows Hello before it unlocks; the user
    /// passes one check now, so that it is known to work. With "remember on this device" off, it is
    /// turned on with it.
    pub async fn enable_device_biometric(&self, reason: Option<String>) -> CoreResult<()> {
        let remembered = unlocked(&self.lock())?.sealed.has_device_slot();
        // The key needs somewhere to go before a fingerprint is asked for.
        if !remembered && self.shared.ports.secrets.status() == KeychainStatus::Unavailable {
            return Err(ErrorCode::KeychainUnavailable.into());
        }
        let biometrics = Arc::clone(&self.shared.ports.biometrics);
        let kind = tokio::task::spawn_blocking(move || biometrics.availability()).await.map_err(|_| CoreError::from(ErrorCode::Internal))?;
        self.lock().biometric = kind;
        if kind.is_none() {
            self.changed();
            return Err(ErrorCode::BiometricUnavailable.into());
        }
        self.check_user(reason).await?;
        self.turn_on_device_check()
    }

    /// The device slot asking for the check, the slot made first when there is none: one write, so
    /// the remembered key never exists without its check.
    fn turn_on_device_check(&self) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let mut next = session.sealed.clone();
            let key = if next.has_device_slot() { None } else { Some(next.enable_device()?) };
            next.set_device_check(Some(DeviceCheck::Biometric))?;
            let account = next.vault_id().to_string();
            if let Some(key) = &key {
                self.shared.ports.secrets.set(&account, &key.to_text()).map_err(|_| ErrorCode::KeychainFailed)?;
            }
            let previous = std::mem::replace(&mut session.sealed, next);
            if let Err(error) = self.write_vault(session) {
                session.sealed = previous;
                if key.is_some() {
                    let _ = self.shared.ports.secrets.delete(&account);
                }
                return Err(error);
            }
            st.device_slot = true;
        }
        self.changed();
        Ok(())
    }

    /// Stop asking for Touch ID or Windows Hello, after the master password was entered again.
    pub async fn disable_device_biometric(&self, password: Zeroizing<String>) -> CoreResult<()> {
        let sealed = unlocked(&self.lock())?.sealed.clone();
        blocking(move || Ok(sealed.verify_password(password.as_bytes())?)).await?;
        self.set_device_check(None)
    }

    fn set_device_check(&self, check: Option<DeviceCheck>) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let mut next = session.sealed.clone();
            if next.has_device_slot() {
                next.set_device_check(check)?;
            }
            let previous = std::mem::replace(&mut session.sealed, next);
            if let Err(error) = self.write_vault(session) {
                session.sealed = previous;
                return Err(error);
            }
        }
        self.changed();
        Ok(())
    }

    fn enter(&self, opened: Opened) -> CoreResult<()> {
        if opened.kind != FileKind::Vault {
            return Err(ErrorCode::VaultCorrupted.into());
        }
        let data = VaultData::open(&opened.payload)?;
        {
            let mut st = self.lock();
            st.vault_id = Some(opened.sealed.vault_id());
            st.device_slot = opened.sealed.has_device_slot();
            st.failed_attempts = 0;
            st.retry_at_ms = None;
            st.last_activity = Instant::now();
            let mut session = Session { sealed: opened.sealed, data, import: None, exports: Vec::new() };
            // A LAN role taken on another installation is that one's: this copy of the vault
            // leaves it, or the two would share one pairing.
            if session.data.sync_mut().is_some_and(|sync| sync.leave_lan_of_other_install(self.shared.install_id))
                && let Err(error) = self.write_vault(&session)
            {
                tracing::warn!(?error, "the vault without its LAN role was not written; the next unlock tries again");
            }
            st.phase = PhaseState::Unlocked(Box::new(session));
            // A device of a sync space syncs as soon as it is unlocked.
            sync_due(&mut st, Instant::now(), true);
        }
        self.changed();
        self.lan_unlocked();
        Ok(())
    }

    /// Lock: the keys and the decrypted entries are dropped (and zeroized).
    pub fn lock_vault(&self) {
        // A change still inside the backup debounce is backed up now, while the keys are at hand.
        if self.lock().auto_backup_at.is_some() {
            let _ = self.run_auto_backup();
        }
        let locked = {
            let mut st = self.lock();
            let was_unlocked = matches!(st.phase, PhaseState::Unlocked(_));
            if let PhaseState::Unlocked(session) = &st.phase {
                st.device_check = session.sealed.device_check().cloned();
            }
            if was_unlocked {
                st.phase = PhaseState::Locked;
                st.auto_backup_at = None;
                // A run in progress finds the vault locked and keeps nothing.
                st.sync.reset();
                Self::lan_locking(&mut st, self.shared.ports.sync.lan());
            }
            was_unlocked
        };
        if locked {
            self.changed();
            self.refresh_biometrics();
        }
    }

    /// Forgotten password: move the vault aside (`vault.lockra.reset-<ms>`, nothing is deleted)
    /// and start over at the welcome screen.
    pub fn reset_vault(&self) -> CoreResult<()> {
        let vault_id = {
            let mut st = self.lock();
            if !matches!(st.phase, PhaseState::Locked) {
                return Err(ErrorCode::NoVault.into());
            }
            let now = self.now_ms();
            let path = self.vault_path();
            fs::rename(&path, sibling(&path, &format!(".reset-{now}")))?;
            let prev = sibling(&path, ".prev");
            if prev.exists() {
                fs::rename(&prev, sibling(&path, &format!(".reset-{now}.prev")))?;
            }
            st.phase = PhaseState::NoVault;
            st.device_slot = false;
            st.failed_attempts = 0;
            st.retry_at_ms = None;
            st.vault_id.take()
        };
        if let Some(id) = vault_id {
            let _ = self.shared.ports.secrets.delete(&id.to_string());
        }
        self.changed();
        Ok(())
    }

    /// A new master password; "remember on this device" survives when the keychain still
    /// returns its key.
    pub async fn change_password(&self, current: Zeroizing<String>, new: Zeroizing<String>) -> CoreResult<()> {
        check_password(&new)?;
        let (mut sealed, vault_id, device, space) = {
            let mut st = self.lock();
            let device = st.device_slot;
            let session = unlocked_mut(&mut st)?;
            let space = session.data.sync().map(|sync| (sync.space_id, sync.keys(), sync.sync_key()));
            (session.sealed.clone(), session.sealed.vault_id(), device, space)
        };
        let key = if device { self.device_key(vault_id).ok() } else { None };
        let cost = self.shared.config.kdf;
        let (sealed, outcome, keyring) = blocking(move || {
            let outcome = sealed.change_password(current.as_bytes(), new.as_bytes(), cost, key.as_ref())?;
            // This device's keyring in its sync space follows its master password; the next run
            // writes it with the snapshot.
            let keyring = match space {
                Some((space_id, Ok(keys), Ok(sync_key))) => Some((space_id, seal_keyring(&keys, &sync_key, new.as_bytes(), cost).map_err(|e| sync_error(&e))?)),
                _ => None,
            };
            Ok((sealed, outcome, keyring))
        })
        .await?;
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            if session.sealed.vault_id() != vault_id {
                return Err(ErrorCode::Internal.into());
            }
            let previous = std::mem::replace(&mut session.sealed, sealed);
            let previous_space = session.data.sync().cloned();
            if let (Some((space_id, keyring)), Some(sync)) = (&keyring, session.data.sync_mut())
                && sync.space_id == *space_id
            {
                sync.keyring = BASE64.encode(keyring);
                // Every storage holds the keyring before.
                sync.keyring_sealed_again();
            }
            if let Err(error) = self.write_vault(session) {
                session.sealed = previous;
                session.data.local_mut().sync = previous_space;
                return Err(error);
            }
            st.device_slot = session_device(&st);
            self.schedule_auto_backup(&mut st);
            if keyring.is_some() {
                sync_due(&mut st, Instant::now(), false);
            }
        }
        if outcome == DeviceSlot::Dropped {
            let _ = self.shared.ports.secrets.delete(&vault_id.to_string());
            self.notice(Notice::DeviceUnlockTurnedOff);
        }
        self.changed();
        Ok(())
    }

    /// Turn "remember on this device" on: a device slot whose key goes into the keychain.
    pub fn enable_device_unlock(&self) -> CoreResult<()> {
        if self.shared.ports.secrets.status() == KeychainStatus::Unavailable {
            return Err(ErrorCode::KeychainUnavailable.into());
        }
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let mut next = session.sealed.clone();
            let key = next.enable_device()?;
            let account = next.vault_id().to_string();
            self.shared.ports.secrets.set(&account, &key.to_text()).map_err(|_| ErrorCode::KeychainFailed)?;
            let previous = std::mem::replace(&mut session.sealed, next);
            if let Err(error) = self.write_vault(session) {
                session.sealed = previous;
                let _ = self.shared.ports.secrets.delete(&account);
                return Err(error);
            }
            st.device_slot = true;
        }
        self.changed();
        Ok(())
    }

    /// Turn "remember on this device" off: the slot goes, the data key rotates, the keychain
    /// entry is deleted.
    pub async fn disable_device_unlock(&self, password: Zeroizing<String>) -> CoreResult<()> {
        let mut sealed = {
            let st = self.lock();
            let session = unlocked(&st)?;
            if !session.sealed.has_device_slot() {
                return Err(ErrorCode::DeviceUnlockOff.into());
            }
            session.sealed.clone()
        };
        let vault_id = sealed.vault_id();
        let sealed = blocking(move || {
            sealed.disable_device(password.as_bytes())?;
            Ok(sealed)
        })
        .await?;
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let previous = std::mem::replace(&mut session.sealed, sealed);
            if let Err(error) = self.write_vault(session) {
                session.sealed = previous;
                return Err(error);
            }
            st.device_slot = false;
        }
        let _ = self.shared.ports.secrets.delete(&vault_id.to_string());
        self.changed();
        Ok(())
    }

    // ---- entries --------------------------------------------------------------------------

    /// Add an account from an `otpauth://` URI.
    pub fn add_uri(&self, text: &str) -> CoreResult<Uuid> {
        let auth = uri::parse(text).map_err(|error| uri_error(&error))?;
        self.add_entry(Entry::from_auth(auth, Origin::Uri, self.now_ms()))
    }

    /// Add an account typed in by hand.
    pub fn add_manual(&self, draft: EntryDraft) -> CoreResult<Uuid> {
        let secret = base32::decode(&draft.secret).map_err(|_| ErrorCode::InvalidSecret)?;
        let auth =
            lockra_otp::OtpAuth { kind: draft.kind, algorithm: draft.algorithm, digits: draft.digits, secret, issuer: draft.issuer, account: draft.account };
        let mut entry = Entry::from_auth(auth, Origin::Manual, self.now_ms());
        entry.group = draft.group.map(|g| clean_name(&g)).filter(|g| !g.is_empty());
        self.add_entry(entry)
    }

    fn add_entry(&self, mut entry: Entry) -> CoreResult<Uuid> {
        let id = entry.id;
        let now = self.now_ms();
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            if session.data.entries.iter().any(|e| e.same_account(&entry.to_auth())) {
                return Err(ErrorCode::DuplicateEntry.into());
            }
            entry.stamp = session.data.tick(now);
            session.data.entries.push(entry);
            self.save(&mut st, true, |s| {
                s.data.entries.pop();
            })?;
        }
        self.changed();
        Ok(id)
    }

    /// Rename, regroup, pin or unpin an entry.
    pub fn update_entry(&self, id: Uuid, patch: EntryPatch) -> CoreResult<()> {
        let now = self.now_ms();
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let stamp = session.data.tick(now);
            let entry = session.data.get_mut(id).ok_or(ErrorCode::EntryNotFound)?;
            let before = entry.clone();
            entry.stamp = stamp;
            if let Some(issuer) = patch.issuer {
                entry.issuer = clean_name(&issuer);
            }
            if let Some(account) = patch.account {
                entry.account = clean_name(&account);
            }
            if let Some(group) = patch.group {
                entry.group = Some(clean_name(&group)).filter(|g| !g.is_empty());
            }
            if let Some(favorite) = patch.favorite {
                entry.favorite = favorite;
            }
            if let Some(color) = patch.color {
                entry.color = color;
            }
            if let Some(mark) = patch.mark {
                entry.mark = clean_mark(&mark);
            }
            entry.updated_at_ms = now;
            self.save(&mut st, true, move |s| {
                if let Some(e) = s.data.get_mut(id) {
                    *e = before;
                }
            })?;
        }
        self.changed();
        Ok(())
    }

    /// Put the entries `ids` in `group` (out of any group when it is empty), in one write. Each
    /// entry that changes gets a new stamp, as an edit of that one account would; none changes when
    /// one of them is not there.
    pub fn set_entries_group(&self, ids: &[Uuid], group: &str) -> CoreResult<()> {
        let group = Some(clean_name(group)).filter(|g| !g.is_empty());
        let now = self.now_ms();
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let wanted: std::collections::BTreeSet<Uuid> = ids.iter().copied().collect();
            if !wanted.iter().all(|id| session.data.entries.iter().any(|e| e.id == *id)) {
                return Err(ErrorCode::EntryNotFound.into());
            }
            let mut before = Vec::new();
            for id in wanted {
                if session.data.entries.iter().any(|e| e.id == id && e.group == group) {
                    continue;
                }
                let stamp = session.data.tick(now);
                let entry = session.data.get_mut(id).ok_or(ErrorCode::EntryNotFound)?;
                before.push(entry.clone());
                entry.group.clone_from(&group);
                entry.stamp = stamp;
                entry.updated_at_ms = now;
            }
            if before.is_empty() {
                return Ok(());
            }
            self.save(&mut st, true, move |s| {
                for old in before {
                    if let Some(entry) = s.data.get_mut(old.id) {
                        *entry = old;
                    }
                }
            })?;
        }
        self.changed();
        Ok(())
    }

    /// Fold `groups` in the code list ("" for the accounts in no group) and unfold the others. Kept
    /// in the vault's local part: this device's view, never synced and never in a backup, and no
    /// change of the accounts (no backup or sync follows).
    pub fn collapse_groups(&self, groups: Vec<String>) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let existing: std::collections::BTreeSet<String> = session.data.entries.iter().filter_map(|e| e.group.clone()).collect();
            let folded: std::collections::BTreeSet<String> = groups.iter().map(|g| clean_name(g)).filter(|g| g.is_empty() || existing.contains(g)).collect();
            let folded: Vec<String> = folded.into_iter().collect();
            if session.data.collapsed_groups() == folded.as_slice() {
                return Ok(());
            }
            let previous = std::mem::replace(&mut session.data.local_mut().view.collapsed_groups, folded);
            self.save(&mut st, false, move |s| s.data.local_mut().view.collapsed_groups = previous)?;
        }
        self.changed();
        Ok(())
    }

    /// Delete an entry.
    pub fn delete_entry(&self, id: Uuid) -> CoreResult<()> {
        let now = self.now_ms();
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let tombstones = session.data.tombstones.clone();
            let (index, removed) = session.data.remove(id, now).ok_or(ErrorCode::EntryNotFound)?;
            self.save(&mut st, true, move |s| {
                s.data.entries.insert(index, removed);
                s.data.tombstones = tombstones;
            })?;
        }
        self.changed();
        Ok(())
    }

    /// HOTP: advance the counter. It is written to disk before the new code is shown, so a crash
    /// can never show the same code twice.
    pub fn hotp_next(&self, id: Uuid) -> CoreResult<()> {
        let now = self.now_ms();
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let stamp = session.data.tick(now);
            let entry = session.data.get_mut(id).ok_or(ErrorCode::EntryNotFound)?;
            let OtpKind::Hotp { counter } = entry.kind else { return Err(ErrorCode::InvalidParameters.into()) };
            let before = entry.stamp;
            entry.kind = OtpKind::Hotp { counter: counter.checked_add(1).ok_or(ErrorCode::InvalidParameters)? };
            // The next counter is a change like any other: it syncs.
            entry.stamp = stamp;
            self.save(&mut st, true, move |s| {
                if let Some(e) = s.data.get_mut(id) {
                    e.kind = OtpKind::Hotp { counter };
                    e.stamp = before;
                }
            })?;
        }
        self.changed();
        Ok(())
    }

    /// Copy the current code; it is cleared again after the configured time if still there.
    pub fn copy_code(&self, id: Uuid) -> CoreResult<()> {
        let now = self.now_ms();
        let clear_after = {
            let mut st = self.lock();
            let clear_seconds = st.settings.clipboard_clear_seconds;
            let session = unlocked_mut(&mut st)?;
            let entry = session.data.get_mut(id).ok_or(ErrorCode::EntryNotFound)?;
            let code = Zeroizing::new(current_code(entry, now));
            self.shared.ports.clipboard.set_secret_text(&code).map_err(|_| ErrorCode::ClipboardFailed)?;
            entry.last_used_at_ms = Some(now);
            // Recency only: no automatic backup for a copy, and a failed write loses nothing.
            if let Err(error) = self.write_vault(session) {
                tracing::warn!(?error, "could not record the copy time");
            }
            st.clipboard = (clear_seconds > 0).then(|| (code, Instant::now() + Duration::from_secs(u64::from(clear_seconds))));
            st.last_activity = Instant::now();
            (clear_seconds > 0).then_some(clear_seconds)
        };
        self.changed();
        self.notice(Notice::Copied { entry_id: id, clear_after_s: clear_after });
        Ok(())
    }

    /// The secret, its URI and its QR code, after the master password was entered again.
    pub async fn reveal(&self, id: Uuid, password: Zeroizing<String>) -> CoreResult<Revealed> {
        let (sealed, auth) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            (session.sealed.clone(), session.data.get(id).ok_or(ErrorCode::EntryNotFound)?.to_auth())
        };
        blocking(move || {
            sealed.verify_password(password.as_bytes())?;
            let uri = auth.to_uri();
            let svg = qr::svg(&uri).map_err(|_| ErrorCode::Internal)?;
            let secret = base32::encode(&auth.secret).as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join(" ");
            Ok(Revealed { entry_id: id, secret, uri: uri.to_string(), svg: svg.to_string() })
        })
        .await
    }

    /// `Ok` when `password` is the master password (the shell checks it before a save dialog).
    pub async fn verify_password(&self, password: Zeroizing<String>) -> CoreResult<()> {
        let sealed = {
            let st = self.lock();
            unlocked(&st)?.sealed.clone()
        };
        blocking(move || Ok(sealed.verify_password(password.as_bytes())?)).await
    }

    // ---- import ---------------------------------------------------------------------------

    /// Read picked or dropped files into the import preview.
    pub async fn import_files(&self, paths: Vec<PathBuf>) -> CoreResult<()> {
        self.ensure_unlocked()?;
        let found = blocking(move || Ok(read_import_files(&paths))).await?;
        self.stage_files(found)
    }

    /// Read files picked on the phone into the import preview, as `import_files` reads picked
    /// or dropped ones: the photo picker hands over their bytes, not a path.
    pub async fn import_picked(&self, files: Vec<PickedFile>) -> CoreResult<()> {
        self.ensure_unlocked()?;
        let found = blocking(move || {
            let mut notices = Vec::new();
            let mut read = Vec::new();
            for file in files {
                if file.bytes.len() as u64 > MAX_IMPORT_BYTES {
                    notices.push(Notice::FileUnreadable { name: file.name });
                } else {
                    read.push((file.name, file.bytes));
                }
            }
            Ok(read_import_bytes(read, notices))
        })
        .await?;
        self.stage_files(found)
    }

    /// What files held, into the import preview with their notices; `ImportEmpty` when nothing
    /// was there before and none of them holds anything.
    fn stage_files(&self, found: FoundFiles) -> CoreResult<()> {
        let mut notices = found.notices;
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let mut import = session.import.take().unwrap_or_default();
            let had_anything = !import.is_empty();
            for batch in found.batches {
                batch.apply(&mut import);
            }
            if let Some(awaiting) = found.awaiting {
                import.awaiting = Some(awaiting);
            }
            if import.is_empty() && !had_anything {
                for n in notices.drain(..) {
                    self.notice(n);
                }
                return Err(ErrorCode::ImportEmpty.into());
            }
            session.import = Some(import);
        }
        for n in notices {
            self.notice(n);
        }
        self.changed();
        Ok(())
    }

    /// Read the clipboard (an image with QR codes, or text with URIs) into the import preview.
    pub async fn import_clipboard(&self) -> CoreResult<()> {
        self.ensure_unlocked()?;
        let clipboard = Arc::clone(&self.shared.ports.clipboard);
        let batches = blocking(move || {
            if let Ok(Some(image)) = clipboard.image() {
                let texts = qr::decode_rgba(image.width, image.height, &image.rgba).unwrap_or_default();
                if !texts.is_empty() {
                    return Ok(texts.iter().flat_map(|t| text_batches(&ImportSource::Clipboard, t)).collect::<Vec<_>>());
                }
            }
            // Text counts only when it holds a URI: other text on the clipboard is not an import.
            let text = clipboard.text().map_err(|_| ErrorCode::ClipboardFailed)?.filter(|t| t.to_ascii_lowercase().contains("otpauth"));
            Ok(text.map(|t| text_batches(&ImportSource::Clipboard, &t)).unwrap_or_default())
        })
        .await?;
        if batches.is_empty() {
            return Err(ErrorCode::ClipboardEmpty.into());
        }
        self.stage(batches)
    }

    /// Read a QR code the phone's camera scanned into the import preview. A QR code is one piece
    /// of text: the line an account came from means nothing there.
    pub fn import_scanned(&self, text: &str) -> CoreResult<()> {
        self.ensure_unlocked()?;
        let mut batches = text_batches(&ImportSource::Camera, text);
        for batch in &mut batches {
            batch.forget_lines();
        }
        if batches.is_empty() {
            return Err(ErrorCode::ImportEmpty.into());
        }
        self.stage(batches)
    }

    /// Read pasted or typed text into the import preview.
    pub fn import_text(&self, text: &str) -> CoreResult<()> {
        self.ensure_unlocked()?;
        let batches = text_batches(&ImportSource::Text, text);
        if batches.is_empty() {
            return Err(ErrorCode::ImportEmpty.into());
        }
        self.stage(batches)
    }

    fn stage(&self, batches: Vec<Batch>) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let import = session.import.get_or_insert_with(ImportSession::default);
            for batch in batches {
                batch.apply(import);
            }
        }
        self.changed();
        Ok(())
    }

    /// Open the Lockra backup waiting in the import with its password and list its entries.
    pub async fn import_backup_password(&self, password: Zeroizing<String>) -> CoreResult<()> {
        let (bytes, name) = {
            let st = self.lock();
            let awaiting = unlocked(&st)?.import.as_ref().and_then(|i| i.awaiting.as_ref()).ok_or(ErrorCode::NoImport)?;
            (awaiting.bytes.clone(), awaiting.name.clone())
        };
        let data = blocking(move || {
            let opened = Sealed::open_with_password(&bytes, password.as_bytes())?;
            VaultData::open(&opened.payload)
        })
        .await?;
        {
            let mut st = self.lock();
            let import = unlocked_mut(&mut st)?.import.as_mut().ok_or(ErrorCode::NoImport)?;
            import.awaiting = None;
            import.add_backup(&ImportSource::File { name }, data.entries);
        }
        self.changed();
        Ok(())
    }

    /// Apply the import with the user's choices.
    pub fn import_commit(&self, choices: &[Choice]) -> CoreResult<Outcome> {
        let now = self.now_ms();
        let outcome = {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let import = session.import.take().ok_or(ErrorCode::NoImport)?;
            let before = session.data.clone();
            let outcome = import.commit(&mut session.data, choices, now);
            self.save(&mut st, true, move |s| s.data = before)?;
            outcome
        };
        self.notice(Notice::Imported { added: outcome.added, replaced: outcome.replaced, skipped: outcome.skipped });
        self.changed();
        Ok(outcome)
    }

    /// Drop the import preview.
    pub fn import_cancel(&self) {
        let changed = {
            let mut st = self.lock();
            match &mut st.phase {
                PhaseState::Unlocked(session) => session.import.take().is_some(),
                _ => false,
            }
        };
        if changed {
            self.changed();
        }
    }

    // ---- export ---------------------------------------------------------------------------

    /// Build the QR codes for `entry_ids` after checking the master password again.
    pub async fn export_start(&self, target: ExportTarget, entry_ids: &[Uuid], password: Zeroizing<String>) -> CoreResult<ExportStarted> {
        let (sealed, entries) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            let entries: Vec<Entry> = entry_ids.iter().filter_map(|id| session.data.get(*id).cloned()).collect();
            (session.sealed.clone(), entries)
        };
        if entries.is_empty() {
            return Err(ErrorCode::ExportNothing.into());
        }
        let (pages, excluded) = blocking(move || {
            sealed.verify_password(password.as_bytes())?;
            export::build(target, &entries)
        })
        .await?;
        let id = Uuid::new_v4();
        let total = u32::try_from(pages.len()).unwrap_or(u32::MAX);
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            session.exports.push(ExportSession { id, pages, expires_at: Instant::now() + EXPORT_IDLE });
            st.last_activity = Instant::now();
        }
        self.shared.wake.notify_one();
        Ok(ExportStarted { session: id, target, pages: total, excluded })
    }

    /// One code of an export session; showing it keeps the session alive.
    pub fn export_page(&self, session_id: Uuid, index: u32) -> CoreResult<ExportPage> {
        let page = {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let export = session.exports.iter_mut().find(|e| e.id == session_id).ok_or(ErrorCode::ExportExpired)?;
            export.expires_at = Instant::now() + EXPORT_IDLE;
            let total = u32::try_from(export.pages.len()).unwrap_or(u32::MAX);
            let page = export.pages.get(index as usize).ok_or(ErrorCode::ExportExpired)?;
            let page = ExportPage { session: session_id, index, total, svg: page.svg.to_string(), entry_ids: page.entry_ids.clone() };
            // Paging through codes is the user at work: the vault does not lock under them.
            st.last_activity = Instant::now();
            page
        };
        self.shared.wake.notify_one();
        Ok(page)
    }

    /// Close an export session.
    pub fn export_close(&self, session_id: Uuid) {
        let mut st = self.lock();
        if let PhaseState::Unlocked(session) = &mut st.phase {
            session.exports.retain(|e| e.id != session_id);
        }
    }

    /// Write `entry_ids` as a plain `otpauth://` list to `path` (the user confirmed it is plaintext).
    pub async fn export_otpauth_file(&self, entry_ids: &[Uuid], password: Zeroizing<String>, path: PathBuf) -> CoreResult<String> {
        let list = self.export_otpauth_text(entry_ids, password).await?;
        blocking(move || {
            write_private(&path, list.as_bytes())?;
            Ok(file_name(&path))
        })
        .await
    }

    /// `entry_ids` as a plain `otpauth://` list, after checking the master password, for a shell
    /// that saves files itself (the phone's file picker).
    pub async fn export_otpauth_text(&self, entry_ids: &[Uuid], password: Zeroizing<String>) -> CoreResult<Zeroizing<String>> {
        let (sealed, auths) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            let auths: Vec<lockra_otp::OtpAuth> = entry_ids.iter().filter_map(|id| session.data.get(*id).map(Entry::to_auth)).collect();
            (session.sealed.clone(), auths)
        };
        if auths.is_empty() {
            return Err(ErrorCode::ExportNothing.into());
        }
        blocking(move || {
            sealed.verify_password(password.as_bytes())?;
            let refs: Vec<&lockra_otp::OtpAuth> = auths.iter().collect();
            Ok(text::write(&refs))
        })
        .await
    }

    // ---- backup and restore ---------------------------------------------------------------

    /// Write a backup to `path`: under the master password, or under `separate` if given.
    pub async fn backup_to(&self, path: PathBuf, separate: Option<Zeroizing<String>>) -> CoreResult<String> {
        let bytes = self.backup_sealed(separate).await?;
        let name = blocking(move || {
            write_private(&path, &bytes)?;
            Ok(file_name(&path))
        })
        .await?;
        self.backup_recorded(&name);
        Ok(name)
    }

    /// A backup as bytes, for a shell that saves files itself (the phone's file picker): under the
    /// master password, or under `separate` if given. Nothing is recorded before
    /// [`Self::backup_recorded`].
    pub async fn backup_sealed(&self, separate: Option<Zeroizing<String>>) -> CoreResult<Vec<u8>> {
        if let Some(password) = &separate {
            check_password(password)?;
        }
        let (sealed, payload) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            (session.sealed.clone(), session.data.backup_bytes())
        };
        let cost = self.shared.config.kdf;
        blocking(move || {
            Ok(match separate {
                Some(password) => Sealed::create_for(sealed.vault_id(), password.as_bytes(), cost, sealed.created_at_ms())?.seal(FileKind::Backup, &payload)?,
                None => sealed.seal(FileKind::Backup, &payload)?,
            })
        })
        .await
    }

    /// A backup was saved as `file_name`: the time of the last backup moves, and the notice says so.
    pub fn backup_recorded(&self, file_name: &str) {
        self.lock().last_backup_ms = Some(self.now_ms());
        self.notice(Notice::BackupWritten { file_name: file_name.to_owned(), automatic: false });
        self.changed();
    }

    /// Run the automatic backup now (Settings › Backup › "back up now").
    pub fn backup_auto_now(&self) -> CoreResult<()> {
        self.run_auto_backup()
    }

    /// Choose the automatic backup folder: it must exist, be writable, and have a Unicode name.
    pub fn set_auto_backup_dir(&self, dir: &Path) -> CoreResult<()> {
        let text = dir.to_str().ok_or(ErrorCode::BackupDirUnavailable)?.to_owned();
        probe_writable(dir).map_err(|_| ErrorCode::BackupDirUnavailable)?;
        let mut settings = self.lock().settings.clone();
        settings.auto_backup.dir = Some(text);
        self.set_settings(settings)
    }

    /// Open a backup (or a vault file) for restoring; its password comes with [`Self::restore_commit`].
    pub async fn restore_open(&self, path: PathBuf) -> CoreResult<()> {
        if matches!(self.lock().phase, PhaseState::Locked) {
            return Err(ErrorCode::Locked.into());
        }
        let name = file_name(&path);
        let bytes = blocking(move || Ok(Zeroizing::new(read_limited(&path)?))).await?;
        self.restore_open_bytes(name, bytes).await
    }

    /// Open a backup's bytes for restoring, as [`Self::restore_open`] opens a file: the phone's file
    /// picker hands over bytes, not a path.
    pub async fn restore_open_bytes(&self, name: String, bytes: Zeroizing<Vec<u8>>) -> CoreResult<()> {
        if matches!(self.lock().phase, PhaseState::Locked) {
            return Err(ErrorCode::Locked.into());
        }
        if bytes.len() as u64 > MAX_IMPORT_BYTES {
            return Err(ErrorCode::ImportUnreadable.into());
        }
        let (bytes, info) = blocking(move || {
            let info = read_header(&bytes)?;
            Ok((bytes, info))
        })
        .await?;
        let view = RestoreView { file_name: name.clone(), kind: info.kind, created_at_ms: info.created_at_ms };
        self.lock().restore = Some(Restore { name, bytes, view });
        self.changed();
        Ok(())
    }

    /// Restore the opened backup. With no vault yet it becomes the vault and its password the
    /// master password; into an unlocked vault it is merged (through the import preview) or
    /// replaces the entries (after `pre-restore-<time>.lockrabackup` keeps the current ones).
    pub async fn restore_commit(&self, password: Zeroizing<String>, mode: RestoreMode) -> CoreResult<()> {
        let bytes = self.lock().restore.as_ref().map(|r| r.bytes.clone()).ok_or(ErrorCode::NoRestore)?;
        let opened = blocking(move || Ok(Sealed::open_with_password(&bytes, password.as_bytes())?)).await?;
        let data = VaultData::open(&opened.payload)?;
        let now = self.now_ms();
        let notice = {
            let mut st = self.lock();
            let restore = st.restore.take().ok_or(ErrorCode::NoRestore)?;
            match &mut st.phase {
                PhaseState::NoVault => {
                    fs::create_dir_all(&self.shared.config.data_dir)?;
                    restrict_dir(&self.shared.config.data_dir);
                    let mut session = Session { sealed: opened.sealed, data, import: None, exports: Vec::new() };
                    // A vault file (not a backup) brings its sync space along; a LAN role in it
                    // stays with the installation that took it.
                    if let Some(sync) = session.data.sync_mut() {
                        sync.leave_lan_of_other_install(self.shared.install_id);
                    }
                    if let Err(error) = self.write_vault(&session) {
                        st.restore = Some(restore);
                        return Err(error);
                    }
                    let entries = u32::try_from(session.data.entries.len()).unwrap_or(u32::MAX);
                    st.vault_id = Some(session.sealed.vault_id());
                    st.device_slot = false;
                    st.phase = PhaseState::Unlocked(Box::new(session));
                    sync_due(&mut st, Instant::now(), true);
                    st.last_activity = Instant::now();
                    Some(Notice::Restored { entries })
                }
                PhaseState::Locked => {
                    st.restore = Some(restore);
                    return Err(ErrorCode::Locked.into());
                }
                PhaseState::Unlocked(session) => match mode {
                    RestoreMode::Merge => {
                        session.import.get_or_insert_with(ImportSession::default).add_backup(&ImportSource::File { name: restore.name }, data.entries);
                        None
                    }
                    RestoreMode::Replace => {
                        let keep = session.sealed.seal(FileKind::Backup, &session.data.backup_bytes())?;
                        write_private(&self.shared.config.data_dir.join(pre_restore_file_name(now)), &keep)?;
                        let before = session.data.clone();
                        // A change made now: this device's part stays, and a sync space ends up
                        // with the backup's accounts.
                        session.data.replace_entries(data.entries, &data.tombstones, now);
                        let entries = u32::try_from(session.data.entries.len()).unwrap_or(u32::MAX);
                        self.save(&mut st, true, move |s| s.data = before)?;
                        Some(Notice::Restored { entries })
                    }
                },
            }
        };
        if let Some(n) = notice {
            self.notice(n);
        }
        self.changed();
        Ok(())
    }

    /// Forget the opened backup.
    pub fn restore_cancel(&self) {
        if self.lock().restore.take().is_some() {
            self.changed();
        }
    }

    // ---- settings, activity, codes --------------------------------------------------------

    /// Save new settings.
    pub fn set_settings(&self, settings: Settings) -> CoreResult<()> {
        let settings = settings.normalized();
        if settings.auto_backup.enabled && settings.auto_backup.dir.is_none() {
            return Err(ErrorCode::BackupDirMissing.into());
        }
        self.shared.settings_store.save(&settings)?;
        let update_turned_on = {
            let mut st = self.lock();
            let backup_turned_on = settings.auto_backup.enabled && !st.settings.auto_backup.enabled;
            if !settings.auto_backup.enabled {
                st.auto_backup_at = None;
            }
            let update_turned_on = settings.auto_update && !st.settings.auto_update;
            if !settings.auto_update {
                st.update.auto_at = None;
            }
            st.settings = settings;
            if backup_turned_on {
                self.schedule_auto_backup(&mut st);
            }
            update_turned_on
        };
        self.changed();
        if update_turned_on && self.shared.ports.updater.method().is_some_and(InstallMethod::installs) {
            // Turned on: look and download now; only a restart installs. Skipped while the user
            // runs one, which answers the same question.
            let _ = self.start_update(UpdateRun::Auto { install_version: None });
        }
        Ok(())
    }

    /// The user did something: the idle timer restarts.
    pub fn activity(&self) {
        self.lock().last_activity = Instant::now();
        self.shared.wake.notify_one();
    }

    /// Stream code frames to `sink` (replacing any previous subscriber); the first frame goes out now.
    pub fn subscribe_codes(&self, sink: Arc<dyn CodeSink>) {
        self.lock().codes = Some(sink);
        self.push_codes();
        self.shared.wake.notify_one();
    }

    /// Stop streaming code frames.
    pub fn unsubscribe_codes(&self) {
        self.lock().codes = None;
    }

    // ---- updates --------------------------------------------------------------------------

    /// Look for a newer release now, afresh; the answer arrives in the state (`update.status`).
    pub fn update_check(&self) -> CoreResult<()> {
        self.start_update(UpdateRun::Check)
    }

    /// Install the newest release and restart: what a check found is not asked for again, and a
    /// package already downloaded is not downloaded again. The progress arrives in the state; with
    /// nothing newer the run ends up to date.
    pub fn update_install(&self) -> CoreResult<()> {
        self.start_update(UpdateRun::Install)
    }

    /// One run at a time, in the background.
    fn start_update(&self, run: UpdateRun) -> CoreResult<()> {
        let Some(method) = self.shared.ports.updater.method() else { return Err(ErrorCode::UpdateUnavailable.into()) };
        // The phone checks only: a newer release opens its page.
        if run != UpdateRun::Check && !method.installs() {
            return Err(ErrorCode::UpdateUnavailable.into());
        }
        {
            let mut st = self.lock();
            if st.update.busy {
                return Err(ErrorCode::UpdateBusy.into());
            }
            st.update.busy = true;
        }
        let core = self.clone();
        tokio::spawn(async move { core.run_update(run).await });
        Ok(())
    }

    async fn run_update(&self, run: UpdateRun) {
        let outcome = self.update_steps(&run).await;
        let now = self.now_ms();
        {
            let mut st = self.lock();
            st.update.busy = false;
            match outcome {
                Ok(Some(status)) => st.update.status = status,
                // Installed: the shell restarts Lockra.
                Ok(None) => {}
                Err(failure) => st.update.status = UpdateStatus::Failed { code: failure_code(failure), at_ms: now },
            }
        }
        self.changed();
    }

    /// The run's steps; the status it ends on, or `None` once the package is installed. A failed
    /// step leaves nothing for the next run, which asks again.
    async fn update_steps(&self, run: &UpdateRun) -> Result<Option<UpdateStatus>, crate::ports::UpdateFailure> {
        let updater = Arc::clone(&self.shared.ports.updater);
        let data_dir = self.shared.config.data_dir.clone();
        let pending = self.lock().update.pending.take().filter(|_| *run != UpdateRun::Check);
        let Pending { release, downloaded } = match pending {
            Some(pending) => pending,
            None => {
                self.set_update(UpdateStatus::Checking);
                let checked_at_ms = self.now_ms();
                let Some(release) = updater.check().await? else {
                    clear_marker(&data_dir);
                    return Ok(Some(UpdateStatus::UpToDate { checked_at_ms }));
                };
                let available =
                    UpdateStatus::Available { version: release.version.clone(), notes: release.notes.clone(), date: release.date.clone(), checked_at_ms };
                if !run.downloads() {
                    self.lock().update.pending = Some(Pending { release, downloaded: false });
                    return Ok(Some(available));
                }
                self.set_update(available);
                Pending { release, downloaded: false }
            }
        };
        let version = release.version.clone();
        if !downloaded {
            self.set_update(UpdateStatus::Downloading { version: version.clone(), received: 0, total: None });
            let core = self.clone();
            let downloading = version.clone();
            let mut gate = ProgressGate::default();
            updater
                .download(Box::new(move |received, total| {
                    if gate.step(received, total) {
                        core.set_update(UpdateStatus::Downloading { version: downloading.clone(), received, total });
                    }
                }))
                .await?;
            write_marker(&data_dir, &version);
        }
        if !run.installs(&version) {
            self.lock().update.pending = Some(Pending { release, downloaded: true });
            return Ok(Some(UpdateStatus::Ready { version }));
        }
        if !downloaded {
            self.set_update(UpdateStatus::Ready { version: version.clone() });
        }
        // The process ends with the install: a change still inside the backup debounce is backed
        // up first.
        if self.lock().auto_backup_at.is_some() {
            let _ = self.run_auto_backup();
        }
        self.set_update(UpdateStatus::Installing { version });
        updater.install().await?;
        clear_marker(&data_dir);
        Ok(None)
    }

    /// Record an intermediate status and send the state (no code frame: nothing about the codes
    /// changed).
    fn set_update(&self, status: UpdateStatus) {
        self.lock().update.status = status;
        let _ = self.shared.events.send(UiEvent::State { state: Box::new(self.state()) });
    }

    // ---- sync -----------------------------------------------------------------------------

    /// Set up sync on a new space at `storage` (Settings › Sync), after the master password was
    /// entered again: a new sync key, which the answer carries to be shown once, and this
    /// device's keyring under the master password. The first run happens here, so a storage that
    /// refuses the snapshot fails the setup with its reason.
    pub async fn sync_create(&self, storage: StorageConfig, password: Zeroizing<String>, device: String) -> CoreResult<SyncCreated> {
        storage.validate().map_err(config_error)?;
        let (sealed, mut working, number) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            if session.data.sync().is_some() {
                return Err(ErrorCode::SyncAlreadyOn.into());
            }
            let working = Working { entries: session.data.entries.clone(), tombstones: session.data.tombstones.clone() };
            (session.sealed.clone(), working, session.data.device())
        };
        let vault_id = sealed.vault_id();
        let remote = self.open_storage(&storage)?;
        let cost = self.shared.config.kdf;
        let (keys, sync_key, keyring) = blocking(move || {
            sealed.verify_password(password.as_bytes())?;
            let sync_key = SyncKey::generate().map_err(|e| sync_error(&e))?;
            let keys = SpaceKeys::generate(sync_key.space_id()).map_err(|e| sync_error(&e))?;
            let keyring = seal_keyring(&keys, &sync_key, password.as_bytes(), cost).map_err(|e| sync_error(&e))?;
            Ok((keys, sync_key, keyring))
        })
        .await?;
        let mut space = SyncLocal::new(Some(storage), &keys, &sync_key, device_name(&device, self.shared.config.platform), &keyring);
        // The key is shown once now; Settings › Sync reminds of it until it is saved.
        space.key_saved = false;
        let name = space.device_name.clone();
        let cloud = space.cloud.as_mut().ok_or(ErrorCode::Internal)?;
        let first = Space { prefix: cloud.storage.prefix(), keys: &keys, device: number, device_name: &name, keyring: &keyring, seq_floor: 0 };
        step(&*remote, &first, &mut cloud.sync.state, &mut working, self.now_ms()).await.map_err(|e| sync_error(&e))?;
        cloud.sync.keyring_written = true;
        cloud.sync.last_ok_ms = Some(self.now_ms());
        let text = sync_key.to_text();
        self.join_space(vault_id, space, Some(remote))?;
        Ok(SyncCreated { sync_key: text.to_string() })
    }

    /// Join an existing space, from another device's invitation or from the storage and the sync
    /// key typed in. `password` is this device's master password: unlocked, it must be the vault's,
    /// and the vault's accounts join the space; with no vault yet, a new vault is made under it.
    /// The space opens with the master password of any of its devices: `space_password` when
    /// theirs is not `password`. This device's keyring goes in under `password`.
    pub async fn sync_join(
        &self,
        source: JoinSource,
        password: Zeroizing<String>,
        device: String,
        space_password: Option<Zeroizing<String>>,
    ) -> CoreResult<()> {
        let (storage, sync_key) = match source {
            JoinSource::Invite { text, code } => {
                // A sealed text costs an Argon2id derivation to open.
                let invite = blocking(move || Invite::from_any_text(&text, code.as_ref().map(|c| c.as_str())).map_err(|e| sync_error(&e))).await?;
                (invite.storage, invite.sync_key)
            }
            JoinSource::Manual { storage, sync_key } => (storage, SyncKey::from_text(&sync_key).map_err(|e| sync_error(&e))?),
        };
        storage.validate().map_err(config_error)?;
        let existing = {
            let st = self.lock();
            match &st.phase {
                PhaseState::Locked => return Err(ErrorCode::Locked.into()),
                PhaseState::NoVault => {
                    check_password(&password)?;
                    None
                }
                PhaseState::Unlocked(session) => {
                    if session.data.sync().is_some() {
                        return Err(ErrorCode::SyncAlreadyOn.into());
                    }
                    Some(session.sealed.clone())
                }
            }
        };
        if let Some(sealed) = existing.clone() {
            let checked = password.clone();
            blocking(move || Ok(sealed.verify_password(checked.as_bytes())?)).await?;
        }
        let remote = self.open_storage(&storage)?;
        let space_id = sync_key.space_id();
        // Without a password of its own, a space whose devices use another one asks for it, rather
        // than calling this vault's (verified) password wrong.
        let ask_for_space_password = existing.is_some() && space_password.is_none();
        let opening = space_password.unwrap_or_else(|| password.clone());
        let keys = open_space(&*remote, storage.prefix(), space_id, |keyring| {
            let (sync_key, opening) = (sync_key.clone(), opening.clone());
            async move {
                tokio::task::spawn_blocking(move || open_keyring(&keyring, space_id, &sync_key, opening.as_bytes()))
                    .await
                    .map_err(|_| SyncError::Interrupted)?
            }
        })
        .await
        .map_err(|e| match e {
            SyncError::WrongCredentials if ask_for_space_password => CoreError::from(ErrorCode::SyncSpacePasswordNeeded),
            other => sync_error(&other),
        })?;
        let (cost, now, data_dir) = (self.shared.config.kdf, self.now_ms(), self.shared.config.data_dir.clone());
        let create = existing.is_none();
        let (keys, sealed, sync_key, keyring) = blocking(move || {
            let keyring = seal_keyring(&keys, &sync_key, password.as_bytes(), cost).map_err(|e| sync_error(&e))?;
            let sealed = if create {
                let sealed = Sealed::create(password.as_bytes(), cost, now)?;
                fs::create_dir_all(&data_dir)?;
                restrict_dir(&data_dir);
                Some(sealed)
            } else {
                None
            };
            Ok((keys, sealed, sync_key, keyring))
        })
        .await?;
        let space = SyncLocal::new(Some(storage), &keys, &sync_key, device_name(&device, self.shared.config.platform), &keyring);
        match (existing.as_ref().map(Sealed::vault_id), sealed) {
            (Some(vault_id), _) => self.join_space(vault_id, space, Some(remote)),
            (None, Some(sealed)) => self.create_joined(sealed, space, Some(remote)),
            (None, None) => Err(ErrorCode::Internal.into()),
        }
    }

    /// Record `space` in the unlocked vault `vault_id`, its cloud storage opened as `remote`; the
    /// first run follows.
    fn join_space(&self, vault_id: Uuid, space: SyncLocal, remote: Option<Arc<dyn RemoteStore>>) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            if session.sealed.vault_id() != vault_id {
                return Err(ErrorCode::Internal.into());
            }
            if session.data.sync().is_some() {
                return Err(ErrorCode::SyncAlreadyOn.into());
            }
            let cache = cloud_cache(&space, remote);
            session.data.local_mut().sync = Some(space);
            self.save(&mut st, false, |s| s.data.local_mut().sync = None)?;
            st.sync.cloud.remote = cache;
            sync_due(&mut st, Instant::now(), true);
        }
        self.changed();
        Ok(())
    }

    /// A new vault, made by joining `space` where there was none.
    fn create_joined(&self, sealed: Sealed, space: SyncLocal, remote: Option<Arc<dyn RemoteStore>>) -> CoreResult<()> {
        {
            let mut st = self.lock();
            if !matches!(st.phase, PhaseState::NoVault) {
                return Err(ErrorCode::VaultExists.into());
            }
            let cache = cloud_cache(&space, remote);
            let mut data = VaultData::new();
            data.local_mut().sync = Some(space);
            let session = Session { sealed, data, import: None, exports: Vec::new() };
            self.write_vault(&session)?;
            st.vault_id = Some(session.sealed.vault_id());
            st.device_slot = false;
            st.phase = PhaseState::Unlocked(Box::new(session));
            st.last_activity = Instant::now();
            st.sync.cloud.remote = cache;
            sync_due(&mut st, Instant::now(), true);
        }
        self.changed();
        Ok(())
    }

    /// The invitation for another device, once the user proved to be at this one (the master
    /// password, or without it the biometric check that unlocks this vault): the storage, its
    /// credentials and the sync key, as the QR code's text, and sealed under a one-time code for
    /// sending.
    pub async fn sync_invite(&self, password: Option<Zeroizing<String>>, reason: Option<String>) -> CoreResult<SyncInvite> {
        let (storage, sync_key) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            let sync = session.data.sync().ok_or(ErrorCode::SyncOff)?;
            (sync.cloud.as_ref().ok_or(ErrorCode::SyncNoStorage)?.storage.clone(), sync.sync_key.clone())
        };
        self.confirm_presence(password, reason).await?;
        let cost = self.shared.config.kdf;
        blocking(move || {
            let sync_key = SyncKey::from_text(&sync_key).map_err(|e| sync_error(&e))?;
            let invite = Invite { storage, sync_key: sync_key.clone() };
            let text = invite.to_text();
            let svg = qr::svg(&text).map_err(|_| ErrorCode::Internal)?;
            let (shared, code) = invite.to_shared_text(cost).map_err(|e| sync_error(&e))?;
            Ok(SyncInvite {
                invite: text.to_string(),
                svg: svg.to_string(),
                shared_text: shared.to_string(),
                code: code.to_string(),
                sync_key: sync_key.to_text().to_string(),
            })
        })
        .await
    }

    /// The user is at this device: `password` is the vault's master password, or, without one,
    /// the biometric check passes, where it is what unlocks this vault (Touch ID, Windows Hello,
    /// the fingerprint). It proves presence only: nothing is sealed under it.
    async fn confirm_presence(&self, password: Option<Zeroizing<String>>, reason: Option<String>) -> CoreResult<()> {
        if let Some(password) = password {
            let sealed = unlocked(&self.lock())?.sealed.clone();
            return blocking(move || Ok(sealed.verify_password(password.as_bytes())?)).await;
        }
        let biometric = {
            let st = self.lock();
            let session = unlocked(&st)?;
            st.biometric.is_some() && st.device_slot && session.sealed.device_check() == Some(&DeviceCheck::Biometric)
        };
        if !biometric {
            return Err(ErrorCode::BiometricUnavailable.into());
        }
        self.check_user(reason).await
    }

    /// The user saved or wrote down the sync key: Settings › Sync stops reminding of it.
    pub fn sync_key_acknowledge(&self) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let sync = session.data.sync_mut().ok_or(ErrorCode::SyncOff)?;
            if sync.key_saved {
                return Ok(());
            }
            sync.key_saved = true;
            self.save(&mut st, false, |s| {
                if let Some(sync) = s.data.sync_mut() {
                    sync.key_saved = false;
                }
            })?;
        }
        self.changed();
        Ok(())
    }

    /// The text of a sync key file, once the user proved to be at this device: `template` (the
    /// interface's words, at most 8 KiB) with its one `{{sync_key}}` replaced by the key. The
    /// shell writes it where the user chose (`sync_key_save`) and then calls
    /// [`Self::sync_key_acknowledge`].
    pub async fn sync_key_file(&self, password: Option<Zeroizing<String>>, reason: Option<String>, template: &str) -> CoreResult<Zeroizing<String>> {
        const SLOT: &str = "{{sync_key}}";
        if template.len() > 8 * 1024 || template.matches(SLOT).count() != 1 {
            return Err(ErrorCode::Internal.into());
        }
        let sync_key = {
            let st = self.lock();
            unlocked(&st)?.data.sync().ok_or(ErrorCode::SyncOff)?.sync_key.clone()
        };
        self.confirm_presence(password, reason).await?;
        Ok(Zeroizing::new(template.replacen(SLOT, &sync_key, 1)))
    }

    /// Write the sync key file to `path` (the desktop's save dialog chose it), readable by its
    /// owner only, and stop the reminder.
    pub async fn sync_key_save(&self, password: Option<Zeroizing<String>>, reason: Option<String>, template: &str, path: PathBuf) -> CoreResult<()> {
        let text = self.sync_key_file(password, reason, template).await?;
        blocking(move || Ok(write_private(&path, text.as_bytes())?)).await?;
        self.sync_key_acknowledge()
    }

    /// Move the space's storage settings on (a new address or new credentials), after the master
    /// password was entered again; the space must be found there.
    pub async fn sync_set_storage(&self, storage: StorageConfig, password: Zeroizing<String>) -> CoreResult<()> {
        storage.validate().map_err(config_error)?;
        let (sealed, keys, device) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            let sync = session.data.sync().ok_or(ErrorCode::SyncOff)?;
            (session.sealed.clone(), sync.keys().map_err(|e| sync_error(&e))?, session.data.device())
        };
        let space_id = keys.space_id();
        blocking(move || Ok(sealed.verify_password(password.as_bytes())?)).await?;
        let remote = self.open_storage(&storage)?;
        // This space, not just objects at its place: one of its snapshots opens under its data key.
        find_space(&*remote, storage.prefix(), &keys, device).await.map_err(|e| sync_error(&e))?;
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let sync = session.data.sync_mut().filter(|s| s.space_id == space_id).ok_or(ErrorCode::SyncOff)?;
            let cloud = sync.cloud.as_mut().ok_or(ErrorCode::SyncNoStorage)?;
            let cache = (space_id, StoreKey::Cloud(storage.clone()), remote);
            let previous = std::mem::replace(&mut cloud.storage, storage);
            self.save(&mut st, false, move |s| {
                if let Some(cloud) = s.data.sync_mut().and_then(|sync| sync.cloud.as_mut()) {
                    cloud.storage = previous;
                }
            })?;
            let cloud = &mut st.sync.cloud;
            cloud.remote = Some(cache);
            cloud.held = false;
            cloud.at = Some(Instant::now());
        }
        self.changed();
        Ok(())
    }

    /// Rename this device in its space; the other devices see the name after the next run.
    pub fn sync_rename_device(&self, name: &str) -> CoreResult<()> {
        let name = device_name(name, self.shared.config.platform);
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let sync = session.data.sync_mut().ok_or(ErrorCode::SyncOff)?;
            let previous = std::mem::replace(&mut sync.device_name, name);
            self.save(&mut st, false, move |s| {
                if let Some(sync) = s.data.sync_mut() {
                    sync.device_name = previous;
                }
            })?;
            sync_due(&mut st, Instant::now(), false);
        }
        self.changed();
        Ok(())
    }

    /// Remove another device's snapshot from the space (a lost or retired device): it stops being
    /// listed. On the LAN hub, from its copy too. A device still in use writes its snapshot again
    /// on its next run.
    pub async fn sync_remove_device(&self, tag: &str) -> CoreResult<()> {
        let (space_id, stores, keys, device) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            let sync = session.data.sync().ok_or(ErrorCode::SyncOff)?;
            // A client may not delete on its hub: the hub removes its devices itself.
            let hub = matches!(sync.lan, Some(LanLocal::Hub { .. }));
            let stores: Vec<(Store, StoreKey)> =
                sync.stores().filter(|store| *store == Store::Cloud || hub).filter_map(|store| sync.store_key(store).map(|key| (store, key))).collect();
            (sync.space_id, stores, sync.keys().map_err(|e| sync_error(&e))?, session.data.device())
        };
        for (store, key) in &stores {
            let remote = self.space_storage(*store, space_id, key).map_err(|e| sync_error(&e))?;
            let space = Space { prefix: key.prefix(), keys: &keys, device, device_name: "", keyring: &[], seq_floor: 0 };
            match remove_device(&*remote, &space, &mut SyncState::default(), tag).await {
                Ok(()) => {}
                // This device, or not a device's name: nothing the interface offers.
                Err(SyncError::Misplaced) => return Err(ErrorCode::Internal.into()),
                Err(error) => return Err(sync_error(&error)),
            }
        }
        {
            let mut st = self.lock();
            if let PhaseState::Unlocked(session) = &mut st.phase
                && let Some(sync) = session.data.sync_mut().filter(|s| s.space_id == space_id)
            {
                let previous = sync.clone();
                let mut forgot = false;
                for store in Store::ALL {
                    if let Some(transport) = sync.transport_mut(store) {
                        forgot |= transport.state.seen.remove(tag).is_some();
                    }
                }
                if forgot {
                    self.save(&mut st, false, move |s| s.data.local_mut().sync = Some(previous))?;
                }
            }
            for store in Store::ALL {
                let runtime = st.sync.store_mut(store);
                runtime.rolled_back.retain(|t| t != tag);
                runtime.unreadable.retain(|t| t != tag);
            }
            sync_due(&mut st, Instant::now(), false);
        }
        self.changed();
        Ok(())
    }

    /// The app came to the front or went behind other windows (the desktop's window focus; a
    /// phone app is in front while it runs). In front, a run follows the last one after a minute
    /// rather than five; coming back makes one due as soon as the last is half a minute old.
    pub fn set_foreground(&self, foreground: bool) {
        {
            let mut st = self.lock();
            if std::mem::replace(&mut st.foreground, foreground) == foreground || !foreground || st.sync.busy {
                return;
            }
            let now = Instant::now();
            for store in configured_stores(&st) {
                let runtime = st.sync.store_mut(store);
                let due = runtime.last_run.map_or(now, |last| (last + SYNC_FOCUS_MIN).max(now));
                if !runtime.held && runtime.at.is_none_or(|at| at > due) {
                    runtime.at = Some(due);
                }
            }
        }
        self.shared.wake.notify_one();
    }

    /// Run the sync now (Settings › Sync › "sync now").
    pub fn sync_now(&self) -> CoreResult<()> {
        {
            let mut st = self.lock();
            unlocked(&st)?.data.sync().ok_or(ErrorCode::SyncOff)?;
            for store in configured_stores(&st) {
                let runtime = st.sync.store_mut(store);
                runtime.at = None;
                runtime.held = false;
                runtime.queued = true;
            }
        }
        self.start_sync();
        Ok(())
    }

    /// Turn sync off on this device: the space is forgotten here (the storage keeps it, and the
    /// other devices go on).
    pub fn sync_disable(&self) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let previous = session.data.local_mut().sync.take().ok_or(ErrorCode::SyncOff)?;
            self.save(&mut st, false, move |s| s.data.local_mut().sync = Some(previous))?;
            st.sync.reset();
        }
        self.serve_lan();
        self.changed();
        Ok(())
    }

    fn open_storage(&self, storage: &StorageConfig) -> CoreResult<Arc<dyn RemoteStore>> {
        self.shared.ports.sync.open(storage).map_err(|e| sync_error(&e))
    }

    /// Start a run of the storages that are due, unless one is in progress: that one takes them
    /// before it ends.
    fn start_sync(&self) {
        {
            let mut st = self.lock();
            let due = configured_stores(&st).into_iter().any(|store| st.sync.store(store).queued);
            if !due || st.sync.busy {
                return;
            }
            st.sync.busy = true;
        }
        let core = self.clone();
        tokio::spawn(async move { core.run_sync().await });
    }

    async fn run_sync(&self) {
        let mut renumbered = false;
        loop {
            let stores: Vec<Store> = {
                let mut st = self.lock();
                let mut stores = configured_stores(&st);
                stores.retain(|store| std::mem::take(&mut st.sync.store_mut(*store).queued));
                if stores.is_empty() {
                    st.sync.busy = false;
                    return;
                }
                for store in &stores {
                    st.sync.store_mut(*store).status = SyncStatus::Syncing;
                }
                stores
            };
            self.changed();
            let (mut brought, mut devices, mut unknown) = (Brought::default(), Vec::<String>::new(), false);
            for store in stores {
                let (taken, result) = self.sync_once(store).await;
                brought += taken;
                match &result {
                    Ok(outcome) => {
                        for device in &outcome.brought {
                            if !devices.contains(device) {
                                devices.push(device.clone());
                            }
                        }
                    }
                    // In the vault whatever the run did next; a run that failed later no longer
                    // knows which devices it came from.
                    Err(_) => unknown |= taken.any(),
                }
                let clash = {
                    let mut st = self.lock();
                    match &result {
                        // Another device writes under this one's name: a copied vault, or this
                        // vault's own state put back. This device becomes a new one of the space,
                        // once, on every storage.
                        Err(SyncError::DeviceClash) if !renumbered => {
                            renumbered = true;
                            match self.renumber(&mut st) {
                                Ok(()) => {
                                    for again in configured_stores(&st) {
                                        st.sync.store_mut(again).queued = true;
                                    }
                                    true
                                }
                                Err(error) => {
                                    self.after_run(&mut st, store, &Err(SyncError::Storage(String::new())));
                                    st.sync.store_mut(store).status = SyncStatus::Failed { code: error.code, at_ms: self.now_ms() };
                                    false
                                }
                            }
                        }
                        _ => {
                            self.after_run(&mut st, store, &result);
                            false
                        }
                    }
                };
                self.changed();
                if clash {
                    break;
                }
            }
            if brought.any() {
                self.notice(brought.notice(if unknown { Vec::new() } else { devices }));
            }
        }
    }

    /// A storage's run ended with `result`: what it did, and when it runs next. A failed run
    /// waits the long interval, in front too; the LAN is tried again sooner, being away from the
    /// hub being no failure; a storage holding a newer format, or a hub that no longer knows this
    /// device, waits for the user.
    fn after_run(&self, st: &mut State, store: Store, result: &Result<SyncOutcome, SyncError>) {
        let now_ms = self.now_ms();
        let foreground = st.foreground;
        let configured = configured_stores(st).contains(&store);
        let runtime = st.sync.store_mut(store);
        runtime.last_run = Some(Instant::now());
        let next = match result {
            Ok(outcome) => {
                runtime.status = SyncStatus::Synced { at_ms: now_ms };
                runtime.offline = false;
                runtime.rolled_back.clone_from(&outcome.rolled_back);
                runtime.unreadable.clone_from(&outcome.unreadable);
                Some(match (store, foreground) {
                    (_, false) => SYNC_INTERVAL,
                    (Store::Lan, true) => SYNC_INTERVAL_LAN_FOREGROUND,
                    (Store::Cloud, true) => SYNC_INTERVAL_FOREGROUND,
                })
            }
            // Locked meanwhile, the space or this storage changed, or the vault could not be
            // written (the status says so already).
            Err(SyncError::Interrupted) => Some(SYNC_INTERVAL),
            Err(error) => {
                runtime.status = SyncStatus::Failed { code: store_error(store, error), at_ms: now_ms };
                runtime.offline = store == Store::Lan && matches!(error, SyncError::Network(_));
                match (store, error) {
                    (Store::Lan, SyncError::Network(_)) if foreground => Some(SYNC_INTERVAL_FOREGROUND),
                    (_, SyncError::Unsupported(_)) | (Store::Lan, SyncError::WrongCredentials) => None,
                    _ => Some(SYNC_INTERVAL),
                }
            }
        };
        if !configured || runtime.queued || runtime.at.is_some() {
            return;
        }
        match next {
            Some(interval) => runtime.at = Some(Instant::now() + interval),
            None => runtime.held = true,
        }
    }

    /// One run on `store`: the other devices' snapshots in, this device's out, its keyring
    /// inside. With what it took into the accounts, which stays there even when the run fails
    /// afterwards.
    async fn sync_once(&self, store: Store) -> (Brought, Result<SyncOutcome, SyncError>) {
        let mut brought = Brought::default();
        let result = self.sync_into(store, &mut brought).await;
        (brought, result)
    }

    async fn sync_into(&self, store: Store, brought: &mut Brought) -> Result<SyncOutcome, SyncError> {
        let SyncJob { space_id, remote, key, keys, device, device_name, keyring, seq_floor, mut state, mut working } = self.sync_job(store)?;
        let remote = remote.ok_or(SyncError::Interrupted)?;
        let space = Space { prefix: key.prefix(), keys: &keys, device, device_name: &device_name, keyring: &keyring, seq_floor };
        let outcome = {
            let mut persist = |state: &SyncState, working: &Working| {
                *brought += self.sync_keep(store, space_id, &key, state, working, None)?;
                Ok(())
            };
            step_with(&*remote, &space, &mut state, &mut working, self.now_ms(), &mut persist).await?
        };
        *brought += self.sync_keep(store, space_id, &key, &state, &working, Some((self.now_ms(), &keyring)))?;
        Ok(outcome)
    }

    fn sync_job(&self, store: Store) -> Result<SyncJob, SyncError> {
        let job = {
            let st = self.lock();
            let PhaseState::Unlocked(session) = &st.phase else { return Err(SyncError::Interrupted) };
            let Some(sync) = session.data.sync() else { return Err(SyncError::Interrupted) };
            let (Some(transport), Some(key)) = (sync.transport(store), sync.store_key(store)) else { return Err(SyncError::Interrupted) };
            SyncJob {
                space_id: sync.space_id,
                remote: None,
                key,
                keys: sync.keys()?,
                device: session.data.device(),
                device_name: sync.device_name.clone(),
                keyring: sync.keyring()?,
                seq_floor: sync.seq_floor(),
                state: transport.state.clone(),
                working: Working { entries: session.data.entries.clone(), tombstones: session.data.tombstones.clone() },
            }
        };
        let remote = self.space_storage(store, job.space_id, &job.key)?;
        Ok(SyncJob { remote: Some(remote), ..job })
    }

    /// The space's storage: opened once and kept for the next runs, until the space or the
    /// storage's settings change. Opening (an HTTP client, the system's certificates) happens
    /// outside the state's lock.
    fn space_storage(&self, store: Store, space_id: Uuid, key: &StoreKey) -> Result<Arc<dyn RemoteStore>, SyncError> {
        if let Some((id, kept, remote)) = &self.lock().sync.store(store).remote
            && *id == space_id
            && kept == key
        {
            return Ok(Arc::clone(remote));
        }
        let transport = &self.shared.ports.sync;
        let remote = match key {
            StoreKey::Cloud(storage) => transport.open(storage)?,
            StoreKey::Hub { .. } => transport.open_hub_store(space_id)?,
            StoreKey::Client(config) => transport.open_lan_client(config)?,
        };
        self.lock().sync.store_mut(store).remote = Some((space_id, key.clone(), Arc::clone(&remote)));
        Ok(remote)
    }

    /// Fold a run's replica into the vault and keep what the runs on `store` remember; `finished`
    /// marks the end of a run, with the keyring it carried. The accounts merge rather than
    /// replace: a change made while the run was out stays, and the next run takes it along. A
    /// space or a storage changed meanwhile stop the run, before it writes to where the space no
    /// longer is. Answers what the merge changed in the accounts.
    fn sync_keep(
        &self,
        store: Store,
        space_id: Uuid,
        key: &StoreKey,
        state: &SyncState,
        working: &Working,
        finished: Option<(u64, &[u8])>,
    ) -> Result<Brought, SyncError> {
        let now = self.now_ms();
        let (changed, brought) = {
            let mut st = self.lock();
            let PhaseState::Unlocked(session) = &mut st.phase else { return Err(SyncError::Interrupted) };
            let here = session.data.sync().filter(|s| s.space_id == space_id).and_then(|s| s.store_key(store));
            if !here.is_some_and(|here| here.same_store(key)) {
                return Err(SyncError::Interrupted);
            }
            let before = session.data.clone();
            let changed = merge_entries(&mut session.data.entries, &mut session.data.tombstones, &working.entries, &working.tombstones);
            let brought = if changed { Brought::between(&before.entries, &session.data.entries) } else { Brought::default() };
            if changed {
                session.data.observe_stamps();
            }
            if let Some(sync) = session.data.sync_mut() {
                // The storage holds the keyring the run carried: still this device's, unless a
                // new master password sealed another meanwhile.
                let current = sync.keyring().ok();
                if let Some(transport) = sync.transport_mut(store) {
                    transport.state = state.clone();
                    if let Some((at, keyring)) = finished {
                        transport.last_ok_ms = Some(at);
                        if current.as_deref() == Some(keyring) {
                            transport.keyring_written = true;
                        }
                    }
                }
            }
            if let Err(error) = self.write_vault(session) {
                session.data = before;
                st.sync.store_mut(store).status = SyncStatus::Failed { code: error.code, at_ms: now };
                return Err(SyncError::Interrupted);
            }
            if changed {
                self.schedule_auto_backup(&mut st);
            }
            (changed, brought)
        };
        if changed {
            self.changed();
        }
        Ok(brought)
    }

    /// Become a new device of the space: a new number (and clock), a fresh state on every
    /// storage. The snapshots under the old number stay, as another device's, until removed.
    fn renumber(&self, st: &mut State) -> CoreResult<()> {
        let PhaseState::Unlocked(session) = &mut st.phase else { return Err(ErrorCode::Locked.into()) };
        let before = session.data.clone();
        let local = session.data.local_mut();
        let mut clock = lockra_sync::Clock::new(random_device());
        clock.observe(local.clock.last());
        local.clock = clock;
        if let Some(sync) = local.sync.as_mut() {
            for store in Store::ALL {
                if let Some(transport) = sync.transport_mut(store) {
                    transport.state = SyncState::default();
                }
            }
        }
        if let Err(error) = self.write_vault(session) {
            session.data = before;
            return Err(error);
        }
        Ok(())
    }

    // ---- internals ------------------------------------------------------------------------

    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared.state.lock()
    }

    fn now_ms(&self) -> u64 {
        self.shared.ports.clock.now_ms()
    }

    fn vault_path(&self) -> PathBuf {
        self.shared.config.data_dir.join(VAULT_FILE)
    }

    fn ensure_unlocked(&self) -> CoreResult<()> {
        unlocked(&self.lock()).map(drop)
    }

    fn device_key(&self, vault_id: Uuid) -> CoreResult<DeviceKey> {
        let secrets = &self.shared.ports.secrets;
        if secrets.status() == KeychainStatus::Unavailable {
            return Err(ErrorCode::KeychainUnavailable.into());
        }
        let text = secrets.get(&vault_id.to_string()).map_err(|_| ErrorCode::KeychainFailed)?.ok_or(ErrorCode::DeviceKeyMissing)?;
        DeviceKey::from_text(&text).map_err(|_| ErrorCode::DeviceKeyStale.into())
    }

    fn write_vault(&self, session: &Session) -> CoreResult<()> {
        let bytes = session.sealed.seal(FileKind::Vault, &session.data.to_bytes())?;
        write_atomic(&self.vault_path(), &bytes)?;
        Ok(())
    }

    /// Write the vault after a change; on failure `undo` puts the session back as it was. A
    /// content change schedules the automatic backup.
    fn save(&self, st: &mut State, content: bool, undo: impl FnOnce(&mut Session)) -> CoreResult<()> {
        let PhaseState::Unlocked(session) = &mut st.phase else { return Err(ErrorCode::Locked.into()) };
        if let Err(error) = self.write_vault(session) {
            undo(session);
            return Err(error);
        }
        st.last_activity = Instant::now();
        if content {
            self.schedule_auto_backup(st);
            schedule_sync(st);
        }
        Ok(())
    }

    fn schedule_auto_backup(&self, st: &mut State) {
        if st.settings.auto_backup.enabled {
            st.auto_backup_at = Some(Instant::now() + AUTO_BACKUP_DEBOUNCE);
        }
    }

    fn run_auto_backup(&self) -> CoreResult<()> {
        let now = self.now_ms();
        let result = {
            let mut st = self.lock();
            st.auto_backup_at = None;
            let keep = st.settings.auto_backup.keep as usize;
            let dir = st.settings.auto_backup.dir.clone().map(PathBuf::from);
            match (&st.phase, dir) {
                (PhaseState::Unlocked(session), Some(dir)) => {
                    let attempt = (|| -> CoreResult<String> {
                        if !dir.is_dir() {
                            return Err(ErrorCode::BackupDirUnavailable.into());
                        }
                        let bytes = session.sealed.seal(FileKind::Backup, &session.data.backup_bytes())?;
                        let name = auto_file_name(now, |n| dir.join(n).exists());
                        write_private(&dir.join(&name), &bytes).map_err(|_| ErrorCode::BackupDirUnavailable)?;
                        prune(&dir, keep).map_err(|_| ErrorCode::BackupDirUnavailable)?;
                        Ok(name)
                    })();
                    match &attempt {
                        Ok(name) => {
                            st.last_backup_ms = Some(now);
                            st.last_auto_file = Some(name.clone());
                            st.last_auto_error = None;
                        }
                        Err(error) => st.last_auto_error = Some(BackupFailure { code: error.code, at_ms: now }),
                    }
                    attempt
                }
                (PhaseState::Unlocked(_), None) => Err(ErrorCode::BackupDirMissing.into()),
                _ => Err(ErrorCode::Locked.into()),
            }
        };
        match &result {
            Ok(name) => self.notice(Notice::BackupWritten { file_name: name.clone(), automatic: true }),
            Err(error) => self.notice(Notice::BackupFailed { code: error.code }),
        }
        self.changed();
        result.map(drop)
    }

    fn view(&self, st: &State) -> UiState {
        let now = self.now_ms();
        let (phase, entries, collapsed_groups, import) = match &st.phase {
            PhaseState::NoVault => (Phase::NoVault, Vec::new(), Vec::new(), None),
            PhaseState::Locked => (Phase::Locked, Vec::new(), Vec::new(), None),
            PhaseState::Unlocked(session) => (
                Phase::Unlocked,
                session.data.entries.iter().map(Entry::view).collect(),
                session.data.collapsed_groups().to_vec(),
                session.import.as_ref().map(|i| i.view(&session.data)),
            ),
        };
        let auto_lock_at_ms = match (&st.phase, st.settings.auto_lock_minutes) {
            (PhaseState::Unlocked(_), minutes) if minutes > 0 => {
                let deadline = st.last_activity + Duration::from_secs(u64::from(minutes) * 60);
                Some(now + u64::try_from(deadline.saturating_duration_since(Instant::now()).as_millis()).unwrap_or(0))
            }
            _ => None,
        };
        UiState {
            app_version: self.shared.config.app_version.clone(),
            platform: self.shared.config.platform,
            phase,
            data_dir: self.shared.config.data_dir.display().to_string(),
            lock: LockView {
                device_unlock: DeviceUnlockView {
                    available: self.shared.ports.secrets.status() == KeychainStatus::Available,
                    enabled: st.device_slot,
                    biometric: BiometricView {
                        kind: st.biometric,
                        enabled: st.device_slot
                            && match &st.phase {
                                PhaseState::Unlocked(session) => session.sealed.device_check() == Some(&DeviceCheck::Biometric),
                                _ => st.device_check == Some(DeviceCheck::Biometric),
                            },
                    },
                },
                failed_attempts: st.failed_attempts,
                retry_at_ms: st.retry_at_ms.filter(|at| *at > now),
            },
            entries,
            collapsed_groups,
            settings: st.settings.clone(),
            import,
            backup: BackupView { last_backup_ms: st.last_backup_ms, last_auto_file: st.last_auto_file.clone(), last_auto_error: st.last_auto_error },
            restore: st.restore.as_ref().map(|r| r.view.clone()),
            auto_lock_at_ms,
            update: UpdateView { method: self.shared.ports.updater.method(), status: st.update.status.clone() },
            sync: sync_view(st),
        }
    }

    /// Tell the webview and the scheduler that something changed.
    fn changed(&self) {
        let state = self.state();
        let _ = self.shared.events.send(UiEvent::State { state: Box::new(state) });
        self.push_codes();
        self.shared.wake.notify_one();
    }

    fn notice(&self, notice: Notice) {
        let _ = self.shared.events.send(UiEvent::Notice { notice });
    }

    fn push_codes(&self) {
        let (sink, frame) = {
            let mut st = self.lock();
            let Some(sink) = st.codes.clone() else { return };
            let frame = self.frame(&st);
            st.codes_until = frame.codes.iter().filter_map(|c| c.valid_until_ms).min();
            (sink, frame)
        };
        if sink.send(&frame).is_err() {
            let mut st = self.lock();
            if st.codes.as_ref().is_some_and(|s| Arc::ptr_eq(s, &sink)) {
                st.codes = None;
            }
        }
    }

    fn frame(&self, st: &State) -> CodesFrame {
        let now = self.now_ms();
        let codes = match &st.phase {
            PhaseState::Unlocked(session) => session.data.entries.iter().map(|e| code_view(e, now)).collect(),
            _ => Vec::new(),
        };
        CodesFrame { at_ms: now, codes }
    }

    /// The earliest moment the scheduler has something to do.
    fn next_deadline(&self) -> Option<Instant> {
        let st = self.lock();
        let mut deadlines: Vec<Instant> = Vec::new();
        if let PhaseState::Unlocked(session) = &st.phase {
            if st.settings.auto_lock_minutes > 0 {
                deadlines.push(st.last_activity + Duration::from_secs(u64::from(st.settings.auto_lock_minutes) * 60));
            }
            if st.codes.is_some() {
                let now = self.now_ms();
                if st.codes_until.is_some_and(|until| until <= now) {
                    // A window ended before the scheduler looked (it was busy): the frame is due now.
                    deadlines.push(Instant::now());
                }
                let boundary = session
                    .data
                    .entries
                    .iter()
                    .filter_map(|e| match e.kind {
                        OtpKind::Totp { period } => Some(totp_window(now, period).valid_until_ms),
                        OtpKind::Hotp { .. } => None,
                    })
                    .min();
                if let Some(boundary) = boundary {
                    deadlines.push(Instant::now() + Duration::from_millis(boundary.saturating_sub(now) + 1));
                }
            }
            deadlines.extend(session.exports.iter().map(|e| e.expires_at));
            deadlines.extend(st.auto_backup_at);
            deadlines.extend(st.sync.lan.at);
            deadlines.extend(st.sync.cloud.at);
            deadlines.extend(st.lan.offer.as_ref().map(|(_, until, _)| *until));
        }
        deadlines.extend(st.clipboard.as_ref().map(|(_, at)| *at));
        deadlines.extend(st.update.auto_at);
        deadlines.into_iter().min()
    }

    /// Everything that is due.
    fn on_timer(&self) {
        let now = Instant::now();
        let (auto_lock, clear, backup, expired, auto_update, sync) = {
            let mut st = self.lock();
            let auto_update = st.update.auto_at.is_some_and(|at| now >= at);
            if auto_update {
                st.update.auto_at = None;
            }
            let mut sync = false;
            for store in Store::ALL {
                let runtime = st.sync.store_mut(store);
                if runtime.at.is_some_and(|at| now >= at) {
                    runtime.at = None;
                    runtime.queued = true;
                    sync = true;
                }
            }
            let auto_lock = matches!(st.phase, PhaseState::Unlocked(_))
                && st.settings.auto_lock_minutes > 0
                && now >= st.last_activity + Duration::from_secs(u64::from(st.settings.auto_lock_minutes) * 60);
            let due = st.clipboard.as_ref().is_some_and(|(_, at)| now >= *at);
            let clear = if due { st.clipboard.take().map(|(code, _)| code) } else { None };
            let backup = st.auto_backup_at.is_some_and(|at| now >= at);
            let mut expired = Vec::new();
            if let PhaseState::Unlocked(session) = &mut st.phase {
                session.exports.retain(|e| {
                    let keep = e.expires_at > now;
                    if !keep {
                        expired.push(e.id);
                    }
                    keep
                });
            }
            (auto_lock, clear, backup, expired, auto_update, sync)
        };
        if let Some(code) = clear
            && self.shared.ports.clipboard.clear_if(&code).unwrap_or(false)
        {
            self.notice(Notice::ClipboardCleared);
        }
        for session in expired {
            self.notice(Notice::ExportExpired { session });
        }
        if backup {
            let _ = self.run_auto_backup();
        }
        if auto_lock {
            self.lock_vault();
            self.notice(Notice::AutoLocked);
        }
        if sync {
            self.start_sync();
        }
        self.lan_offer_lapsed(now);
        if auto_update {
            // Installs at once only the version an earlier start downloaded. Skipped while the user
            // runs one, which answers the same question.
            let install_version = read_marker(&self.shared.config.data_dir);
            let _ = self.start_update(UpdateRun::Auto { install_version });
        }
        self.push_codes();
    }
}

async fn scheduler(shared: Arc<Shared>) {
    let core = Core { shared };
    loop {
        let notified = core.shared.wake.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        match core.next_deadline() {
            Some(deadline) => {
                tokio::select! {
                    () = tokio::time::sleep_until(deadline) => core.on_timer(),
                    () = &mut notified => {}
                }
            }
            None => notified.await,
        }
    }
}

/// One source's findings, applied to the import session under the lock.
enum Batch {
    Items { source: ImportSource, origin: Origin, items: Vec<Item> },
    Google { source: ImportSource, batch: lockra_transfer::google::Batch, items: Vec<Item> },
}

impl Batch {
    fn apply(self, import: &mut ImportSession) {
        match self {
            Self::Items { source, origin, items } => import.add(&source, origin, items),
            Self::Google { source, batch, items } => import.add_google(&source, batch, items),
        }
    }

    /// The items name no line of their source.
    fn forget_lines(&mut self) {
        let (Self::Items { items, .. } | Self::Google { items, .. }) = self;
        for item in items {
            if let Item::Rejected { line, .. } = item {
                *line = None;
            }
        }
    }
}

/// A file picked on the phone: its name and what it holds.
pub struct PickedFile {
    /// The file name, without its directory.
    pub name: String,
    /// The file's bytes.
    pub bytes: Zeroizing<Vec<u8>>,
}

struct FoundFiles {
    batches: Vec<Batch>,
    awaiting: Option<AwaitingBackup>,
    notices: Vec<Notice>,
}

fn text_batches(source: &ImportSource, text: &str) -> Vec<Batch> {
    text::read(text)
        .into_iter()
        .map(|line| match line {
            text::Line::Uri(item) => Batch::Items { source: source.clone(), origin: Origin::Uri, items: vec![item] },
            text::Line::Google { batch, items } => Batch::Google { source: source.clone(), batch, items },
        })
        .collect()
}

fn read_import_files(paths: &[PathBuf]) -> FoundFiles {
    let mut notices = Vec::new();
    let mut read = Vec::new();
    for path in paths {
        let name = file_name(path);
        match read_limited(path) {
            Ok(bytes) => read.push((name, Zeroizing::new(bytes))),
            Err(_) => notices.push(Notice::FileUnreadable { name }),
        }
    }
    read_import_bytes(read, notices)
}

/// What the files hold, by what their bytes are; `notices` already says which could not be read.
fn read_import_bytes(read: Vec<(String, Zeroizing<Vec<u8>>)>, notices: Vec<Notice>) -> FoundFiles {
    let mut found = FoundFiles { batches: Vec::new(), awaiting: None, notices };
    let files: Vec<(String, Zeroizing<Vec<u8>>, detect::Kind)> = read
        .into_iter()
        .map(|(name, bytes)| {
            let kind = detect::detect(&bytes);
            (name, bytes, kind)
        })
        .collect();
    let wals: Vec<usize> = files.iter().enumerate().filter(|(_, f)| f.2 == detect::Kind::SqliteWal).map(|(i, _)| i).collect();
    let mut used_wals: Vec<usize> = Vec::new();
    for (name, bytes, kind) in &files {
        let source = ImportSource::File { name: name.clone() };
        match kind {
            detect::Kind::Sqlite => {
                // The log named after the database (`PhoneFactor-wal`), else the only log picked.
                let wal = wals.iter().copied().find(|&w| files[w].0 == format!("{name}-wal")).or_else(|| (wals.len() == 1).then(|| wals[0]));
                if let Some(w) = wal {
                    used_wals.push(w);
                }
                match microsoft::read(bytes, wal.map(|w| files[w].1.as_slice())) {
                    Ok(items) => found.batches.push(Batch::Items { source, origin: Origin::Microsoft, items }),
                    Err(microsoft::PhoneFactorError::NotPhoneFactor) => found.notices.push(Notice::FileUnrecognized { name: name.clone() }),
                    Err(microsoft::PhoneFactorError::Unreadable) => found.notices.push(Notice::FileUnreadable { name: name.clone() }),
                }
            }
            detect::Kind::Image => match qr::decode_image(bytes) {
                Ok(texts) if !texts.is_empty() => found.batches.extend(texts.iter().flat_map(|t| text_batches(&source, t))),
                Ok(_) => found.notices.push(Notice::FileUnrecognized { name: name.clone() }),
                Err(_) => found.notices.push(Notice::FileUnreadable { name: name.clone() }),
            },
            detect::Kind::Text => match std::str::from_utf8(bytes) {
                Ok(text) => found.batches.extend(text_batches(&source, text.trim_start_matches('\u{feff}'))),
                Err(_) => found.notices.push(Notice::FileUnreadable { name: name.clone() }),
            },
            detect::Kind::Lockra => {
                if found.awaiting.is_none() {
                    found.awaiting = Some(AwaitingBackup { name: name.clone(), bytes: bytes.clone() });
                }
            }
            detect::Kind::SqliteWal => {}
            detect::Kind::Unknown => found.notices.push(Notice::FileUnrecognized { name: name.clone() }),
        }
    }
    for w in wals {
        if !used_wals.contains(&w) {
            found.notices.push(Notice::FileUnrecognized { name: files[w].0.clone() });
        }
    }
    found
}

fn code_view(entry: &Entry, now_ms: u64) -> CodeView {
    match entry.kind {
        OtpKind::Totp { period } => {
            let window = totp_window(now_ms, period);
            CodeView {
                entry_id: entry.id,
                code: hotp(&entry.secret, window.counter, entry.algorithm, entry.digits),
                next_code: Some(hotp(&entry.secret, window.counter + 1, entry.algorithm, entry.digits)),
                valid_from_ms: Some(window.valid_from_ms),
                valid_until_ms: Some(window.valid_until_ms),
            }
        }
        OtpKind::Hotp { counter } => CodeView {
            entry_id: entry.id,
            code: hotp(&entry.secret, counter, entry.algorithm, entry.digits),
            next_code: None,
            valid_from_ms: None,
            valid_until_ms: None,
        },
    }
}

fn current_code(entry: &Entry, now_ms: u64) -> String {
    code_view(entry, now_ms).code
}

/// The cloud storage of `space` opened as `remote`, for the runtime to keep.
fn cloud_cache(space: &SyncLocal, remote: Option<Arc<dyn RemoteStore>>) -> Option<(Uuid, StoreKey, Arc<dyn RemoteStore>)> {
    Some((space.space_id, space.store_key(Store::Cloud)?, remote?))
}

/// The storages of the unlocked vault's space, in the order a run takes them.
fn configured_stores(st: &State) -> Vec<Store> {
    match &st.phase {
        PhaseState::Unlocked(session) => session.data.sync().map(|sync| sync.stores().collect()).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The space's storages are due at `at` (unlocking, a command); the ones waiting for the user
/// too when `held`, the user having asked.
fn sync_due(st: &mut State, at: Instant, held: bool) {
    for store in configured_stores(st) {
        let runtime = st.sync.store_mut(store);
        if held {
            runtime.held = false;
        }
        if !runtime.held && runtime.at.is_none_or(|due| due > at) {
            runtime.at = Some(at);
        }
    }
}

/// A change to sync: a run follows after the debounce.
fn schedule_sync(st: &mut State) {
    let at = Instant::now() + SYNC_DEBOUNCE;
    for store in configured_stores(st) {
        let runtime = st.sync.store_mut(store);
        if !runtime.held {
            runtime.at = Some(at);
        }
    }
}

/// The code a storage's failure shows: the LAN hub's refusals are told apart from a storage's.
fn store_error(store: Store, error: &SyncError) -> ErrorCode {
    match (store, error) {
        (Store::Lan, SyncError::WrongCredentials) => ErrorCode::SyncLanUnpaired,
        (Store::Lan, SyncError::Denied) => ErrorCode::SyncLanRefused,
        _ => sync_error(error).code,
    }
}

/// A storage's status as its row shows it: out of reach of the LAN hub is offline, not failed.
fn store_status(runtime: &StoreRuntime) -> SyncStatus {
    match runtime.status {
        SyncStatus::Failed { at_ms, .. } if runtime.offline => SyncStatus::Offline { at_ms },
        ref status => status.clone(),
    }
}

/// The space's status from its storages': a run in progress; else the latest failure (being
/// away from the LAN hub is none); else the latest success; else offline when every storage is out
/// of reach; else nothing yet.
fn sync_status(st: &State, stores: &[Store]) -> SyncStatus {
    let runtimes: Vec<&StoreRuntime> = stores.iter().map(|store| st.sync.store(*store)).collect();
    if runtimes.iter().any(|r| r.status == SyncStatus::Syncing) {
        return SyncStatus::Syncing;
    }
    let failed = runtimes
        .iter()
        .filter(|r| !r.offline)
        .filter_map(|r| match r.status {
            SyncStatus::Failed { code, at_ms } => Some((at_ms, code)),
            _ => None,
        })
        .max_by_key(|(at_ms, _)| *at_ms);
    if let Some((at_ms, code)) = failed {
        return SyncStatus::Failed { code, at_ms };
    }
    let synced = runtimes
        .iter()
        .filter_map(|r| match r.status {
            SyncStatus::Synced { at_ms } => Some(at_ms),
            _ => None,
        })
        .max();
    if let Some(at_ms) = synced {
        return SyncStatus::Synced { at_ms };
    }
    let offline = runtimes
        .iter()
        .map(|r| match (r.offline, &r.status) {
            (true, SyncStatus::Failed { at_ms, .. }) => Some(*at_ms),
            _ => None,
        })
        .collect::<Option<Vec<u64>>>();
    match offline.and_then(|at| at.into_iter().max()) {
        Some(at_ms) => SyncStatus::Offline { at_ms },
        None => SyncStatus::Idle,
    }
}

fn sync_view(st: &State) -> SyncView {
    let joining = st.lan.joining.clone();
    let PhaseState::Unlocked(session) = &st.phase else { return SyncView { space: None, joining } };
    let space = session.data.sync().map(|sync| {
        let own_tag = sync.keys().map(|keys| keys.device_tag(session.data.device())).unwrap_or_default();
        let stores: Vec<Store> = sync.stores().collect();
        let (mut rolled_back, mut unreadable) = (Vec::<String>::new(), Vec::<String>::new());
        for store in &stores {
            let runtime = st.sync.store(*store);
            for tag in &runtime.rolled_back {
                if !rolled_back.contains(tag) {
                    rolled_back.push(tag.clone());
                }
            }
            for tag in &runtime.unreadable {
                if !unreadable.contains(tag) {
                    unreadable.push(tag.clone());
                }
            }
        }
        let transports = stores
            .iter()
            .map(|store| TransportView {
                kind: match store {
                    Store::Lan => TransportKind::Lan,
                    Store::Cloud => TransportKind::Cloud,
                },
                status: store_status(st.sync.store(*store)),
                last_ok_ms: sync.transport(*store).and_then(|t| t.last_ok_ms),
            })
            .collect();
        let lan = sync.lan.as_ref().map(|lan| match lan {
            LanLocal::Hub { port, peers, .. } => LanView::Hub {
                serving: st.lan.serving,
                port: st.lan.served.as_ref().map_or(*port, |served| served.port),
                peers: peers
                    .iter()
                    .map(|peer| LanPeerView { peer_id: peer.peer_id, name: peer.name.clone(), platform: peer.platform.clone(), tag: peer.tag.clone() })
                    .collect(),
                request: st.lan.request.clone(),
                offer_until_ms: st.lan.offer.as_ref().map(|(_, _, at_ms)| *at_ms),
            },
            LanLocal::Client { hub_name, .. } => LanView::Client { hub_name: hub_name.clone() },
        });
        SyncSpaceView {
            storage: sync.cloud.as_ref().map(|cloud| storage_view(&cloud.storage)),
            device_name: sync.device_name.clone(),
            devices: sync.devices(own_tag),
            status: sync_status(st, &stores),
            last_sync_ms: sync.last_sync_ms(),
            rolled_back,
            unreadable,
            keyring_pending: sync.keyring_pending(),
            key_saved: sync.key_saved,
            transports,
            lan,
        }
    });
    SyncView { space, joining }
}

/// This installation's id, kept beside the settings and nowhere in the vault or a backup: made
/// once. A vault opened by another installation leaves the LAN role taken here.
fn install_id(config_dir: &Path) -> Uuid {
    let path = config_dir.join(INSTALL_ID_FILE);
    if let Ok(text) = fs::read_to_string(&path)
        && let Ok(id) = Uuid::parse_str(text.trim())
    {
        return id;
    }
    let id = Uuid::new_v4();
    if let Err(error) = fs::create_dir_all(config_dir).and_then(|()| write_atomic(&path, id.to_string().as_bytes())) {
        tracing::warn!(?error, "the installation id was not kept: a LAN pairing lasts until the app quits");
    }
    id
}

fn unlocked(st: &State) -> CoreResult<&Session> {
    match &st.phase {
        PhaseState::Unlocked(session) => Ok(session),
        PhaseState::Locked => Err(ErrorCode::Locked.into()),
        PhaseState::NoVault => Err(ErrorCode::NoVault.into()),
    }
}

/// What the tests need before the LAN has commands of its own.
#[cfg(test)]
impl Core {
    /// This installation.
    pub(crate) fn install_id(&self) -> Uuid {
        self.shared.install_id
    }

    /// The space as the vault keeps it.
    pub(crate) fn sync_local(&self) -> Option<SyncLocal> {
        unlocked(&self.lock()).ok().and_then(|session| session.data.sync().cloned())
    }
}

fn unlocked_mut(st: &mut State) -> CoreResult<&mut Session> {
    match &mut st.phase {
        PhaseState::Unlocked(session) => Ok(session),
        PhaseState::Locked => Err(ErrorCode::Locked.into()),
        PhaseState::NoVault => Err(ErrorCode::NoVault.into()),
    }
}

fn session_device(st: &State) -> bool {
    matches!(&st.phase, PhaseState::Unlocked(session) if session.sealed.has_device_slot())
}

fn check_password(password: &str) -> CoreResult<()> {
    if password.chars().count() < MIN_PASSWORD_CHARS { Err(ErrorCode::PasswordTooShort.into()) } else { Ok(()) }
}

fn retry_delay_ms(failed: u32) -> u64 {
    if failed < FREE_ATTEMPTS { 0 } else { (1000u64 << (failed - FREE_ATTEMPTS).min(5)).min(MAX_RETRY_DELAY_MS) }
}

fn uri_error(error: &uri::UriError) -> CoreError {
    CoreError::from(match error {
        uri::UriError::InvalidSecret(_) | uri::UriError::MissingSecret => ErrorCode::InvalidSecret,
        uri::UriError::InvalidDigits(_) | uri::UriError::InvalidPeriod(_) | uri::UriError::InvalidCounter(_) | uri::UriError::UnsupportedAlgorithm(_) => {
            ErrorCode::InvalidParameters
        }
        uri::UriError::NotOtpauth | uri::UriError::UnknownType(_) => ErrorCode::InvalidUri,
    })
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> CoreResult<T> + Send + 'static) -> CoreResult<T> {
    tokio::task::spawn_blocking(work).await.map_err(|_| CoreError::from(ErrorCode::Internal))?
}

fn read_limited(path: &Path) -> CoreResult<Vec<u8>> {
    let size = fs::metadata(path)?.len();
    if size > MAX_IMPORT_BYTES {
        return Err(ErrorCode::ImportUnreadable.into());
    }
    Ok(fs::read(path)?)
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Write a file readable by its owner only (an export, a backup): no `.tmp`/`.prev` siblings,
/// since a plaintext export must not leave a second copy behind.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn probe_writable(dir: &Path) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Err(std::io::Error::other("not a directory"));
    }
    let probe = dir.join(format!(".lockra-probe-{}", Uuid::new_v4()));
    write_private(&probe, b"")?;
    fs::remove_file(probe)
}

#[cfg(unix)]
fn restrict_dir(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_dir(_dir: &Path) {}
