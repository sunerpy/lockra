//! The sync storage: lockra-remote over HTTPS (S3-compatible object storage or WebDAV, through
//! OpenDAL). The core opens it only for a space the user set up on storage of their own.

use std::sync::Arc;

use lockra_core::StorageConfig;
use lockra_core::ports::{RemoteStore, SyncError, SyncTransport};

/// lockra-remote's storage.
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpSync;

impl SyncTransport for HttpSync {
    fn open(&self, config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        Ok(Arc::new(lockra_remote::Storage::open(config)?))
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
            matches!(HttpSync.open(&dav("https://dav.example.com/dav/")), Ok(storage) if !storage.conditional_puts()),
            "WebDAV opens, and writes without conditions"
        );
        assert!(HttpSync.open(&dav("http://192.168.1.2/dav/")).is_err());
    }
}
