//! The OS keychain through `keyring` (Windows Credential Manager, macOS Keychain, Secret Service
//! on Linux): one entry per vault id under the service `dev.lockra.desktop`.

use std::sync::mpsc;
use std::time::Duration;

use lockra_core::ports::{KeychainStatus, PortError, SecretStore};
use zeroize::Zeroizing;

/// How long the start-up probe waits for the keychain (a Secret Service that never answers makes
/// D-Bus wait 25 s; the switch is then shown as unavailable instead of freezing the start).
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// The keychain, if the start-up probe reached it.
#[derive(Debug, Clone)]
pub struct KeyringStore {
    service: String,
    status: KeychainStatus,
}

impl KeyringStore {
    /// Probe the keychain once (reading an entry that does not exist) and remember the answer.
    pub fn probe(service: &str) -> Self {
        let (tx, rx) = mpsc::channel();
        let probe_service = service.to_owned();
        let spawned = std::thread::Builder::new().name("lockra-keychain-probe".into()).spawn(move || {
            let reachable =
                keyring::Entry::new(&probe_service, "lockra-probe").map(|entry| matches!(entry.get_password(), Ok(_) | Err(keyring::Error::NoEntry)));
            let _ = tx.send(reachable.unwrap_or(false));
        });
        let reachable = spawned.is_ok() && rx.recv_timeout(PROBE_TIMEOUT).unwrap_or(false);
        if !reachable {
            tracing::warn!("no usable OS keychain: \"remember on this device\" is unavailable");
        }
        Self { service: service.to_owned(), status: if reachable { KeychainStatus::Available } else { KeychainStatus::Unavailable } }
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, PortError> {
        keyring::Entry::new(&self.service, account).map_err(|e| PortError(e.to_string()))
    }
}

impl SecretStore for KeyringStore {
    fn status(&self) -> KeychainStatus {
        self.status
    }

    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, PortError> {
        match self.entry(account)?.get_password() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(PortError(e.to_string())),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), PortError> {
        self.entry(account)?.set_password(secret).map_err(|e| PortError(e.to_string()))
    }

    fn delete(&self, account: &str) -> Result<(), PortError> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(PortError(e.to_string())),
        }
    }
}
