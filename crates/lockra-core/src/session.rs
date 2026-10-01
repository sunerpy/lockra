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

use lockra_otp::{OtpKind, base32, hotp, totp_window, uri};
use lockra_transfer::{Item, Origin, detect, microsoft, qr, text};
use lockra_vault::{DeviceKey, DeviceSlot, FileKind, KdfCost, Opened, Sealed, read_header, write_atomic};
use parking_lot::{Mutex, MutexGuard};
use tokio::sync::{Notify, broadcast};
use tokio::time::Instant;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::backup::{auto_file_name, pre_restore_file_name, prune};
use crate::entry::{Entry, EntryDraft, EntryPatch, VaultData, clean_name};
use crate::error::{CoreError, CoreResult, ErrorCode};
use crate::export::{self, EXPORT_IDLE, ExportSession};
use crate::import::{AwaitingBackup, Choice, ImportSession, Outcome};
use crate::ports::{Clipboard, Clock, CodeSink, KeychainStatus, SecretStore, Updater};
use crate::settings::{Settings, SettingsStore};
use crate::ui::{
    BackupFailure, BackupView, CodeView, CodesFrame, DeviceUnlockView, ExportPage, ExportStarted, ExportTarget, ImportSource, LockView, Notice, Phase,
    Platform, RestoreView, Revealed, UiEvent, UiState, UpdateStatus, UpdateView,
};
use crate::update::{CHECK_INTERVAL, CHECK_RETRY, ProgressGate, UpdateRun, UpdateState, failure_code};

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
        let (phase, vault_id, device_slot) = match fs::read(&vault_path) {
            Ok(bytes) => match read_header(&bytes) {
                Ok(info) => (PhaseState::Locked, Some(info.vault_id), info.has_device_slot),
                // A damaged header still means "there is a vault": unlocking reports the damage.
                Err(_) => (PhaseState::Locked, None, false),
            },
            Err(_) => (PhaseState::NoVault, None, false),
        };
        let update = UpdateState::new(settings.auto_check_updates && ports.updater.method().is_some());
        let state = State {
            phase,
            settings,
            vault_id,
            device_slot,
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
        };
        let (events, _) = broadcast::channel(64);
        let shared = Arc::new(Shared { config, ports, settings_store, state: Mutex::new(state), events, wake: Notify::new() });
        tokio::spawn(scheduler(Arc::clone(&shared)));
        Self { shared }
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
            let session = Session { sealed, data: VaultData::default(), import: None, exports: Vec::new() };
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

    /// Unlock with the key "remember on this device" left in the keychain.
    pub async fn unlock_with_device(&self) -> CoreResult<()> {
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
        let key = self.device_key(vault_id)?;
        let path = self.vault_path();
        let opened = blocking(move || Ok(Sealed::open_with_device_key(&read_limited(&path)?, &key)?)).await?;
        self.enter(opened)
    }

    fn enter(&self, opened: Opened) -> CoreResult<()> {
        if opened.kind != FileKind::Vault {
            return Err(ErrorCode::VaultCorrupted.into());
        }
        let data = VaultData::from_bytes(&opened.payload).ok_or(ErrorCode::VaultCorrupted)?;
        {
            let mut st = self.lock();
            st.vault_id = Some(opened.sealed.vault_id());
            st.device_slot = opened.sealed.has_device_slot();
            st.failed_attempts = 0;
            st.retry_at_ms = None;
            st.last_activity = Instant::now();
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
            if was_unlocked {
                st.phase = PhaseState::Locked;
                st.auto_backup_at = None;
            }
            was_unlocked
        };
        if locked {
            self.changed();
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
        let (mut sealed, vault_id, device) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            (session.sealed.clone(), session.sealed.vault_id(), st.device_slot)
        };
        let key = if device { self.device_key(vault_id).ok() } else { None };
        let cost = self.shared.config.kdf;
        let (sealed, outcome) = blocking(move || {
            let outcome = sealed.change_password(current.as_bytes(), new.as_bytes(), cost, key.as_ref())?;
            Ok((sealed, outcome))
        })
        .await?;
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            if session.sealed.vault_id() != vault_id {
                return Err(ErrorCode::Internal.into());
            }
            let previous = std::mem::replace(&mut session.sealed, sealed);
            if let Err(error) = self.write_vault(session) {
                session.sealed = previous;
                return Err(error);
            }
            st.device_slot = session_device(&st);
            self.schedule_auto_backup(&mut st);
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

    fn add_entry(&self, entry: Entry) -> CoreResult<Uuid> {
        let id = entry.id;
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            if session.data.entries.iter().any(|e| e.same_account(&entry.to_auth())) {
                return Err(ErrorCode::DuplicateEntry.into());
            }
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
            let entry = session.data.get_mut(id).ok_or(ErrorCode::EntryNotFound)?;
            let before = entry.clone();
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

    /// Delete an entry.
    pub fn delete_entry(&self, id: Uuid) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let index = session.data.entries.iter().position(|e| e.id == id).ok_or(ErrorCode::EntryNotFound)?;
            let removed = session.data.entries.remove(index);
            self.save(&mut st, true, move |s| s.data.entries.insert(index, removed))?;
        }
        self.changed();
        Ok(())
    }

    /// HOTP: advance the counter. It is written to disk before the new code is shown, so a crash
    /// can never show the same code twice.
    pub fn hotp_next(&self, id: Uuid) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let entry = session.data.get_mut(id).ok_or(ErrorCode::EntryNotFound)?;
            let OtpKind::Hotp { counter } = entry.kind else { return Err(ErrorCode::InvalidParameters.into()) };
            entry.kind = OtpKind::Hotp { counter: counter.checked_add(1).ok_or(ErrorCode::InvalidParameters)? };
            self.save(&mut st, true, move |s| {
                if let Some(e) = s.data.get_mut(id) {
                    e.kind = OtpKind::Hotp { counter };
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
            VaultData::from_bytes(&opened.payload).ok_or_else(|| ErrorCode::VaultCorrupted.into())
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
            write_private(&path, text::write(&refs).as_bytes())?;
            Ok(file_name(&path))
        })
        .await
    }

    // ---- backup and restore ---------------------------------------------------------------

    /// Write a backup to `path`: under the master password, or under `separate` if given.
    pub async fn backup_to(&self, path: PathBuf, separate: Option<Zeroizing<String>>) -> CoreResult<String> {
        if let Some(password) = &separate {
            check_password(password)?;
        }
        let (sealed, payload) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            (session.sealed.clone(), session.data.to_bytes())
        };
        let cost = self.shared.config.kdf;
        let name = blocking(move || {
            let bytes = match separate {
                Some(password) => Sealed::create_for(sealed.vault_id(), password.as_bytes(), cost, sealed.created_at_ms())?.seal(FileKind::Backup, &payload)?,
                None => sealed.seal(FileKind::Backup, &payload)?,
            };
            write_private(&path, &bytes)?;
            Ok(file_name(&path))
        })
        .await?;
        self.lock().last_backup_ms = Some(self.now_ms());
        self.notice(Notice::BackupWritten { file_name: name.clone(), automatic: false });
        self.changed();
        Ok(name)
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
        let (bytes, info) = blocking(move || {
            let bytes = Zeroizing::new(read_limited(&path)?);
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
        let data = VaultData::from_bytes(&opened.payload).ok_or(ErrorCode::VaultCorrupted)?;
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
                        let keep = session.sealed.seal(FileKind::Backup, &session.data.to_bytes())?;
                        write_private(&self.shared.config.data_dir.join(pre_restore_file_name(now)), &keep)?;
                        let before = std::mem::replace(&mut session.data, data);
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
        {
            let mut st = self.lock();
            let backup_turned_on = settings.auto_backup.enabled && !st.settings.auto_backup.enabled;
            if !settings.auto_backup.enabled {
                st.auto_backup_at = None;
            }
            if !settings.auto_check_updates {
                st.update.next_check = None;
            } else if !st.settings.auto_check_updates && self.shared.ports.updater.method().is_some() {
                st.update.next_check = Some(Instant::now());
            }
            st.settings = settings;
            if backup_turned_on {
                self.schedule_auto_backup(&mut st);
            }
        }
        self.changed();
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

    /// Look for a newer release now; the answer arrives in the state (`update.status`).
    pub fn update_check(&self) -> CoreResult<()> {
        self.start_update(UpdateRun::Check { automatic: false })
    }

    /// Look for the newest release, download it, verify it and install it, then restart; the
    /// progress arrives in the state. With nothing newer the run ends up to date.
    pub fn update_install(&self) -> CoreResult<()> {
        self.start_update(UpdateRun::Install)
    }

    /// One run at a time, in the background.
    fn start_update(&self, run: UpdateRun) -> CoreResult<()> {
        if self.shared.ports.updater.method().is_none() {
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
        let outcome = self.update_steps(run).await;
        let now = self.now_ms();
        let announce = {
            let mut st = self.lock();
            st.update.busy = false;
            let mut announce = None;
            let failed = outcome.is_err();
            match outcome {
                Ok(Some(status)) => {
                    if let (UpdateRun::Check { automatic: true }, UpdateStatus::Available { version, .. }) = (run, &status)
                        && st.update.announced.as_deref() != Some(version.as_str())
                    {
                        st.update.announced = Some(version.clone());
                        announce = Some(version.clone());
                    }
                    st.update.status = status;
                }
                // Installed: the shell restarts Lockra.
                Ok(None) => {}
                Err(failure) => st.update.status = UpdateStatus::Failed { code: failure_code(failure), at_ms: now },
            }
            if run == (UpdateRun::Check { automatic: true }) && st.settings.auto_check_updates {
                st.update.next_check = Some(Instant::now() + if failed { CHECK_RETRY } else { CHECK_INTERVAL });
            }
            announce
        };
        if let Some(version) = announce {
            self.notice(Notice::UpdateAvailable { version });
        }
        self.changed();
    }

    /// The run's steps; the status it ends on, or `None` once the package is installed.
    async fn update_steps(&self, run: UpdateRun) -> Result<Option<UpdateStatus>, crate::ports::UpdateFailure> {
        let updater = Arc::clone(&self.shared.ports.updater);
        self.set_update(UpdateStatus::Checking);
        let checked_at_ms = self.now_ms();
        let Some(release) = updater.check().await? else { return Ok(Some(UpdateStatus::UpToDate { checked_at_ms })) };
        if run != UpdateRun::Install {
            return Ok(Some(UpdateStatus::Available { version: release.version, notes: release.notes, date: release.date, checked_at_ms }));
        }
        let version = release.version;
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
        // The process ends with the install: a change still inside the backup debounce is backed
        // up first.
        if self.lock().auto_backup_at.is_some() {
            let _ = self.run_auto_backup();
        }
        self.set_update(UpdateStatus::Installing { version });
        updater.install().await?;
        Ok(None)
    }

    /// Record an intermediate status and send the state (no code frame: nothing about the codes
    /// changed).
    fn set_update(&self, status: UpdateStatus) {
        self.lock().update.status = status;
        let _ = self.shared.events.send(UiEvent::State { state: Box::new(self.state()) });
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
                        let bytes = session.sealed.seal(FileKind::Backup, &session.data.to_bytes())?;
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
        let (phase, entries, import) = match &st.phase {
            PhaseState::NoVault => (Phase::NoVault, Vec::new(), None),
            PhaseState::Locked => (Phase::Locked, Vec::new(), None),
            PhaseState::Unlocked(session) => {
                (Phase::Unlocked, session.data.entries.iter().map(Entry::view).collect(), session.import.as_ref().map(|i| i.view(&session.data)))
            }
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
                device_unlock: DeviceUnlockView { available: self.shared.ports.secrets.status() == KeychainStatus::Available, enabled: st.device_slot },
                failed_attempts: st.failed_attempts,
                retry_at_ms: st.retry_at_ms.filter(|at| *at > now),
            },
            entries,
            settings: st.settings.clone(),
            import,
            backup: BackupView { last_backup_ms: st.last_backup_ms, last_auto_file: st.last_auto_file.clone(), last_auto_error: st.last_auto_error },
            restore: st.restore.as_ref().map(|r| r.view.clone()),
            auto_lock_at_ms,
            update: UpdateView { method: self.shared.ports.updater.method(), status: st.update.status.clone() },
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
        }
        deadlines.extend(st.clipboard.as_ref().map(|(_, at)| *at));
        deadlines.extend(st.update.next_check);
        deadlines.into_iter().min()
    }

    /// Everything that is due.
    fn on_timer(&self) {
        let now = Instant::now();
        let (auto_lock, clear, backup, expired, update_check) = {
            let mut st = self.lock();
            let update_check = st.update.next_check.is_some_and(|at| now >= at);
            if update_check {
                st.update.next_check = None;
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
            (auto_lock, clear, backup, expired, update_check)
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
        if update_check && self.start_update(UpdateRun::Check { automatic: true }).is_err() {
            // A check or an install the user started is running: it answers the same question.
            let mut st = self.lock();
            if st.settings.auto_check_updates {
                st.update.next_check = Some(now + CHECK_INTERVAL);
            }
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
    let mut found = FoundFiles { batches: Vec::new(), awaiting: None, notices: Vec::new() };
    let mut files: Vec<(String, Zeroizing<Vec<u8>>, detect::Kind)> = Vec::new();
    for path in paths {
        let name = file_name(path);
        match read_limited(path) {
            Ok(bytes) => {
                let kind = detect::detect(&bytes);
                files.push((name, Zeroizing::new(bytes), kind));
            }
            Err(_) => found.notices.push(Notice::FileUnreadable { name }),
        }
    }
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
