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

use futures_util::StreamExt as _;
pub use lockra_sync::{ConfigError, StorageConfig};
use lockra_sync::{MAX_OBJECT_BYTES, ObjectMeta, PutCondition, RemoteFuture, RemoteStore, SyncError};
use opendal::layers::{RetryLayer, TimeoutLayer};
use opendal::{ErrorKind, Operator};
use url::Url;

/// How long opening a connection may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long one request may take in full (a snapshot is a few kilobytes).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// Attempts of a request that failed on the way (the first one included).
const ATTEMPTS: usize = 3;
/// Redirects followed at most.
const MAX_REDIRECTS: usize = 5;

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
            // Both: the control operations, and the reading, writing and listing themselves (whose
            // own default is 10 s).
            .layer(TimeoutLayer::new().with_timeout(REQUEST_TIMEOUT).with_io_timeout(REQUEST_TIMEOUT))
            .layer(RetryLayer::new().with_max_times(ATTEMPTS - 1).with_jitter());
        Ok(Self { operator, conditional: AtomicBool::new(conditional) })
    }
}

/// The HTTP client: rustls on ring (the process's provider, as the update check installs it), the
/// system's verifier and proxy, bounded connection time. A redirect may not leave HTTPS (plain
/// HTTP only to this computer, as for the address itself).
fn http_client() -> Result<reqwest::Client, SyncError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let redirects = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            attempt.stop()
        } else if redirect_allowed(attempt.url()) {
            attempt.follow()
        } else {
            attempt.error("a redirect to plain HTTP")
        }
    });
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent("Lockra")
        .redirect(redirects)
        .build()
        .map_err(|e| SyncError::Storage(e.to_string()))
}

/// Where a redirect may lead: HTTPS anywhere, plain HTTP only to this computer.
fn redirect_allowed(url: &Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => url.host_str().is_some_and(|host| {
            host == "localhost" || host.trim_matches(|c| c == '[' || c == ']').parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
        }),
        _ => false,
    }
}

/// The storage's error, as the sync engine tells them apart. OpenDAL classifies by service: a
/// WebDAV server refusing the credentials (401) is left `Unexpected` with its response attached,
/// and a failure retried until the attempts ran out is marked persistent rather than temporary.
fn map(error: opendal::Error) -> SyncError {
    let text = error.to_string();
    match error.kind() {
        ErrorKind::PermissionDenied => SyncError::Denied,
        // A failed condition (412), or a conditional write racing another (S3's OperationAborted).
        ErrorKind::ConditionNotMatch | ErrorKind::Conflict => SyncError::Conflict,
        ErrorKind::RateLimited => SyncError::Network(text),
        _ if text.contains("status: 401") => SyncError::Denied,
        _ if error.is_temporary() || error.is_persistent() => SyncError::Network(text),
        _ => SyncError::Storage(text),
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
            // Its size first: a larger object than any snapshot is not read, and the read asks for
            // no more than the size (a server sending more is not listened to). No etag: the
            // listing's, taken before the read, is the one a run may skip by.
            let size = match self.operator.stat(path).await {
                Ok(meta) => meta.content_length(),
                Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(map(e)),
            };
            if size > MAX_OBJECT_BYTES {
                return Err(SyncError::Corrupted);
            }
            if size == 0 {
                return Ok(Some((Vec::new(), None)));
            }
            // Streamed, and stopped past the size: a server that ignores the range and sends more is
            // not read to its end.
            let stream = match self.operator.reader(path).await {
                Ok(reader) => reader.into_stream(0..size).await,
                Err(e) => Err(e),
            };
            let mut stream = match stream {
                Ok(stream) => stream,
                Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(map(e)),
            };
            let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
            while let Some(chunk) = stream.next().await {
                for piece in chunk.map_err(map)? {
                    if (bytes.len() + piece.len()) as u64 > size {
                        return Err(SyncError::Corrupted);
                    }
                    bytes.extend_from_slice(&piece);
                }
            }
            Ok(Some((bytes, None)))
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
    use zeroize::Zeroizing;

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
    fn storage_errors_map_to_what_the_engine_distinguishes() {
        assert_eq!(map(opendal::Error::new(ErrorKind::PermissionDenied, "403")), SyncError::Denied);
        assert_eq!(map(opendal::Error::new(ErrorKind::ConditionNotMatch, "412")), SyncError::Conflict);
        assert_eq!(map(opendal::Error::new(ErrorKind::Conflict, "OperationAborted")), SyncError::Conflict);
        assert!(matches!(map(opendal::Error::new(ErrorKind::RateLimited, "429")), SyncError::Network(_)));
        assert!(matches!(map(opendal::Error::new(ErrorKind::Unexpected, "timeout").set_temporary()), SyncError::Network(_)));
        // Retried until the attempts ran out.
        assert!(matches!(map(opendal::Error::new(ErrorKind::Unexpected, "dns").set_temporary().set_persistent()), SyncError::Network(_)));
        // WebDAV's 401, as OpenDAL leaves it.
        let unauthorized = opendal::Error::new(ErrorKind::Unexpected, "").with_context("response", "Parts { status: 401, version: HTTP/1.1 }");
        assert_eq!(map(unauthorized), SyncError::Denied);
        assert!(matches!(map(opendal::Error::new(ErrorKind::Unexpected, "400")), SyncError::Storage(_)));
        assert!(matches!(map(opendal::Error::new(ErrorKind::ConfigInvalid, "NoSuchBucket")), SyncError::Storage(_)));
    }

    #[test]
    fn a_redirect_never_leaves_https_but_for_this_computer() {
        for (url, allowed) in [
            ("https://s3.example.com/x", true),
            ("http://127.0.0.1:9000/x", true),
            ("http://localhost/x", true),
            ("http://[::1]:9000/x", true),
            ("http://s3.example.com/x", false),
            ("http://10.0.0.1/x", false),
            ("ftp://example.com/x", false),
        ] {
            assert_eq!(redirect_allowed(&Url::parse(url).unwrap()), allowed, "{url}");
        }
    }

    /// An S3 server on this computer that says an object has 10 bytes and, asked for them, sends a
    /// mebibyte (it ignores the range).
    async fn lying_server() -> u16 {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut request = vec![0u8; 8192];
                    let read = socket.read(&mut request).await.unwrap_or(0);
                    let head = String::from_utf8_lossy(&request[..read]).starts_with("HEAD");
                    let headers = "ETag: \"a\"\r\nLast-Modified: Thu, 01 Oct 2026 00:00:00 GMT\r\nConnection: close\r\n";
                    if head {
                        let _ = socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: 10\r\n{headers}\r\n").as_bytes()).await;
                    } else {
                        let body = vec![b'x'; 1024 * 1024];
                        let _ = socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{headers}\r\n", body.len()).as_bytes()).await;
                        let _ = socket.write_all(&body).await;
                    }
                });
            }
        });
        port
    }

    #[tokio::test]
    async fn a_server_sending_more_than_the_size_it_gave_is_not_read_to_its_end() {
        let port = lying_server().await;
        let storage = Storage::open(&s3(&format!("http://127.0.0.1:{port}"))).unwrap();
        // Refused at once, as damaged: not a network failure to retry (OpenDAL's own check reads the
        // whole body first, then calls it a temporary error).
        let answer = storage.get("phone/x.lks").await;
        assert_eq!(answer.map(|found| found.map(|(bytes, _)| bytes.len())), Err(SyncError::Corrupted));
    }

    #[tokio::test]
    async fn an_unreachable_storage_is_a_network_failure() {
        // A closed port on this computer: refused at once, retried, then given up.
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let storage = Storage::open(&webdav(&format!("http://127.0.0.1:{port}/dav/"))).unwrap();
        assert!(matches!(storage.list("x/").await, Err(SyncError::Network(_))));
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
