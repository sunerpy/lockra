//! What the core needs from the platform: the shell wires the real implementations in, the tests
//! wire the fakes (`fakes`). None of these may block for long; the core calls them off the async
//! runtime's critical path.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub use lockra_sync::{RemoteStore, StorageConfig, SyncError};
use parking_lot::Mutex;
use zeroize::Zeroizing;

use crate::ui::{BiometricKind, CodesFrame, InstallMethod};

/// A port failed; the core maps it to a code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PortError(pub String);

/// Whether the keychain can be used at all, decided once at start-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeychainStatus {
    /// Usable.
    Available,
    /// Not on this system (no Secret Service on a Linux session, a sandbox without access).
    Unavailable,
}

/// Why a biometric check did not pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BiometricError {
    /// The user cancelled, or chose to type the password instead.
    Cancelled,
    /// Nothing to check with here now: no sensor, nothing enrolled, turned off, locked out.
    Unavailable,
    /// Not recognised, or the platform answered with an error.
    Failed(String),
}

/// The platform's check of the user (Touch ID, Windows Hello).
pub trait Biometrics: Send + Sync {
    /// What this computer offers now; `None` when it offers nothing.
    fn availability(&self) -> Option<BiometricKind>;
    /// Ask the user to confirm, with `reason` in the system's prompt. Blocks until they answer:
    /// the core calls it on the blocking pool.
    fn verify(&self, reason: &str) -> Result<(), BiometricError>;
}

/// No biometric check (Linux, tests that need none).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoBiometrics;

impl Biometrics for NoBiometrics {
    fn availability(&self) -> Option<BiometricKind> {
        None
    }

    fn verify(&self, _reason: &str) -> Result<(), BiometricError> {
        Err(BiometricError::Unavailable)
    }
}

/// The OS keychain: one secret per vault id.
pub trait SecretStore: Send + Sync {
    /// Usable at all.
    fn status(&self) -> KeychainStatus;
    /// The stored secret, `None` when there is none.
    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, PortError>;
    /// Store or replace.
    fn set(&self, account: &str, secret: &str) -> Result<(), PortError>;
    /// Remove; removing nothing is not an error.
    fn delete(&self, account: &str) -> Result<(), PortError>;
}

/// An image from the clipboard, RGBA, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    /// Pixels per row.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// `width × height × 4` bytes.
    pub rgba: Vec<u8>,
}

/// The system clipboard.
pub trait Clipboard: Send + Sync {
    /// Put a code on the clipboard, marked as excluded from history and cloud sync where the
    /// platform has such a flag.
    fn set_secret_text(&self, text: &str) -> Result<(), PortError>;
    /// The clipboard's text, if any.
    fn text(&self) -> Result<Option<Zeroizing<String>>, PortError>;
    /// The clipboard's image, if any.
    fn image(&self) -> Result<Option<ClipboardImage>, PortError>;
    /// Clear the clipboard if it still holds exactly `expected`; `Ok(true)` when it was cleared.
    fn clear_if(&self, expected: &str) -> Result<bool, PortError>;
}

/// Wall-clock time for the codes; scheduling uses tokio's monotonic clock.
pub trait Clock: Send + Sync {
    /// Unix milliseconds.
    fn now_ms(&self) -> u64;
}

/// Where the code frames go (a `tauri::ipc::Channel` in the shell).
pub trait CodeSink: Send + Sync {
    /// Deliver one frame; `Err` means the receiver is gone and the subscription ends.
    fn send(&self, frame: &CodesFrame) -> Result<(), PortError>;
}

/// A newer release the update source announces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Its version (SemVer, without a `v`).
    pub version: String,
    /// Its release notes (Markdown), when the release has any.
    pub notes: Option<String>,
    /// When it was published (RFC 3339), when the release says.
    pub date: Option<String>,
}

/// Why an update step failed; the core turns it into an error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateFailure {
    /// The update server could not be reached, or answered with an error.
    Network,
    /// The answer could not be read, or has no package for this computer.
    Invalid,
    /// The package is not signed with the key built into the app (or for another version).
    Signature,
    /// The package could not be installed.
    Install,
    /// The user cancelled the administrator prompt.
    Cancelled,
}

/// An update step that waits on the network or on an installer.
pub type UpdateFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, UpdateFailure>> + Send + 'a>>;

/// Download progress: bytes so far, and the size when the server says.
pub type UpdateProgress = Box<dyn FnMut(u64, Option<u64>) + Send>;

/// The update source: the shell's tauri-plugin-updater, which reads the release manifest and
/// checks every package against the public key built into the app. Nothing here runs unless the
/// core asks, and the core asks only when the user does (or turned automatic checks on).
pub trait Updater: Send + Sync {
    /// How this copy installs an update; `None` when it cannot update itself.
    fn method(&self) -> Option<InstallMethod>;
    /// Ask for a newer release; `None` when this is the newest.
    fn check(&self) -> UpdateFuture<'_, Option<Release>>;
    /// Download the release the last `check` found and verify its signature.
    fn download(&self, progress: UpdateProgress) -> UpdateFuture<'_, ()>;
    /// Install the downloaded package and restart Lockra. On Windows the installer takes over and
    /// this does not return.
    fn install(&self) -> UpdateFuture<'_, ()>;
}

/// A watch on a sync storage ([`SyncTransport::watch`]); dropping it stops the watch.
pub type StorageWatch = Box<dyn Send + Sync>;

/// What a storage watch calls when something changed: from the watcher's own thread, and it must
/// not block.
pub type StorageChanged = Box<dyn Fn() + Send + Sync>;

/// Opens the storage of a sync space: lockra-remote over HTTPS in the shells. Opening contacts
/// nothing; the requests go out when the core runs the sync, which it does only for a space the
/// user set up on storage of their own.
pub trait SyncTransport: Send + Sync {
    /// The storage `config` names, ready for requests.
    fn open(&self, config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError>;

    /// Hear, where the storage can tell (a folder of this computer), when objects in `dir` (a
    /// directory of `config`'s, ending in `/`) appear, change or go: `changed` is called then.
    /// `None` where it cannot, or could not start: the runs then look at their intervals only.
    fn watch(&self, _config: &StorageConfig, _dir: &str, _changed: StorageChanged) -> Option<StorageWatch> {
        None
    }
}

/// No sync storage (a build without it): every space fails to open.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSync;

impl SyncTransport for NoSync {
    fn open(&self, _config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        Err(SyncError::Storage("sync is not available in this build".into()))
    }
}

/// A copy that cannot update itself: the tests' default, and the shell's when the build has no
/// update key or was not installed from a package.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoUpdater;

impl Updater for NoUpdater {
    fn method(&self) -> Option<InstallMethod> {
        None
    }

    fn check(&self) -> UpdateFuture<'_, Option<Release>> {
        Box::pin(async { Err(UpdateFailure::Invalid) })
    }

    fn download(&self, _progress: UpdateProgress) -> UpdateFuture<'_, ()> {
        Box::pin(async { Err(UpdateFailure::Invalid) })
    }

    fn install(&self) -> UpdateFuture<'_, ()> {
        Box::pin(async { Err(UpdateFailure::Install) })
    }
}

/// The system clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)).unwrap_or(0)
    }
}

/// A keychain that lives in memory: for debug builds without an OS keychain (headless smoke
/// runs) and for tests. A release build never uses it.
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    entries: Mutex<HashMap<String, Zeroizing<String>>>,
}

impl SecretStore for MemorySecretStore {
    fn status(&self) -> KeychainStatus {
        KeychainStatus::Available
    }

    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, PortError> {
        Ok(self.entries.lock().get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), PortError> {
        self.entries.lock().insert(account.to_owned(), Zeroizing::new(secret.to_owned()));
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), PortError> {
        self.entries.lock().remove(account);
        Ok(())
    }
}

/// A keychain that is not there: release builds where the OS offers none.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSecretStore;

impl SecretStore for NoSecretStore {
    fn status(&self) -> KeychainStatus {
        KeychainStatus::Unavailable
    }

    fn get(&self, _account: &str) -> Result<Option<Zeroizing<String>>, PortError> {
        Err(PortError("no keychain".into()))
    }

    fn set(&self, _account: &str, _secret: &str) -> Result<(), PortError> {
        Err(PortError("no keychain".into()))
    }

    fn delete(&self, _account: &str) -> Result<(), PortError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_memory_store_keeps_and_forgets() {
        let store = MemorySecretStore::default();
        assert_eq!(store.status(), KeychainStatus::Available);
        assert_eq!(store.get("a").unwrap(), None);
        store.set("a", "one").unwrap();
        store.set("a", "two").unwrap();
        assert_eq!(store.get("a").unwrap().as_deref().map(String::as_str), Some("two"));
        store.delete("a").unwrap();
        store.delete("a").unwrap();
        assert_eq!(store.get("a").unwrap(), None);
    }

    #[test]
    fn the_missing_store_refuses_but_deletes_quietly() {
        let store = NoSecretStore;
        assert_eq!(store.status(), KeychainStatus::Unavailable);
        assert!(store.get("a").is_err());
        assert!(store.set("a", "b").is_err());
        assert!(store.delete("a").is_ok());
    }

    #[test]
    fn the_system_clock_is_after_2025() {
        assert!(SystemClock.now_ms() > 1_735_689_600_000);
    }

    #[tokio::test]
    async fn no_updater_cannot_update() {
        let updater = NoUpdater;
        assert_eq!(updater.method(), None);
        assert_eq!(updater.check().await, Err(UpdateFailure::Invalid));
        assert_eq!(updater.download(Box::new(|_, _| {})).await, Err(UpdateFailure::Invalid));
        assert_eq!(updater.install().await, Err(UpdateFailure::Install));
    }
}
