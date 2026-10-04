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
use crate::sync::{SYNC_DEBOUNCE, SYNC_INTERVAL, SyncLocal, Working, config_error, device_name, merge_entries, storage_view, sync_error};
use crate::ui::{
    BackupFailure, BackupView, BiometricKind, BiometricView, CodeView, CodesFrame, DeviceUnlockView, ExportPage, ExportStarted, ExportTarget, ImportSource,
    InstallMethod, JoinSource, LockView, Notice, Phase, Platform, RestoreView, Revealed, SyncCreated, SyncInvite, SyncSpaceView, SyncStatus, SyncView, UiEvent,
    UiState, UpdateStatus, UpdateView,
};
use crate::update::{Pending, ProgressGate, UpdateRun, UpdateState, clear_marker, failure_code, read_marker, write_marker};

/// The vault's file name in the data directory.
pub const VAULT_FILE: &str = "vault.lockra";
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
}

/// The sync as it runs; the vault keeps the space itself.
#[derive(Default)]
struct SyncRuntime {
    /// A run is in progress.
    busy: bool,
    /// A run was asked for while one was in progress: another follows it.
    again: bool,
    /// When the next run is due.
    at: Option<Instant>,
    /// What the last run did.
    status: SyncStatus,
    /// The last run's refused rollbacks and unreadable snapshots (device tags).
    rolled_back: Vec<String>,
    unreadable: Vec<String>,
    /// The storage, opened once per space and storage settings.
    remote: Option<(Uuid, StorageConfig, Arc<dyn RemoteStore>)>,
}

/// What one run needs, taken from the vault when it starts.
struct SyncJob {
    space_id: Uuid,
    remote: Option<Arc<dyn RemoteStore>>,
    storage: StorageConfig,
    keys: SpaceKeys,
    device: u64,
    device_name: String,
    keyring: Vec<u8>,
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
        };
        let (events, _) = broadcast::channel(64);
        let shared = Arc::new(Shared { config, ports, settings_store, state: Mutex::new(state), events, wake: Notify::new() });
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
            // A device of a sync space syncs as soon as it is unlocked.
            if data.sync().is_some() {
                st.sync.at = Some(Instant::now());
            }
            st.phase = PhaseState::Unlocked(Box::new(Session { sealed: opened.sealed, data, import: None, exports: Vec::new() }));
        }
        self.changed();
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
                st.sync = SyncRuntime { busy: st.sync.busy, ..SyncRuntime::default() };
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
            let previous_keyring = match (&keyring, session.data.sync_mut()) {
                (Some((space_id, keyring)), Some(sync)) if sync.space_id == *space_id => {
                    Some((std::mem::replace(&mut sync.keyring, BASE64.encode(keyring)), std::mem::replace(&mut sync.keyring_written, false)))
                }
                _ => None,
            };
            if let Err(error) = self.write_vault(session) {
                session.sealed = previous;
                if let (Some((keyring, written)), Some(sync)) = (previous_keyring, session.data.sync_mut()) {
                    sync.keyring = keyring;
                    sync.keyring_written = written;
                }
                return Err(error);
            }
            st.device_slot = session_device(&st);
            self.schedule_auto_backup(&mut st);
            if keyring.is_some() {
                st.sync.at = Some(Instant::now());
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
                    let session = Session { sealed: opened.sealed, data, import: None, exports: Vec::new() };
                    if let Err(error) = self.write_vault(&session) {
                        st.restore = Some(restore);
                        return Err(error);
                    }
                    let entries = u32::try_from(session.data.entries.len()).unwrap_or(u32::MAX);
                    st.vault_id = Some(session.sealed.vault_id());
                    st.device_slot = false;
                    // A vault file (not a backup) brings its sync space along.
                    if session.data.sync().is_some() {
                        st.sync.at = Some(Instant::now());
                    }
                    st.phase = PhaseState::Unlocked(Box::new(session));
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
        let mut space = SyncLocal::new(storage, &keys, &sync_key, device_name(&device, self.shared.config.platform), &keyring);
        // The key is shown once now; Settings › Sync reminds of it until it is saved.
        space.key_saved = false;
        let first = Space { prefix: space.storage.prefix(), keys: &keys, device: number, device_name: &space.device_name, keyring: &keyring };
        step(&*remote, &first, &mut space.state, &mut working, self.now_ms()).await.map_err(|e| sync_error(&e))?;
        space.keyring_written = true;
        space.last_sync_ms = Some(self.now_ms());
        let text = sync_key.to_text();
        self.join_space(vault_id, space, remote)?;
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
        let space = SyncLocal::new(storage, &keys, &sync_key, device_name(&device, self.shared.config.platform), &keyring);
        match (existing.as_ref().map(Sealed::vault_id), sealed) {
            (Some(vault_id), _) => self.join_space(vault_id, space, remote),
            (None, Some(sealed)) => self.create_joined(sealed, space, remote),
            (None, None) => Err(ErrorCode::Internal.into()),
        }
    }

    /// Record `space` in the unlocked vault `vault_id`; the first run follows.
    fn join_space(&self, vault_id: Uuid, space: SyncLocal, remote: Arc<dyn RemoteStore>) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            if session.sealed.vault_id() != vault_id {
                return Err(ErrorCode::Internal.into());
            }
            if session.data.sync().is_some() {
                return Err(ErrorCode::SyncAlreadyOn.into());
            }
            let cache = (space.space_id, space.storage.clone(), remote);
            session.data.local_mut().sync = Some(space);
            self.save(&mut st, false, |s| s.data.local_mut().sync = None)?;
            st.sync.remote = Some(cache);
            st.sync.at = Some(Instant::now());
        }
        self.changed();
        Ok(())
    }

    /// A new vault, made by joining `space` where there was none.
    fn create_joined(&self, sealed: Sealed, space: SyncLocal, remote: Arc<dyn RemoteStore>) -> CoreResult<()> {
        {
            let mut st = self.lock();
            if !matches!(st.phase, PhaseState::NoVault) {
                return Err(ErrorCode::VaultExists.into());
            }
            let cache = (space.space_id, space.storage.clone(), remote);
            let mut data = VaultData::new();
            data.local_mut().sync = Some(space);
            let session = Session { sealed, data, import: None, exports: Vec::new() };
            self.write_vault(&session)?;
            st.vault_id = Some(session.sealed.vault_id());
            st.device_slot = false;
            st.phase = PhaseState::Unlocked(Box::new(session));
            st.last_activity = Instant::now();
            st.sync.remote = Some(cache);
            st.sync.at = Some(Instant::now());
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
            (sync.storage.clone(), sync.sync_key.clone())
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
            let cache = (space_id, storage.clone(), remote);
            let previous = std::mem::replace(&mut sync.storage, storage);
            self.save(&mut st, false, move |s| {
                if let Some(sync) = s.data.sync_mut() {
                    sync.storage = previous;
                }
            })?;
            st.sync.remote = Some(cache);
            st.sync.at = Some(Instant::now());
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
            st.sync.at = Some(Instant::now());
        }
        self.changed();
        Ok(())
    }

    /// Remove another device's snapshot from the space (a lost or retired device): it stops being
    /// listed. A device still in use writes its snapshot again on its next run.
    pub async fn sync_remove_device(&self, tag: &str) -> CoreResult<()> {
        let (storage, keys, device) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            let sync = session.data.sync().ok_or(ErrorCode::SyncOff)?;
            (sync.storage.clone(), sync.keys().map_err(|e| sync_error(&e))?, session.data.device())
        };
        let remote = self.space_storage(keys.space_id(), &storage).map_err(|e| sync_error(&e))?;
        let space = Space { prefix: storage.prefix(), keys: &keys, device, device_name: "", keyring: &[] };
        match remove_device(&*remote, &space, &mut SyncState::default(), tag).await {
            Ok(()) => {}
            // This device, or not a device's name: nothing the interface offers.
            Err(SyncError::Misplaced) => return Err(ErrorCode::Internal.into()),
            Err(error) => return Err(sync_error(&error)),
        }
        {
            let mut st = self.lock();
            if let PhaseState::Unlocked(session) = &mut st.phase
                && let Some(sync) = session.data.sync_mut().filter(|s| s.space_id == keys.space_id())
                && let Some(seen) = sync.state.seen.remove(tag)
            {
                let tag = tag.to_owned();
                self.save(&mut st, false, move |s| {
                    if let Some(sync) = s.data.sync_mut() {
                        sync.state.seen.insert(tag, seen);
                    }
                })?;
            }
            st.sync.rolled_back.retain(|t| t != tag);
            st.sync.unreadable.retain(|t| t != tag);
            st.sync.at = Some(Instant::now());
        }
        self.changed();
        Ok(())
    }

    /// Run the sync now (Settings › Sync › "sync now").
    pub fn sync_now(&self) -> CoreResult<()> {
        {
            let st = self.lock();
            unlocked(&st)?.data.sync().ok_or(ErrorCode::SyncOff)?;
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
            st.sync = SyncRuntime { busy: st.sync.busy, ..SyncRuntime::default() };
        }
        self.changed();
        Ok(())
    }

    fn open_storage(&self, storage: &StorageConfig) -> CoreResult<Arc<dyn RemoteStore>> {
        self.shared.ports.sync.open(storage).map_err(|e| sync_error(&e))
    }

    /// Start a run unless one is in progress, which another then follows.
    fn start_sync(&self) {
        {
            let mut st = self.lock();
            if !matches!(&st.phase, PhaseState::Unlocked(session) if session.data.sync().is_some()) {
                return;
            }
            st.sync.at = None;
            if st.sync.busy {
                st.sync.again = true;
                return;
            }
            st.sync.busy = true;
            st.sync.status = SyncStatus::Syncing;
        }
        self.changed();
        let core = self.clone();
        tokio::spawn(async move { core.run_sync().await });
    }

    async fn run_sync(&self) {
        let mut renumbered = false;
        loop {
            let result = self.sync_once().await;
            let now = self.now_ms();
            let again = {
                let mut st = self.lock();
                let mut again = std::mem::take(&mut st.sync.again);
                match &result {
                    Ok(outcome) => {
                        st.sync.status = SyncStatus::Synced { at_ms: now };
                        st.sync.rolled_back.clone_from(&outcome.rolled_back);
                        st.sync.unreadable.clone_from(&outcome.unreadable);
                    }
                    // Another device writes under this one's name: a copied vault, or this vault's
                    // own state put back. This device becomes a new one of the space, once.
                    Err(SyncError::DeviceClash) if !renumbered => {
                        renumbered = true;
                        match self.renumber(&mut st) {
                            Ok(()) => again = true,
                            Err(error) => st.sync.status = SyncStatus::Failed { code: error.code, at_ms: now },
                        }
                    }
                    // Locked meanwhile, the space changed, or the vault could not be written (the
                    // status says so already).
                    Err(SyncError::Interrupted) => {}
                    Err(error) => st.sync.status = SyncStatus::Failed { code: sync_error(error).code, at_ms: now },
                }
                let configured = matches!(&st.phase, PhaseState::Unlocked(session) if session.data.sync().is_some());
                again &= configured;
                if !again {
                    st.sync.busy = false;
                    if configured && st.sync.at.is_none() {
                        st.sync.at = Some(Instant::now() + SYNC_INTERVAL);
                    }
                }
                again
            };
            self.changed();
            if !again {
                break;
            }
        }
    }

    /// One run: the other devices' snapshots in, this device's out, its keyring inside.
    async fn sync_once(&self) -> Result<SyncOutcome, SyncError> {
        let SyncJob { space_id, remote, storage, keys, device, device_name, keyring, mut state, mut working } = self.sync_job()?;
        let remote = remote.ok_or(SyncError::Interrupted)?;
        let space = Space { prefix: storage.prefix(), keys: &keys, device, device_name: &device_name, keyring: &keyring };
        let core = self.clone();
        let settings = storage.clone();
        let mut persist = move |state: &SyncState, working: &Working| core.sync_keep(space_id, &settings, state, working, None);
        let outcome = step_with(&*remote, &space, &mut state, &mut working, self.now_ms(), &mut persist).await?;
        self.sync_keep(space_id, &storage, &state, &working, Some((self.now_ms(), &keyring)))?;
        Ok(outcome)
    }

    fn sync_job(&self) -> Result<SyncJob, SyncError> {
        let (job, storage) = {
            let st = self.lock();
            let PhaseState::Unlocked(session) = &st.phase else { return Err(SyncError::Interrupted) };
            let Some(sync) = session.data.sync() else { return Err(SyncError::Interrupted) };
            let job = SyncJob {
                space_id: sync.space_id,
                remote: None,
                storage: sync.storage.clone(),
                keys: sync.keys()?,
                device: session.data.device(),
                device_name: sync.device_name.clone(),
                keyring: sync.keyring()?,
                state: sync.state.clone(),
                working: Working { entries: session.data.entries.clone(), tombstones: session.data.tombstones.clone() },
            };
            (job, sync.storage.clone())
        };
        let remote = self.space_storage(job.space_id, &storage)?;
        Ok(SyncJob { remote: Some(remote), ..job })
    }

    /// The space's storage: opened once and kept for the next runs, until the space or its
    /// storage settings change. Opening (an HTTP client, the system's certificates) happens
    /// outside the state's lock.
    fn space_storage(&self, space_id: Uuid, storage: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        if let Some((id, config, remote)) = &self.lock().sync.remote
            && *id == space_id
            && config == storage
        {
            return Ok(Arc::clone(remote));
        }
        let remote = self.shared.ports.sync.open(storage)?;
        self.lock().sync.remote = Some((space_id, storage.clone(), Arc::clone(&remote)));
        Ok(remote)
    }

    /// Fold a run's replica into the vault and keep its state; `finished` marks the end of a run,
    /// with the keyring it carried. The accounts merge rather than replace: a change made while
    /// the run was out stays, and the next run takes it along. A space or storage settings changed
    /// meanwhile stop the run, before it writes to where the space no longer is.
    fn sync_keep(
        &self,
        space_id: Uuid,
        storage: &StorageConfig,
        state: &SyncState,
        working: &Working,
        finished: Option<(u64, &[u8])>,
    ) -> Result<(), SyncError> {
        let now = self.now_ms();
        let changed = {
            let mut st = self.lock();
            let PhaseState::Unlocked(session) = &mut st.phase else { return Err(SyncError::Interrupted) };
            if session.data.sync().map(|s| (s.space_id, &s.storage)) != Some((space_id, storage)) {
                return Err(SyncError::Interrupted);
            }
            let before = session.data.clone();
            let changed = merge_entries(&mut session.data.entries, &mut session.data.tombstones, &working.entries, &working.tombstones);
            if changed {
                session.data.observe_stamps();
            }
            if let Some(sync) = session.data.sync_mut() {
                sync.state = state.clone();
                if let Some((at, keyring)) = finished {
                    sync.last_sync_ms = Some(at);
                    // The storage holds the keyring the run carried: still this device's, unless a
                    // new master password sealed another meanwhile.
                    if sync.keyring().is_ok_and(|current| current == keyring) {
                        sync.keyring_written = true;
                    }
                }
            }
            if let Err(error) = self.write_vault(session) {
                session.data = before;
                st.sync.status = SyncStatus::Failed { code: error.code, at_ms: now };
                return Err(SyncError::Interrupted);
            }
            if changed {
                self.schedule_auto_backup(&mut st);
            }
            changed
        };
        if changed {
            self.changed();
        }
        Ok(())
    }

    /// Become a new device of the space: a new number (and clock), a fresh state. The snapshot
    /// under the old number stays, as another device's, until it is removed.
    fn renumber(&self, st: &mut State) -> CoreResult<()> {
        let PhaseState::Unlocked(session) = &mut st.phase else { return Err(ErrorCode::Locked.into()) };
        let before = session.data.clone();
        let local = session.data.local_mut();
        let mut clock = lockra_sync::Clock::new(random_device());
        clock.observe(local.clock.last());
        local.clock = clock;
        if let Some(sync) = local.sync.as_mut() {
            sync.state = SyncState::default();
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
            deadlines.extend(st.sync.at);
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
            let sync = st.sync.at.is_some_and(|at| now >= at);
            if sync {
                st.sync.at = None;
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

/// A change to sync: a run follows after the debounce.
fn schedule_sync(st: &mut State) {
    if matches!(&st.phase, PhaseState::Unlocked(session) if session.data.sync().is_some()) {
        st.sync.at = Some(Instant::now() + SYNC_DEBOUNCE);
    }
}

fn sync_view(st: &State) -> SyncView {
    let PhaseState::Unlocked(session) = &st.phase else { return SyncView { space: None } };
    let space = session.data.sync().map(|sync| {
        let own_tag = sync.keys().map(|keys| keys.device_tag(session.data.device())).unwrap_or_default();
        SyncSpaceView {
            storage: storage_view(&sync.storage),
            device_name: sync.device_name.clone(),
            devices: sync.devices(own_tag),
            status: st.sync.status.clone(),
            last_sync_ms: sync.last_sync_ms,
            rolled_back: st.sync.rolled_back.clone(),
            unreadable: st.sync.unreadable.clone(),
            keyring_pending: sync.keyring_pending(),
            key_saved: sync.key_saved,
        }
    });
    SyncView { space }
}

fn unlocked(st: &State) -> CoreResult<&Session> {
    match &st.phase {
        PhaseState::Unlocked(session) => Ok(session),
        PhaseState::Locked => Err(ErrorCode::Locked.into()),
        PhaseState::NoVault => Err(ErrorCode::NoVault.into()),
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
