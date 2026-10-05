//! Where a sync space is stored, as the user configures it: S3-compatible object storage or a
//! WebDAV server, with the credentials to reach it, or a folder on this computer that a cloud
//! drive's client keeps in sync. Only data and the checks that need no request; lockra-remote
//! turns a configuration into a [`crate::RemoteStore`].

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::Zeroizing;

/// Where a sync space is stored, with the credentials to reach it. The vault keeps it in its
/// encrypted local part; the interface writes it and never reads the secret back.
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
    /// A folder on this computer that a cloud drive's client keeps in sync (OneDrive, iCloud
    /// Drive, Dropbox, Jianguoyun, Nextcloud, Synology Drive, Syncthing, …): the drive carries
    /// the snapshots, and no credential is needed here. Only a native dialog names it: the
    /// interface asks for "the folder chosen" without a path, which the core fills in.
    Folder {
        /// The folder, absolute.
        #[serde(default)]
        path: PathBuf,
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
            Self::Folder { path } => f.debug_struct("Folder").field("path", path).finish(),
        }
    }
}

/// Why a [`StorageConfig`] cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// The endpoint or URL is not an address, or carries a user name or password (which would be
    /// kept and shown with the address; they go in their own fields).
    Address,
    /// Plain HTTP to another computer: the credentials would cross the network readable.
    Insecure,
    /// A required field is empty.
    Missing,
}

impl StorageConfig {
    /// The folder inside the storage the space lives under (none in a folder of this computer:
    /// the folder is the place).
    pub fn prefix(&self) -> &str {
        match self {
            Self::S3 { prefix, .. } | Self::Webdav { prefix, .. } => prefix,
            Self::Folder { .. } => "",
        }
    }

    /// The host the storage is reached at, for the interface (no credentials, no path).
    pub fn host(&self) -> Option<String> {
        let address = match self {
            Self::S3 { endpoint, .. } => endpoint,
            Self::Webdav { url, .. } => url,
            Self::Folder { .. } => return None,
        };
        Url::parse(address.trim()).ok().and_then(|u| u.host_str().map(str::to_owned))
    }

    /// A folder on this computer.
    pub fn is_folder(&self) -> bool {
        matches!(self, Self::Folder { .. })
    }

    /// Everything that can be checked without a request: the required fields, an address, and
    /// HTTPS (plain HTTP only to this computer); for a folder, an absolute path.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let (address, required) = match self {
            Self::S3 { endpoint, region, bucket, access_key_id, secret_access_key, .. } => {
                (endpoint, vec![region.as_str(), bucket.as_str(), access_key_id.as_str(), secret_access_key.as_str()])
            }
            Self::Webdav { url, username, password, .. } => (url, vec![username.as_str(), password.as_str()]),
            Self::Folder { path } if path.as_os_str().is_empty() => return Err(ConfigError::Missing),
            Self::Folder { path } => return if path.is_absolute() { Ok(()) } else { Err(ConfigError::Address) },
        };
        if required.iter().any(|v| v.trim().is_empty()) {
            return Err(ConfigError::Missing);
        }
        let parsed = Url::parse(address.trim()).map_err(|_| ConfigError::Address)?;
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(ConfigError::Address);
        }
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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
        // Credentials in the address would be kept, and shown, with it.
        assert_eq!(webdav("https://me:app-password@dav.example.com/dav/").validate(), Err(ConfigError::Address));
        assert_eq!(s3("https://AKID@s3.example.com").validate(), Err(ConfigError::Address));
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
    fn a_folder_is_an_absolute_path_with_no_prefix_and_no_host() {
        let place = std::env::temp_dir().join("Dropbox").join("Lockra");
        let folder = StorageConfig::Folder { path: place.clone() };
        assert_eq!(folder.validate(), Ok(()));
        assert_eq!(folder.prefix(), "");
        assert_eq!(folder.host(), None);
        assert!(format!("{folder:?}").contains("Lockra"));
        assert_eq!(StorageConfig::Folder { path: PathBuf::from("Dropbox/Lockra") }.validate(), Err(ConfigError::Address));
        assert_eq!(StorageConfig::Folder { path: PathBuf::new() }.validate(), Err(ConfigError::Missing));
        // In the vault's local part with its path; from the interface without one (the folder the
        // native dialog chose stands in).
        let json = serde_json::to_value(&folder).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "folder", "path": place.to_str().unwrap() }));
        assert_eq!(serde_json::from_value::<StorageConfig>(json).unwrap(), folder);
        let asked: StorageConfig = serde_json::from_value(serde_json::json!({ "kind": "folder" })).unwrap();
        assert_eq!(asked, StorageConfig::Folder { path: PathBuf::new() });
        assert!(asked.is_folder() && !webdav("https://dav.example.com").is_folder());
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
        assert_eq!(s3("not a url").host(), None);
        // The configuration serializes with its kind, for the vault's local part.
        let json = serde_json::to_value(&dav).unwrap();
        assert_eq!(json["kind"], "webdav");
        let back: StorageConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, dav);
    }
}
