//! The wire types the webview receives: the state it renders, the events it folds in, the code
//! frames it draws and the answers of the commands that return something. Field names are
//! snake_case; `packages/shared/src/schema.ts` mirrors every type here and the IPC fixtures
//! (lockra-bridge's contract test) keep the two in step.

use lockra_otp::{Algorithm, Digits, OtpKind};
use lockra_transfer::{Incompatible, Origin, RejectReason};
use lockra_vault::FileKind;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entry::EntryView;
use crate::error::ErrorCode;
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
}

impl Platform {
    /// The platform this build runs on.
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
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

/// Everything the webview renders.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UiState {
    /// The app version (the root package.json's).
    pub app_version: String,
    /// The host.
    pub platform: Platform,
    /// Session phase.
    pub phase: Phase,
    /// Where the vault and its `.prev` copy live (Settings › About shows it).
    pub data_dir: String,
    /// Unlock screen facts.
    pub lock: LockView,
    /// The entries, without secrets; empty unless unlocked.
    pub entries: Vec<EntryView>,
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
    /// The same account appears earlier in this import.
    Duplicate,
    /// Cannot be imported.
    Unsupported {
        /// Why.
        reason: RejectReason,
    },
}

/// What to do with a candidate on commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateAction {
    /// Add as a new entry.
    Add,
    /// Leave out.
    Skip,
    /// Replace the conflicting entry's secret and parameters (its group and pin stay).
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
    /// A Lockra backup waiting for its password before its entries can be listed.
    pub awaiting_password: Option<String>,
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
    /// The copied code was removed from the clipboard.
    ClipboardCleared,
    /// An import was committed.
    Imported {
        /// New entries.
        added: u32,
        /// Entries whose secret was replaced.
        replaced: u32,
        /// Candidates left out.
        skipped: u32,
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
    /// The automatic check found a newer release (once per version).
    UpdateAvailable {
        /// Its version.
        version: String,
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
        } else {
            Platform::Linux
        };
        assert_eq!(Platform::current(), expected);
        assert_eq!(serde_json::to_value(Platform::Macos).unwrap(), "macos");
    }
}
