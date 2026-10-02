//! The storage of a sync space over HTTP: S3-compatible object storage or WebDAV, through Apache
//! OpenDAL, as lockra-sync's [`RemoteStore`] (docs/security.md, "Sync").
//!
//! Only HTTPS, with rustls on ring and the system's certificate verifier, through the system proxy;
//! plain HTTP only to this computer (the test servers). The credentials live in the
//! vault's encrypted local part and are handed in here; nothing of them is logged.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use lockra_sync::{ObjectMeta, PutCondition, RemoteFuture, RemoteStore, SyncError};
use opendal::layers::{RetryLayer, TimeoutLayer};
use opendal::{ErrorKind, Operator};
use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::Zeroizing;

/// How long opening a connection may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long one request may take in full (a snapshot is a few kilobytes).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// Attempts of a request that failed on the way (the first one included).
const ATTEMPTS: usize = 3;

/// Where a sync space is stored, with the credentials to reach it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StorageConfig {
    /// S3-compatible object storage (AWS S3, Cloudflare R2, MinIO, Backblaze B2, Alibaba OSS, …).
    S3 {
        /// The service's endpoint, `https://s3.eu-central-1.amazonaws.com`.
        endpoint: String,
        /// The region (`auto` where the service has none).
        region: String,
        /// The bucket.
        bucket: String,
        /// The folder inside the bucket the space lives under; may be empty.
        prefix: String,
        /// The access key id.
        access_key_id: String,
        /// The secret access key.
        secret_access_key: Zeroizing<String>,
        /// Address the bucket in the path (MinIO) rather than in the host name (AWS, OSS).
        path_style: bool,
    },
    /// A WebDAV server (Nextcloud, ownCloud, Synology, Jianguoyun, rclone, …).
    Webdav {
        /// The server's WebDAV address, `https://dav.example.com/remote.php/dav/files/me`.
        url: String,
        /// The folder under it the space lives in; may be empty.
        prefix: String,
        /// The user name.
        username: String,
        /// The password (an app password where the service offers one).
        password: Zeroizing<String>,
    },
}

impl fmt::Debug for StorageConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::S3 { endpoint, region, bucket, prefix, path_style, .. } => f
                .debug_struct("S3")
                .field("endpoint", endpoint)
                .field("region", region)
                .field("bucket", bucket)
                .field("prefix", prefix)
                .field("path_style", path_style)
                .finish_non_exhaustive(),
            Self::Webdav { url, prefix, .. } => f.debug_struct("Webdav").field("url", url).field("prefix", prefix).finish_non_exhaustive(),
        }
    }
}

/// Why a [`StorageConfig`] cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// The endpoint or URL is not an address.
    Address,
    /// Plain HTTP to another computer: the credentials would cross the network readable.
    Insecure,
    /// A required field is empty.
    Missing,
}

impl StorageConfig {
    /// The folder inside the storage the space lives under.
    pub fn prefix(&self) -> &str {
        match self {
            Self::S3 { prefix, .. } | Self::Webdav { prefix, .. } => prefix,
        }
    }

    /// The host the storage is reached at, for the interface (no credentials, no path).
    pub fn host(&self) -> Option<String> {
        let address = match self {
            Self::S3 { endpoint, .. } => endpoint,
            Self::Webdav { url, .. } => url,
        };
        Url::parse(address).ok().and_then(|u| u.host_str().map(str::to_owned))
    }

    /// Everything that can be checked without a request.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let (address, required) = match self {
            Self::S3 { endpoint, region, bucket, access_key_id, secret_access_key, .. } => {
                (endpoint, vec![region.as_str(), bucket.as_str(), access_key_id.as_str(), secret_access_key.as_str()])
            }
            Self::Webdav { url, username, password, .. } => (url, vec![username.as_str(), password.as_str()]),
        };
        if required.iter().any(|v| v.trim().is_empty()) {
            return Err(ConfigError::Missing);
        }
        let parsed = Url::parse(address.trim()).map_err(|_| ConfigError::Address)?;
        let host = parsed.host_str().ok_or(ConfigError::Address)?;
        match parsed.scheme() {
            "https" => Ok(()),
            "http" if is_loopback(host) => Ok(()),
            "http" => Err(ConfigError::Insecure),
            _ => Err(ConfigError::Address),
        }
    }
}

fn is_loopback(host: &str) -> bool {
    host == "localhost" || host.trim_matches(|c| c == '[' || c == ']').parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// A sync space's storage, ready for requests.
pub struct Storage {
    operator: Operator,
    conditional: AtomicBool,
}

impl fmt::Debug for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Storage").field("conditional", &self.conditional.load(Ordering::Relaxed)).finish_non_exhaustive()
    }
}

impl Storage {
    /// Connect nothing yet: build the client for `config`.
    pub fn open(config: &StorageConfig) -> Result<Self, SyncError> {
        config.validate().map_err(|e| SyncError::Storage(format!("{e:?}")))?;
        let (operator, conditional) = match config {
            StorageConfig::S3 { endpoint, region, bucket, access_key_id, secret_access_key, path_style, .. } => {
                let mut builder = opendal::services::S3::default()
                    .endpoint(endpoint.trim())
                    .region(region.trim())
                    .bucket(bucket.trim())
                    .access_key_id(access_key_id.trim())
                    .secret_access_key(secret_access_key.trim())
                    // Only the credentials given here: no profile, environment or instance metadata.
                    .disable_config_load()
                    .disable_ec2_metadata();
                if !path_style {
                    builder = builder.enable_virtual_host_style();
                }
                (Operator::new(builder).map_err(map)?, true)
            }
            StorageConfig::Webdav { url, username, password, .. } => {
                let builder = opendal::services::Webdav::default().endpoint(url.trim()).username(username.trim()).password(password);
                (Operator::new(builder).map_err(map)?, false)
            }
        };
        let transport = opendal::HttpTransporter::new(opendal_http_transport_reqwest::ReqwestTransport::new(http_client()?));
        let operator = operator
            .with_context(opendal::OperationContext::new().with_http_transport(transport))
            .layer(TimeoutLayer::new().with_timeout(REQUEST_TIMEOUT))
            .layer(RetryLayer::new().with_max_times(ATTEMPTS - 1).with_jitter());
        Ok(Self { operator, conditional: AtomicBool::new(conditional) })
    }
}

/// The HTTP client: rustls on ring (the process's provider, as the update check installs it), the
/// system's verifier and proxy, bounded connection time.
fn http_client() -> Result<reqwest::Client, SyncError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent("Lockra")
        .https_only(false)
        .build()
        .map_err(|e| SyncError::Storage(e.to_string()))
}

/// The storage's error, as the sync engine tells them apart.
fn map(error: opendal::Error) -> SyncError {
    match error.kind() {
        ErrorKind::PermissionDenied => SyncError::Denied,
        ErrorKind::ConditionNotMatch => SyncError::Conflict,
        ErrorKind::RateLimited => SyncError::Network(error.to_string()),
        _ if error.is_temporary() => SyncError::Network(error.to_string()),
        _ => SyncError::Storage(error.to_string()),
    }
}

impl RemoteStore for Storage {
    fn conditional_puts(&self) -> bool {
        self.conditional.load(Ordering::Relaxed)
    }

    fn list<'a>(&'a self, dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>> {
        Box::pin(async move {
            let entries = match self.operator.list(dir).await {
                Ok(entries) => entries,
                Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
                Err(e) => return Err(map(e)),
            };
            Ok(entries
                .into_iter()
                .filter(|entry| entry.metadata().is_file() && !entry.name().is_empty())
                .map(|entry| ObjectMeta {
                    name: entry.name().to_owned(),
                    etag: entry.metadata().etag().map(str::to_owned),
                    size: entry.metadata().content_length(),
                })
                .collect())
        })
    }

    fn get<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, Option<(Vec<u8>, Option<String>)>> {
        Box::pin(async move {
            match self.operator.read(path).await {
                Ok(buffer) => Ok(Some((buffer.to_vec(), None))),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
                Err(e) => Err(map(e)),
            }
        })
    }

    fn put<'a>(&'a self, path: &'a str, bytes: Vec<u8>, condition: PutCondition) -> RemoteFuture<'a, Option<String>> {
        Box::pin(async move {
            if self.conditional_puts() && condition != PutCondition::Always {
                let write = self.operator.write_with(path, bytes.clone());
                let write = match &condition {
                    PutCondition::IfAbsent => write.if_not_exists(true),
                    PutCondition::IfMatch(etag) => write.if_match(etag),
                    PutCondition::Always => write,
                };
                match write.await {
                    Ok(meta) => return Ok(meta.etag().map(str::to_owned)),
                    // An S3-compatible service without conditional writes: write as WebDAV does
                    // from now on.
                    Err(e) if e.kind() == ErrorKind::Unsupported => self.conditional.store(false, Ordering::Relaxed),
                    Err(e) => return Err(map(e)),
                }
            }
            self.operator.write(path, bytes).await.map(|meta| meta.etag().map(str::to_owned)).map_err(map)
        })
    }

    fn delete<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, ()> {
        Box::pin(async move {
            match self.operator.delete(path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
                Err(e) => Err(map(e)),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s3(endpoint: &str) -> StorageConfig {
        StorageConfig::S3 {
            endpoint: endpoint.into(),
            region: "us-east-1".into(),
            bucket: "lockra".into(),
            prefix: "phone/".into(),
            access_key_id: "AKIDEXAMPLE".into(),
            secret_access_key: Zeroizing::new("wJalrXUtnFEMI/K7MDENG".into()),
            path_style: true,
        }
    }

    fn webdav(url: &str) -> StorageConfig {
        StorageConfig::Webdav { url: url.into(), prefix: String::new(), username: "me".into(), password: Zeroizing::new("app password".into()) }
    }

    #[test]
    fn only_https_or_plain_http_to_this_computer_is_accepted() {
        assert_eq!(s3("https://s3.eu-central-1.amazonaws.com").validate(), Ok(()));
        assert_eq!(webdav("https://dav.jianguoyun.com/dav/").validate(), Ok(()));
        for local in ["http://127.0.0.1:9000", "http://localhost:9000", "http://[::1]:9000"] {
            assert_eq!(s3(local).validate(), Ok(()), "{local}");
        }
        assert_eq!(s3("http://192.168.1.10:9000").validate(), Err(ConfigError::Insecure));
        assert_eq!(webdav("http://nas.local/dav").validate(), Err(ConfigError::Insecure));
        assert_eq!(s3("ftp://example.com").validate(), Err(ConfigError::Address));
        assert_eq!(s3("not a url").validate(), Err(ConfigError::Address));
        assert_eq!(s3("https://").validate(), Err(ConfigError::Address));
    }

    #[test]
    fn every_required_field_must_be_filled() {
        let StorageConfig::S3 { endpoint, region, prefix, access_key_id, secret_access_key, path_style, .. } = s3("https://s3.example.com") else {
            unreachable!()
        };
        let no_bucket = StorageConfig::S3 { endpoint, region, bucket: " ".into(), prefix, access_key_id, secret_access_key, path_style };
        assert_eq!(no_bucket.validate(), Err(ConfigError::Missing));
        let no_password = StorageConfig::Webdav {
            url: "https://dav.example.com".into(),
            prefix: String::new(),
            username: "me".into(),
            password: Zeroizing::new(String::new()),
        };
        assert_eq!(no_password.validate(), Err(ConfigError::Missing));
    }

    #[test]
    fn the_host_and_prefix_are_shown_and_the_secrets_never_are() {
        let config = s3("https://s3.eu-central-1.amazonaws.com");
        assert_eq!(config.host().as_deref(), Some("s3.eu-central-1.amazonaws.com"));
        assert_eq!(config.prefix(), "phone/");
        let debug = format!("{config:?}");
        assert!(!debug.contains("wJalr") && !debug.contains("AKIDEXAMPLE"), "{debug}");
        let dav = webdav("https://dav.jianguoyun.com/dav/");
        assert_eq!(dav.host().as_deref(), Some("dav.jianguoyun.com"));
        assert!(!format!("{dav:?}").contains("app password"));
        // The configuration serializes with its kind, for the vault's local part.
        let json = serde_json::to_value(&dav).unwrap();
        assert_eq!(json["kind"], "webdav");
        let back: StorageConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, dav);
    }

    #[test]
    fn storage_errors_map_to_what_the_engine_distinguishes() {
        assert_eq!(map(opendal::Error::new(ErrorKind::PermissionDenied, "403")), SyncError::Denied);
        assert_eq!(map(opendal::Error::new(ErrorKind::ConditionNotMatch, "412")), SyncError::Conflict);
        assert!(matches!(map(opendal::Error::new(ErrorKind::RateLimited, "429")), SyncError::Network(_)));
        assert!(matches!(map(opendal::Error::new(ErrorKind::Unexpected, "timeout").set_temporary()), SyncError::Network(_)));
        assert!(matches!(map(opendal::Error::new(ErrorKind::Unexpected, "500")), SyncError::Storage(_)));
    }

    #[test]
    fn opening_builds_a_client_without_contacting_anything() {
        let storage = Storage::open(&s3("https://s3.eu-central-1.amazonaws.com")).unwrap();
        assert!(storage.conditional_puts());
        let dav = Storage::open(&webdav("https://dav.example.com/dav/")).unwrap();
        assert!(!dav.conditional_puts());
        assert!(Storage::open(&s3("http://10.0.0.1")).is_err());
        assert!(format!("{storage:?}").contains("conditional"));
    }
}
