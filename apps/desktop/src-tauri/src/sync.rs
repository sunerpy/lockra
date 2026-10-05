//! The sync storage: lockra-remote, over HTTPS (S3-compatible object storage or WebDAV, through
//! OpenDAL) or in a folder of this computer that a cloud drive keeps in sync. The core opens it
//! only for a space the user set up on storage of their own.

use std::sync::Arc;

use lockra_core::StorageConfig;
use lockra_core::ports::{RemoteStore, SyncError, SyncTransport};

/// lockra-remote's storages.
#[derive(Debug, Default, Clone, Copy)]
pub struct Storages;

impl SyncTransport for Storages {
    fn open(&self, config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        lockra_remote::open(config)
    }
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::*;

    #[test]
    fn a_storage_opens_without_contacting_it_and_plain_http_elsewhere_is_refused() {
        let dav = |url: &str| StorageConfig::Webdav { url: url.into(), prefix: String::new(), username: "me".into(), password: Zeroizing::new("pw".into()) };
        assert!(
            matches!(Storages.open(&dav("https://dav.example.com/dav/")), Ok(storage) if !storage.conditional_puts()),
            "WebDAV opens, and writes without conditions"
        );
        assert!(Storages.open(&dav("http://192.168.1.2/dav/")).is_err());
        let folder = tempfile::tempdir().unwrap();
        assert!(
            matches!(Storages.open(&StorageConfig::Folder { path: folder.path().into() }), Ok(storage) if !storage.conditional_puts()),
            "a cloud drive's folder opens, and writes without conditions"
        );
        assert!(Storages.open(&StorageConfig::Folder { path: "relative".into() }).is_err());
    }
}
