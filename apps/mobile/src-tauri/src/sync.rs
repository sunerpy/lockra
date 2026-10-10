//! The sync storage, lockra-remote over HTTPS as on the desktop (on Android, with the certificate
//! authorities the system keeps; a Lockra relay waited on for the other devices' changes), and
//! joining a space from the invitation another device shows, read with the camera: its text, which
//! holds the storage's credentials (or a relay's address) and the sync key, goes from the camera to
//! the core and never to the webview. The phone chooses no folder: a space in a computer's cloud
//! drive folder is reached over the same drive's WebDAV.

use std::sync::Arc;

use lockra_core::ports::{PortError, RemoteStore, SpaceAccess, StorageChanged, StorageWatch, SyncError, SyncTransport};
use lockra_core::ui::JoinSource;
use lockra_core::{Core, CoreError, StorageConfig};
use zeroize::Zeroizing;

use crate::scanner::{self, Scan};

/// lockra-remote's storages.
#[derive(Debug, Default, Clone, Copy)]
pub struct Storages;

impl SyncTransport for Storages {
    fn open(&self, config: &StorageConfig, access: &SpaceAccess) -> Result<Arc<dyn RemoteStore>, SyncError> {
        lockra_remote::open(config, access)
    }

    /// A relay is waited on; the other storages are looked at at the runs' intervals.
    fn watch(&self, config: &StorageConfig, access: &SpaceAccess, dir: &str, changed: StorageChanged) -> Option<StorageWatch> {
        if !config.is_relay() {
            return None;
        }
        match lockra_remote::watch(config, access, dir, changed) {
            Ok(watch) => watch.map(|watch| Box::new(watch) as StorageWatch),
            Err(error) => {
                tracing::debug!(%error, "the relay is not watched; the runs look at it at their intervals");
                None
            }
        }
    }
}

/// How the joining phone proves the user is there: its master password, or without one the
/// fingerprint that unlocks its vault (`reason` for the prompt).
pub struct JoinProof {
    pub password: Option<Zeroizing<String>>,
    pub reason: Option<String>,
}

/// What a scan means for joining: an invitation read joins its space as `sync_join` does, with
/// `proof`, `device`, `space_password` and, for an invitation with the sync key alone, the
/// `storage` this phone reaches the space at (`true`); otherwise as [`scanner::read`] (`false`).
pub async fn join(
    core: &Core,
    scan: Result<Scan, PortError>,
    proof: JoinProof,
    device: String,
    space_password: Option<Zeroizing<String>>,
    storage: Option<StorageConfig>,
) -> Result<bool, CoreError> {
    let Some(text) = scanner::read(core, scan)? else { return Ok(false) };
    core.sync_join(JoinSource::Invite { text, code: None, storage }, proof.password, device, space_password, proof.reason).await.map(|()| true)
}
