//! The sync storage, lockra-remote over HTTPS as on the desktop (on Android, with the certificate
//! authorities the system keeps), and joining a space from the invitation another device shows,
//! read with the camera: its text, which holds the storage's credentials and the sync key, goes
//! from the camera to the core and never to the webview.

use std::sync::Arc;

use lockra_core::ports::{PortError, RemoteStore, SyncError, SyncTransport};
use lockra_core::ui::JoinSource;
use lockra_core::{Core, CoreError, StorageConfig};
use zeroize::Zeroizing;

use crate::scanner::{self, Scan};

/// lockra-remote's storage.
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpSync;

impl SyncTransport for HttpSync {
    fn open(&self, config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        Ok(Arc::new(lockra_remote::Storage::open(config)?))
    }
}

/// What a scan means for joining: an invitation read joins its space as `sync_join` does, with
/// `password`, `device` and `space_password` (`true`); otherwise as [`scanner::read`] (`false`).
pub async fn join(
    core: &Core,
    scan: Result<Scan, PortError>,
    password: Zeroizing<String>,
    device: String,
    space_password: Option<Zeroizing<String>>,
) -> Result<bool, CoreError> {
    let Some(text) = scanner::read(core, scan)? else { return Ok(false) };
    core.sync_join(JoinSource::Invite { text }, password, device, space_password).await.map(|()| true)
}
