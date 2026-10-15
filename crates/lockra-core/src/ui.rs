//! The wire types the webview receives: the state it renders, the events it folds in, the code
//! frames it draws and the answers of the commands that return something. Field names are
//! snake_case; `packages/shared/src/schema.ts` mirrors every type here and the IPC fixtures
//! (lockra-bridge's contract test) keep the two in step.

use lockra_otp::{Algorithm, Digits, OtpKind};
use lockra_transfer::{Incompatible, Origin, RejectReason};
use lockra_vault::FileKind;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::browser::BrowserView;
use crate::entry::{EntryView, PinnedGroup};
use crate::error::ErrorCode;
use crate::logins::SiteRuleView;
use crate::settings::Settings;

/// The Tauri event that carries [`UiEvent`]s.
pub const UI_EVENT_NAME: &str = "lockra://event";

/// The host operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// Windows.
    Windows,
    /// macOS.
    Macos,
    /// Linux and the other Unix desktops.
    Linux,
    /// The Android app.
    Android,
}

impl Platform {
    /// The platform this build runs on.
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "android") {
            Self::Android
        } else {
            Self::Linux
        }
    }
}

/// Where the session is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// No vault file: the welcome screen.
    NoVault,
    /// A vault exists and is locked.
    Locked,
    /// Unlocked: the codes are available.
    Unlocked,
}

/// What this build offers of what is still being finished. A release build turns a feature on
/// only once it ships; a debug build and the tests have everything on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Features {
    /// Password management: logins, the generator, the browser extension. Off: the commands
    /// answer `feature_unavailable` and the interface shows none of it, while the vault keeps
    /// reading, keeping and syncing what devices with it wrote.
    pub passwords: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self { passwords: cfg!(debug_assertions) }
    }
}

/// Everything the webview renders.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UiState {
    /// The app version (the root package.json's).
    pub app_version: String,
    /// The host.
    pub platform: Platform,
    /// What this build offers of what is still being finished.
    pub features: Features,
    /// Session phase.
    pub phase: Phase,
    /// Where the vault and its `.prev` copy live (Settings › About shows it).
    pub data_dir: String,
    /// Unlock screen facts.
    pub lock: LockView,
    /// The entries, without secrets; empty unless unlocked.
    pub entries: Vec<EntryView>,
    /// Goes up whenever the logins or the site rules change, and as the vault opens and locks:
    /// the logins themselves come from `items_list`, asked for again when it moves.
    pub items_rev: u64,
    /// How many logins the vault holds; 0 unless unlocked.
    pub items_count: u32,
    /// The site rules (never save, never fill, equivalent domains); empty unless unlocked.
    pub site_rules: Vec<SiteRuleView>,
    /// The groups folded in the password list ("" for the logins in no group); this device's, kept
    /// in the vault, so empty unless unlocked.
    pub item_collapsed_groups: Vec<String>,
    /// The password generator.
    pub generator: GeneratorStatus,
    /// The browser extension: the integration, the pairings, the requests waiting for the user.
    pub browser: BrowserView,
    /// The quick search's global shortcut, as the desktop shell registers it.
    pub quick_access: QuickAccessView,
    /// The groups folded in the code list ("" for the accounts in no group); this device's, kept in
    /// the vault, so empty unless unlocked.
    pub collapsed_groups: Vec<String>,
    /// The accounts in the order they were dragged into (`SortOrder::Manual`); this device's, kept
    /// in the vault, so empty unless unlocked.
    pub entry_order: Vec<Uuid>,
    /// The groups in the order they were dragged into ("" for the accounts in no group); this
    /// device's, kept in the vault.
    pub group_order: Vec<String>,
    /// The group this device pins at the top of the code list; kept in the vault, so none unless
    /// unlocked.
    pub pinned_group: Option<PinnedGroup>,
    /// The settings.
    pub settings: Settings,
    /// The import being previewed.
    pub import: Option<ImportView>,
    /// Backup status.
    pub backup: BackupView,
    /// A backup file opened for restoring, waiting for its password.
    pub restore: Option<RestoreView>,
    /// When the vault locks itself if nothing happens, Unix milliseconds.
    pub auto_lock_at_ms: Option<u64>,
    /// The in-app update.
    pub update: UpdateView,
    /// Multi-device sync.
    pub sync: SyncView,
}

/// The quick search (the command palette brought up from anywhere), as Settings › General shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuickAccessView {
    /// The global shortcut the shell registers (`settings.json`'s `quick_access_shortcut`); none
    /// when none is chosen, and where the build or the platform offers no quick search.
    pub shortcut: Option<String>,
    /// Why the shell could not register it (`shortcut_unavailable`); in memory only.
    pub error: Option<ErrorCode>,
}

/// The password generator, as the interface shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GeneratorStatus {
    /// Passwords in its history (shown only on `generator_history_reveal`); 0 unless unlocked.
    pub history_count: u32,
}

/// What `item_copy` or `generator_copy` put on the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CopiedPart {
    /// A login's user name.
    Username,
    /// A login's email address.
    Email,
    /// A login's password.
    Password,
    /// The current code of the account linked to a login.
    Totp,
    /// A custom field's value.
    Field,
    /// The password the generator made.
    Generated,
}

/// Multi-device sync, as Settings › Sync shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncView {
    /// The space this device belongs to; absent when sync is off, and while locked (the vault
    /// holds the space).
    pub space: Option<SyncSpaceView>,
}

/// The sync space this device belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncSpaceView {
    /// Where it is stored, without the secret.
    pub storage: StorageView,
    /// This device's name in the space.
    pub device_name: String,
    /// The devices of the space, this one first.
    pub devices: Vec<SyncDeviceView>,
    /// What the sync is doing.
    pub status: SyncStatus,
    /// When a run last finished without error, Unix milliseconds.
    pub last_sync_ms: Option<u64>,
    /// Devices (tags) whose snapshot went back in sequence on the last run: refused.
    pub rolled_back: Vec<String>,
    /// Devices (tags) whose snapshot does not open: altered, of another space, or from a newer
    /// Lockra.
    pub unreadable: Vec<String>,
    /// A new master password still has to reach the space's keyring.
    pub keyring_pending: bool,
    /// This device joined by the biometric check: its master password recovers the space only
    /// once it was typed here again (the next unlock with it seals its keyring).
    pub keyring_unsealed: bool,
    /// The sync key was saved or written down (set on the device that made the space once the
    /// user says so); until then Settings › Sync reminds of it.
    pub key_saved: bool,
}

/// A sync storage as the interface shows it: everything but the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StorageView {
    /// S3-compatible object storage.
    S3 {
        /// The endpoint.
        endpoint: String,
        /// The region.
        region: String,
        /// The bucket.
        bucket: String,
        /// The folder inside the bucket.
        prefix: String,
        /// The access key id (the secret key is never shown).
        access_key_id: String,
        /// The bucket is addressed in the path.
        path_style: bool,
    },
    /// A WebDAV server.
    Webdav {
        /// Its address.
        url: String,
        /// The folder under it.
        prefix: String,
        /// The user name (the password is never shown).
        username: String,
    },
    /// A folder on this computer that a cloud drive keeps in sync.
    Folder {
        /// The folder, as the system writes it.
        path: String,
    },
    /// A Lockra relay.
    Relay {
        /// Its address.
        url: String,
    },
}

/// A device of the sync space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncDeviceView {
    /// Its tag, the name of its snapshot (for `sync_remove_device`).
    pub tag: String,
    /// Its name.
    pub name: String,
    /// When it last wrote, Unix milliseconds.
    pub written_at_ms: Option<u64>,
    /// This device.
    pub this_device: bool,
}

/// Where the sync is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SyncStatus {
    /// Nothing run since unlocking.
    #[default]
    Idle,
    /// A run is in progress.
    Syncing,
    /// The last run finished.
    Synced {
        /// When, Unix milliseconds.
        at_ms: u64,
    },
    /// The last run failed.
    Failed {
        /// Why.
        code: ErrorCode,
        /// When, Unix milliseconds.
        at_ms: u64,
    },
}

/// A space's sync key (`LKS1-…`), which the interface calls the recovery key: the answer to
/// `sync_key_reveal`, and what `Core::sync_create` made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncKeyView {
    /// `LKS1-…`.
    pub sync_key: String,
}

/// The answer to `sync_invite`: what another device scans or pastes to join, under its own master
/// password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncInvite {
    /// The invitation text (`lockra-invite:1:…`): the storage, its credentials, the sync key and
    /// the space's data key. For a space on a relay it is also the pairing link the user copies
    /// and sends as it is (the relay's address and the keys, no credentials): it opens alone and
    /// for good, which the user chose over a code.
    pub invite: String,
    /// The invitation as a QR code (SVG).
    pub svg: String,
    /// The invitation sealed for sending through a chat or a mail, with its code; none for a space
    /// on a relay, whose pairing link is sent instead.
    pub shared: Option<SharedInvite>,
    /// The invitation carries the storage; not for a space in a folder of this computer, where
    /// the other device says how it reaches the space (the same drive's folder, its WebDAV).
    pub includes_storage: bool,
}

/// An invitation sealed for sending, and what opens it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SharedInvite {
    /// `lockra-invite:2:…`: it opens only with `code`.
    pub text: String,
    /// The one-time code of `text` (`ABCDE-FGHJK`), shown on the screen only.
    pub code: String,
}

/// How a device joins a space.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JoinSource {
    /// An invitation another device of the space showed.
    Invite {
        /// Its text (scanned or pasted).
        text: zeroize::Zeroizing<String>,
        /// The one-time code a sealed text (`lockra-invite:2:…`) opens with.
        #[serde(default)]
        code: Option<zeroize::Zeroizing<String>>,
        /// How this device reaches the space, instead of the storage the invitation carries;
        /// needed when it carries none.
        #[serde(default)]
        storage: Option<lockra_sync::StorageConfig>,
    },
    /// The storage and the sync key, typed in (recovery without another device).
    Manual {
        /// Where the space is stored.
        storage: lockra_sync::StorageConfig,
        /// The sync key.
        sync_key: zeroize::Zeroizing<String>,
    },
}

/// How this copy of Lockra was installed, which is how an update installs: the package format the
/// bundler built it into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMethod {
    /// A Debian package (installing asks for an administrator's password).
    Deb,
    /// An RPM package (installing asks for an administrator's password).
    Rpm,
    /// An AppImage, replaced in place.
    Appimage,
    /// The Windows setup program.
    Nsis,
    /// The Windows installer package.
    Msi,
    /// The macOS app, replaced in place.
    App,
    /// The Android app from a GitHub release (or anything but Google Play): a check finds a newer
    /// release, whose page the phone opens; nothing is downloaded or installed in the app.
    Android,
    /// The Android app from Google Play, which updates it: the phone opens its Play listing and
    /// asks GitHub nothing.
    Play,
}

impl InstallMethod {
    /// An update downloads and installs in the app (everywhere but the phone).
    pub fn installs(self) -> bool {
        !matches!(self, Self::Android | Self::Play)
    }

    /// The app asks for a newer release (everywhere but a copy from Google Play, which Play updates).
    pub fn checks(self) -> bool {
        !matches!(self, Self::Play)
    }
}

/// The in-app update, as Settings › About shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateView {
    /// How an update installs here; absent when this copy cannot update itself (it was not
    /// installed from a package, or the build has no update key).
    pub method: Option<InstallMethod>,
    /// What the updater is doing.
    pub status: UpdateStatus,
}

/// Where the in-app update is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateStatus {
    /// Nothing asked since start.
    Idle,
    /// Asking for a newer release.
    Checking,
    /// This is the newest release.
    UpToDate {
        /// When it was asked, Unix milliseconds.
        checked_at_ms: u64,
    },
    /// A newer release is out.
    Available {
        /// Its version.
        version: String,
        /// Its release notes (Markdown), when there are any.
        notes: Option<String>,
        /// When it was published (RFC 3339), when the release says.
        date: Option<String>,
        /// When it was found, Unix milliseconds.
        checked_at_ms: u64,
    },
    /// Downloading it.
    Downloading {
        /// Its version.
        version: String,
        /// Bytes so far.
        received: u64,
        /// Its size, when the server says.
        total: Option<u64>,
    },
    /// Downloaded and its signature verified: it installs when Lockra restarts for it.
    Ready {
        /// Its version.
        version: String,
    },
    /// Installing it; Lockra restarts next.
    Installing {
        /// Its version.
        version: String,
    },
    /// The last check or install failed.
    Failed {
        /// Why.
        code: ErrorCode,
        /// When, Unix milliseconds.
        at_ms: u64,
    },
}

/// The unlock screen's facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockView {
    /// Remember-on-this-device.
    pub device_unlock: DeviceUnlockView,
    /// Wrong passwords in a row since the last unlock.
    pub failed_attempts: u32,
    /// When the next attempt is allowed, Unix milliseconds.
    pub retry_at_ms: Option<u64>,
}

/// "Remember on this device" (the keychain slot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DeviceUnlockView {
    /// The keychain can be used here.
    pub available: bool,
    /// The vault carries a device slot.
    pub enabled: bool,
    /// The check before the keychain's key is used.
    pub biometric: BiometricView,
}

/// Touch ID or Windows Hello before "remember on this device" unlocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BiometricView {
    /// What this computer offers; `None` when it offers nothing (no sensor, nothing enrolled).
    pub kind: Option<BiometricKind>,
    /// The device slot asks for it.
    pub enabled: bool,
}

/// A platform's biometric check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BiometricKind {
    /// macOS.
    TouchId,
    /// Windows: a fingerprint, the face or the Windows Hello PIN.
    WindowsHello,
    /// Android: a strong biometric, a fingerprint in most phones.
    Fingerprint,
}

/// Where imported accounts came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImportSource {
    /// A file (picked or dropped); only its name.
    File {
        /// The file name, without its directory.
        name: String,
    },
    /// The clipboard.
    Clipboard,
    /// Text typed or pasted into the import box.
    Text,
    /// A QR code the phone's camera read.
    Camera,
}

/// What committing a candidate would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CandidateStatus {
    /// Not in the vault yet.
    New,
    /// The same account (secret and parameters) is already in the vault.
    Exists {
        /// The entry it matches.
        entry_id: Uuid,
    },
    /// An entry with the same issuer and account name but a different secret.
    Conflict {
        /// The entry it collides with.
        entry_id: Uuid,
    },
    /// The same account (or login) appears earlier in this import.
    Duplicate,
    /// Cannot be imported.
    Unsupported {
        /// Why.
        reason: RejectReason,
    },
    /// A login: the same one is in the vault (the same site, user name and password, and the
    /// same code or none from the import; for a backup's, the same login unchanged).
    LoginExists {
        /// The vault's login.
        item_id: Uuid,
    },
    /// A login: the vault's of the same site and user name differs from it (or, for a backup's,
    /// the same login changed since). Replacing is the default.
    LoginConflict {
        /// The vault's login.
        item_id: Uuid,
        /// What differs.
        reason: ConflictReason,
    },
}

/// What differs between a login of the import and the vault's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictReason {
    /// The password: replacing writes the import's, the vault's goes into the history.
    Password,
    /// The code linked (or the vault's has none): replacing links the import's; the account
    /// linked before stays in the code list.
    Code,
    /// A backup's version of the same login, changed since: replacing takes the backup's back.
    Changed,
}

/// What to do with a candidate on commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateAction {
    /// Add as a new entry.
    Add,
    /// Leave out.
    Skip,
    /// Replace the conflicting entry's secret and parameters (its group and pin stay); for a
    /// login, take the import's password or code (or a backup's version) into the vault's.
    Replace,
}

/// One account found by the import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateView {
    /// Position in the import, for the commit choices.
    pub id: u32,
    /// Where it came from.
    pub source: ImportSource,
    /// Which format.
    pub origin: Origin,
    /// Issuer, when the source names it.
    pub issuer: String,
    /// Account name, when the source names it.
    pub account: String,
    /// TOTP / HOTP with period or counter; absent for unsupported candidates.
    pub kind: Option<OtpKind>,
    /// HMAC hash.
    pub algorithm: Option<Algorithm>,
    /// Digits.
    pub digits: Option<Digits>,
    /// The line of a text source.
    pub line: Option<u32>,
    /// What commit would do.
    pub status: CandidateStatus,
    /// The action taken when the commit names no other.
    pub default_action: CandidateAction,
    /// A login, not an account: its title, user name and site (its secrets stay in the core).
    pub login: Option<LoginCandidateView>,
}

/// A login found by the import, as the preview shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoginCandidateView {
    /// Its name.
    pub title: String,
    /// The user name.
    pub username: String,
    /// The host of its first web address; empty when it has none.
    pub host: String,
    /// The candidate of its code (the account it is linked to once imported), when it brings one.
    pub totp: Option<u32>,
}

/// One Google export: which of its codes arrived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoogleBatchView {
    /// The export's id.
    pub id: i32,
    /// How many codes it has.
    pub size: u32,
    /// Zero-based indices received.
    pub received: Vec<u32>,
    /// Zero-based indices still missing.
    pub missing: Vec<u32>,
}

/// The import preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportView {
    /// Everything found so far, in arrival order.
    pub candidates: Vec<CandidateView>,
    /// Google exports seen, with their missing codes.
    pub google_batches: Vec<GoogleBatchView>,
    /// A file waiting for its password before what it holds can be listed.
    pub awaiting: Option<AwaitingView>,
}

/// A file of the import waiting for its password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AwaitingView {
    /// Its name.
    pub name: String,
    /// What it is.
    pub kind: AwaitingKind,
    /// The name of the key file chosen for it (a KeePass database's); the file stays in the core.
    pub key_file: Option<String>,
}

/// What kind of file waits for its password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AwaitingKind {
    /// A Lockra backup.
    Backup,
    /// A KeePass database, which may take a key file too.
    Kdbx,
}

/// Backup status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupView {
    /// The last backup of any kind, Unix milliseconds.
    pub last_backup_ms: Option<u64>,
    /// The last automatic backup's file name.
    pub last_auto_file: Option<String>,
    /// When the last automatic backup failed, and why.
    pub last_auto_error: Option<BackupFailure>,
}

/// A failed automatic backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BackupFailure {
    /// Why.
    pub code: ErrorCode,
    /// When, Unix milliseconds.
    pub at_ms: u64,
}

/// A backup opened for restoring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreView {
    /// The file name.
    pub file_name: String,
    /// Backup or a vault file.
    pub kind: FileKind,
    /// When the vault it belongs to was created, Unix milliseconds.
    pub created_at_ms: u64,
}

/// Export targets that show QR codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportTarget {
    /// Google Authenticator: migration codes, up to ten accounts each.
    Google,
    /// Microsoft Authenticator: one standard code per account.
    Microsoft,
}

/// An entry an export leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Excluded {
    /// Which.
    pub entry_id: Uuid,
    /// Why.
    pub reason: Incompatible,
}

/// The answer to `export_start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportStarted {
    /// The session to page through.
    pub session: Uuid,
    /// Target.
    pub target: ExportTarget,
    /// How many codes.
    pub pages: u32,
    /// Entries left out, with the reason.
    pub excluded: Vec<Excluded>,
}

/// One code of an export session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportPage {
    /// Session.
    pub session: Uuid,
    /// Zero-based page.
    pub index: u32,
    /// Pages in the session.
    pub total: u32,
    /// The QR code as SVG.
    pub svg: String,
    /// The entries this code carries, for checking their codes after the scan.
    pub entry_ids: Vec<Uuid>,
}

/// The answer to `entry_reveal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Revealed {
    /// Which entry.
    pub entry_id: Uuid,
    /// The secret in Base32, in groups of four.
    pub secret: String,
    /// The `otpauth://` URI.
    pub uri: String,
    /// The URI as a QR code (SVG).
    pub svg: String,
}

/// The codes of every entry at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodesFrame {
    /// When computed, Unix milliseconds.
    pub at_ms: u64,
    /// Empty while locked.
    pub codes: Vec<CodeView>,
}

/// One entry's code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeView {
    /// Which entry.
    pub entry_id: Uuid,
    /// The current code.
    pub code: String,
    /// TOTP: the code of the next window.
    pub next_code: Option<String>,
    /// TOTP: start of the current window, Unix milliseconds.
    pub valid_from_ms: Option<u64>,
    /// TOTP: end of the current window (exclusive), Unix milliseconds.
    pub valid_until_ms: Option<u64>,
}

/// Something the webview tells the user once (a toast).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Notice {
    /// A code is on the clipboard.
    Copied {
        /// Which entry.
        entry_id: Uuid,
        /// Seconds until it is cleared; absent when it stays.
        clear_after_s: Option<u32>,
    },
    /// A part of a login, or a generated password, is on the clipboard.
    ItemCopied {
        /// Which login; none for a generated password.
        item_id: Option<Uuid>,
        /// What.
        part: CopiedPart,
        /// Seconds until it is cleared; absent when it stays.
        clear_after_s: Option<u32>,
    },
    /// The copied code was removed from the clipboard.
    ClipboardCleared,
    /// A login was saved, or its password changed, from the browser extension.
    ItemSaved {
        /// Which.
        item_id: Uuid,
        /// Its title.
        title: String,
        /// Its password was changed rather than a new login saved.
        updated: bool,
    },
    /// An extension asks to pair: the window comes to the front with the request.
    BrowserPairRequested {
        /// The request (`browser_pair_approve`).
        id: Uuid,
        /// The browser.
        browser: String,
        /// The code the extension shows.
        code: String,
    },
    /// An extension asks to fill or copy a login that waits for the user.
    BrowserConfirmRequested {
        /// The request (`browser_confirm`).
        id: Uuid,
        /// The browser.
        browser: String,
        /// The login's title.
        title: String,
    },
    /// An extension asked too often and gets nothing until the user resumes it.
    BrowserPaused {
        /// The pairing.
        pairing: Uuid,
        /// The browser.
        browser: String,
        /// What the extension calls itself.
        label: String,
    },
    /// An extension asks Lockra to unlock: the window comes up with the unlock screen.
    BrowserUnlockRequested {
        /// The browser.
        browser: String,
    },
    /// The quick search was asked for (its global shortcut, the tray, `lockra-desktop --palette`):
    /// the shell brought the window up, and the webview opens the command palette. Sent only while
    /// unlocked; not a toast.
    OpenPalette,
    /// The tray's 「设置…」: the shell brought the window up, and the webview opens Settings. Sent
    /// only while unlocked; not a toast.
    OpenSettings,
    /// The tray's 「检查更新…」: the shell brought the window up, and the webview opens the update
    /// dialog (a check started first when no run is in flight and no release waits). Sent only
    /// while unlocked; not a toast.
    OpenUpdates,
    /// An import was committed.
    Imported {
        /// New entries.
        added: u32,
        /// Entries whose secret was replaced.
        replaced: u32,
        /// Candidates left out.
        skipped: u32,
        /// What became of the logins.
        logins: crate::import::LoginOutcome,
    },
    /// A file in the import was not recognized.
    FileUnrecognized {
        /// Its name.
        name: String,
    },
    /// A file in the import could not be read.
    FileUnreadable {
        /// Its name.
        name: String,
    },
    /// A backup was written.
    BackupWritten {
        /// Its file name.
        file_name: String,
        /// Written by the automatic backup.
        automatic: bool,
    },
    /// The automatic backup failed.
    BackupFailed {
        /// Why.
        code: ErrorCode,
    },
    /// A backup was restored.
    Restored {
        /// Entries now in the vault.
        entries: u32,
    },
    /// The vault locked itself after the idle time.
    AutoLocked,
    /// An export session ran out.
    ExportExpired {
        /// Which.
        session: Uuid,
    },
    /// Changing the master password could not keep "remember on this device" (the keychain key
    /// was unreadable); it is off now.
    DeviceUnlockTurnedOff,
    /// A sync run changed the accounts here.
    SyncBrought {
        /// Accounts new to this device.
        added: u32,
        /// Accounts changed on another device.
        updated: u32,
        /// Accounts deleted on another device.
        removed: u32,
        /// Logins new, changed or deleted, counted only where password management is on.
        logins: u32,
        /// The devices the changes came from, as they name themselves; empty when the run failed
        /// after taking them in.
        devices: Vec<String>,
    },
}

/// One event on [`UI_EVENT_NAME`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    /// The whole state, after any change.
    State {
        /// The new state.
        state: Box<UiState>,
    },
    /// A one-off message.
    Notice {
        /// What to tell.
        notice: Notice,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_is_the_build_target() {
        let expected = if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(target_os = "android") {
            Platform::Android
        } else {
            Platform::Linux
        };
        assert_eq!(Platform::current(), expected);
        assert_eq!(serde_json::to_value(Platform::Macos).unwrap(), "macos");
        assert_eq!(serde_json::to_value(Platform::Android).unwrap(), "android");
    }
}
