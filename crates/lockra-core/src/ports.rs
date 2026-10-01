//! What the core needs from the platform: the shell wires the real implementations in, the tests
//! wire the fakes (`fakes`). None of these may block for long; the core calls them off the async
//! runtime's critical path.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use zeroize::Zeroizing;

use crate::ui::CodesFrame;

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
}
