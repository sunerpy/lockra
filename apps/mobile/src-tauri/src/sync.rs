//! The sync storage, lockra-remote over HTTPS as on the desktop (on Android, with the certificate
//! authorities the system keeps), and joining a space from the invitation another device shows,
//! read with the camera: its text, which holds the storage's credentials and the sync key, goes
//! from the camera to the core and never to the webview. The phone chooses no folder: a space in a
//! computer's cloud drive folder is reached over the same drive's WebDAV.

use std::sync::Arc;

use lockra_core::ports::{PortError, RemoteStore, SyncError, SyncTransport};
use lockra_core::ui::JoinSource;
use lockra_core::{Core, CoreError, StorageConfig};
use zeroize::Zeroizing;

use crate::scanner::{self, Scan};

/// lockra-remote's storages.
#[derive(Debug, Default, Clone, Copy)]
pub struct Storages;

impl SyncTransport for Storages {
    fn open(&self, config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        lockra_remote::open(config)
    }
}

/// What a scan means for joining: an invitation read joins its space as `sync_join` does, with
/// `password`, `device`, `space_password` and, for an invitation with the sync key alone, the
/// `storage` this phone reaches the space at (`true`); otherwise as [`scanner::read`] (`false`).
pub async fn join(
    core: &Core,
    scan: Result<Scan, PortError>,
    password: Zeroizing<String>,
    device: String,
    space_password: Option<Zeroizing<String>>,
    storage: Option<StorageConfig>,
) -> Result<bool, CoreError> {
    let Some(text) = scanner::read(core, scan)? else { return Ok(false) };
    core.sync_join(JoinSource::Invite { text, code: None, storage }, password, device, space_password).await.map(|()| true)
}
