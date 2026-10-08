//! The sync storage: lockra-remote, over HTTPS (S3-compatible object storage or WebDAV, through
//! OpenDAL; a Lockra relay, waited on for the other devices' changes) or in a folder of this
//! computer that a cloud drive keeps in sync, watched for the files the drive brings. The core
//! opens it only for a space the user set up.

use std::sync::Arc;

use lockra_core::StorageConfig;
use lockra_core::ports::{RemoteStore, SpaceAccess, StorageChanged, StorageWatch, SyncError, SyncTransport};

/// lockra-remote's storages.
#[derive(Debug, Default, Clone, Copy)]
pub struct Storages;

impl SyncTransport for Storages {
    fn open(&self, config: &StorageConfig, access: &SpaceAccess) -> Result<Arc<dyn RemoteStore>, SyncError> {
        lockra_remote::open(config, access)
    }

    fn watch(&self, config: &StorageConfig, access: &SpaceAccess, dir: &str, changed: StorageChanged) -> Option<StorageWatch> {
        match lockra_remote::watch(config, access, dir, changed) {
            Ok(watch) => watch.map(|watch| Box::new(watch) as StorageWatch),
            Err(error) => {
                tracing::debug!(%error, "the sync storage is not watched; the runs look at it at their intervals");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::*;

    #[test]
    fn a_storage_opens_without_contacting_it_and_plain_http_elsewhere_is_refused() {
        let access = SpaceAccess::of(&lockra_sync::SyncKey::generate().unwrap());
        let dav = |url: &str| StorageConfig::Webdav { url: url.into(), prefix: String::new(), username: "me".into(), password: Zeroizing::new("pw".into()) };
        assert!(
            matches!(Storages.open(&dav("https://dav.example.com/dav/"), &access), Ok(storage) if !storage.conditional_puts()),
            "WebDAV opens, and writes without conditions"
        );
        assert!(Storages.open(&dav("http://192.168.1.2/dav/"), &access).is_err());
        let folder = tempfile::tempdir().unwrap();
        assert!(
            matches!(Storages.open(&StorageConfig::Folder { path: folder.path().into() }, &access), Ok(storage) if !storage.conditional_puts()),
            "a cloud drive's folder opens, and writes without conditions"
        );
        assert!(Storages.open(&StorageConfig::Folder { path: "relative".into() }, &access).is_err());
        let relay = StorageConfig::Relay { url: "https://lockra-relay.onethinker.top".into() };
        assert!(matches!(Storages.open(&relay, &access), Ok(storage) if storage.conditional_puts()), "a relay opens, and holds conditions");
        assert!(Storages.open(&StorageConfig::Relay { url: "http://relay.example.com".into() }, &access).is_err());
        // Nothing to watch on WebDAV.
        assert!(Storages.watch(&dav("https://dav.example.com/dav/"), &access, "x/", Box::new(|| {})).is_none());
    }
}
