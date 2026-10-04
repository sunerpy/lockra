//! Multi-device sync in the core: what the vault keeps of its space, the replica a run works on,
//! the views and the error codes. The runs and the commands are in `session.rs`, beside the
//! others; lockra-sync does the cryptography and the merge.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use data_encoding::BASE64;
use lockra_sync::{ConfigError, Replica, SpaceKeys, StorageConfig, SyncError, SyncKey, SyncState, Tombstone, merge};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::entry::{Entry, clean_name};
use crate::error::{CoreError, ErrorCode};
use crate::ui::{Platform, StorageView, SyncDeviceView};

/// Delay between the last change and the sync run it causes.
pub const SYNC_DEBOUNCE: Duration = Duration::from_secs(3);
/// Time between runs while the vault is unlocked.
pub const SYNC_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// The longest device name kept (characters), as a snapshot carries it.
pub const MAX_DEVICE_NAME_CHARS: usize = 64;
/// The format of the payload inside a snapshot: the accounts and the deletions.
const PAYLOAD_FORMAT: u32 = 1;

/// This device's sync space, in the vault's local part.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SyncLocal {
    /// Where the space is stored, with the credentials.
    pub storage: StorageConfig,
    /// The space.
    pub space_id: Uuid,
    /// The space's data key (Base64).
    pub data_key: Zeroizing<String>,
    /// The space's sync key (`LKS1-…`), for invitations.
    pub sync_key: Zeroizing<String>,
    /// This device's name in the space.
    pub device_name: String,
    /// What the runs remember.
    #[serde(default)]
    pub state: SyncState,
    /// This device's keyring: the data key under this vault's master password and the sync key
    /// (Base64). Its snapshots carry it; a new master password seals it again. A space kept
    /// without one, or with an empty one, does not read as a space: the vault leaves it out
    /// (`VaultData::from_bytes`), and no snapshot ever goes out without a keyring.
    #[serde(deserialize_with = "keyring_text")]
    pub keyring: String,
    /// The storage holds this keyring: a run wrote it, or found it there, since it was sealed.
    #[serde(default)]
    pub keyring_written: bool,
    /// When a run last finished without error, Unix milliseconds.
    #[serde(default)]
    pub last_sync_ms: Option<u64>,
    /// The sync key was saved or written down. Only the device that made the space starts
    /// without; spaces kept by earlier versions count as saved (they showed the key when made).
    #[serde(default = "saved")]
    pub key_saved: bool,
}

fn saved() -> bool {
    true
}

impl fmt::Debug for SyncLocal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncLocal")
            .field("storage", &self.storage)
            .field("space_id", &self.space_id)
            .field("device_name", &self.device_name)
            .field("state", &self.state)
            .field("keyring_written", &self.keyring_written)
            .field("key_saved", &self.key_saved)
            .finish_non_exhaustive()
    }
}

impl SyncLocal {
    /// A space just created or joined, with this device's `keyring`.
    pub fn new(storage: StorageConfig, keys: &SpaceKeys, sync_key: &SyncKey, device_name: String, keyring: &[u8]) -> Self {
        Self {
            storage,
            space_id: keys.space_id(),
            data_key: keys.data_key_text(),
            sync_key: sync_key.to_text(),
            device_name,
            state: SyncState::default(),
            keyring: BASE64.encode(keyring),
            keyring_written: false,
            last_sync_ms: None,
            key_saved: true,
        }
    }

    pub fn keys(&self) -> Result<SpaceKeys, SyncError> {
        SpaceKeys::from_parts(self.space_id, &self.data_key)
    }

    pub fn sync_key(&self) -> Result<SyncKey, SyncError> {
        SyncKey::from_text(&self.sync_key)
    }

    pub fn keyring(&self) -> Result<Vec<u8>, SyncError> {
        BASE64.decode(self.keyring.as_bytes()).map_err(|_| SyncError::Corrupted)
    }

    /// A new master password's keyring is still to reach the storage: this device's snapshot
    /// there carries the one before.
    pub fn keyring_pending(&self) -> bool {
        self.state.own_seq > 0 && !self.keyring_written
    }

    /// The devices as the last runs found them, this one (`own_tag`) first, the others by name.
    pub fn devices(&self, own_tag: String) -> Vec<SyncDeviceView> {
        let mut others: Vec<SyncDeviceView> = self
            .state
            .seen
            .iter()
            .map(|(tag, seen)| SyncDeviceView { tag: tag.clone(), name: seen.name.clone(), written_at_ms: Some(seen.written_at_ms), this_device: false })
            .collect();
        others.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.tag.cmp(&b.tag)));
        let mut devices = vec![SyncDeviceView { tag: own_tag, name: self.device_name.clone(), written_at_ms: self.state.own_written_at_ms, this_device: true }];
        devices.extend(others);
        devices
    }
}

/// [`SyncLocal::keyring`] as the vault keeps it: Base64 of a keyring, never empty.
fn keyring_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let text = String::deserialize(deserializer)?;
    match BASE64.decode(text.as_bytes()) {
        Ok(bytes) if !bytes.is_empty() => Ok(text),
        _ => Err(serde::de::Error::custom("not a keyring")),
    }
}

/// The storage without its secret.
pub(crate) fn storage_view(storage: &StorageConfig) -> StorageView {
    match storage {
        StorageConfig::S3 { endpoint, region, bucket, prefix, access_key_id, path_style, .. } => StorageView::S3 {
            endpoint: endpoint.clone(),
            region: region.clone(),
            bucket: bucket.clone(),
            prefix: prefix.clone(),
            access_key_id: access_key_id.clone(),
            path_style: *path_style,
        },
        StorageConfig::Webdav { url, prefix, username, .. } => StorageView::Webdav { url: url.clone(), prefix: prefix.clone(), username: username.clone() },
    }
}

/// A device name as the space shows it: cleaned and cut; the platform's name when nothing is
/// left.
pub(crate) fn device_name(name: &str, platform: Platform) -> String {
    let cleaned: String = clean_name(name).chars().take(MAX_DEVICE_NAME_CHARS).collect::<String>().trim().to_owned();
    if !cleaned.is_empty() {
        return cleaned;
    }
    match platform {
        Platform::Windows => "Windows",
        Platform::Macos => "macOS",
        Platform::Linux => "Linux",
        Platform::Android => "Android",
    }
    .to_owned()
}

/// The replica a run works on: a copy of the vault's accounts and deletions taken when the run
/// started.
pub(crate) struct Working {
    pub entries: Vec<Entry>,
    pub tombstones: Vec<Tombstone>,
}

#[derive(Serialize)]
struct PayloadOut<'a> {
    format: u32,
    entries: Vec<Entry>,
    tombstones: &'a [Tombstone],
}

#[derive(Deserialize)]
struct PayloadIn {
    format: u32,
    #[serde(default)]
    entries: Vec<Entry>,
    #[serde(default)]
    tombstones: Vec<Tombstone>,
}

impl Replica for Working {
    fn payload(&self) -> Zeroizing<Vec<u8>> {
        // The copy times stay on this device.
        let entries = self.entries.iter().map(|e| Entry { last_used_at_ms: None, ..e.clone() }).collect();
        let payload = PayloadOut { format: PAYLOAD_FORMAT, entries, tombstones: &self.tombstones };
        // Serializing plain data structures to JSON cannot fail.
        #[allow(clippy::expect_used)]
        Zeroizing::new(serde_json::to_vec(&payload).expect("a sync payload serializes"))
    }

    fn absorb(&mut self, payload: &[u8]) -> Result<bool, SyncError> {
        let theirs: PayloadIn = serde_json::from_slice(payload).map_err(|_| SyncError::Corrupted)?;
        if theirs.format != PAYLOAD_FORMAT {
            return Err(SyncError::Unsupported(theirs.format));
        }
        Ok(merge_entries(&mut self.entries, &mut self.tombstones, &theirs.entries, &theirs.tombstones))
    }
}

/// Fold another replica's accounts and deletions in (last writer wins), keeping this device's copy
/// times, which do not sync; `true` when something changed.
pub(crate) fn merge_entries(entries: &mut Vec<Entry>, tombstones: &mut Vec<Tombstone>, their_entries: &[Entry], their_tombstones: &[Tombstone]) -> bool {
    let used: HashMap<Uuid, u64> = entries.iter().filter_map(|e| e.last_used_at_ms.map(|at| (e.id, at))).collect();
    let changed = merge(entries, tombstones, their_entries, their_tombstones);
    for entry in entries.iter_mut() {
        if let Some(&at) = used.get(&entry.id) {
            entry.last_used_at_ms = Some(entry.last_used_at_ms.map_or(at, |theirs| theirs.max(at)));
        }
    }
    changed
}

/// The code the interface shows for a sync failure.
pub(crate) fn sync_error(error: &SyncError) -> CoreError {
    CoreError::from(match error {
        SyncError::Network(_) => ErrorCode::SyncNetwork,
        SyncError::Denied => ErrorCode::SyncDenied,
        SyncError::Conflict | SyncError::Storage(_) | SyncError::DeviceClash => ErrorCode::SyncStorageFailed,
        SyncError::NotLockra | SyncError::Corrupted | SyncError::Misplaced => ErrorCode::SyncDataCorrupted,
        SyncError::Unsupported(_) => ErrorCode::SyncUnsupported,
        SyncError::WrongCredentials => ErrorCode::SyncWrongCredentials,
        SyncError::NoSpace => ErrorCode::SyncSpaceNotFound,
        SyncError::BadSyncKey => ErrorCode::SyncKeyInvalid,
        SyncError::BadInvite => ErrorCode::SyncInviteInvalid,
        SyncError::BadInviteCode => ErrorCode::SyncInviteCodeWrong,
        SyncError::Interrupted => ErrorCode::Locked,
        SyncError::Random => ErrorCode::Internal,
    })
}

/// The code the interface shows for a storage configuration it cannot use.
pub(crate) fn config_error(error: ConfigError) -> CoreError {
    CoreError::from(match error {
        ConfigError::Address | ConfigError::Missing => ErrorCode::SyncConfigInvalid,
        ConfigError::Insecure => ErrorCode::SyncInsecure,
    })
}

#[cfg(test)]
mod tests {
    use lockra_otp::uri;
    use lockra_sync::Hlc;
    use lockra_transfer::Origin;

    use super::*;

    fn entry(text: &str, wall_ms: u64, device: u64) -> Entry {
        let mut entry = Entry::from_auth(uri::parse(text).unwrap(), Origin::Uri, 1);
        entry.stamp = Hlc { wall_ms, counter: 0, device };
        entry
    }

    #[test]
    fn the_payload_leaves_the_copy_times_here_and_a_merge_keeps_them() {
        let mut mine = entry("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP", 10, 1);
        mine.last_used_at_ms = Some(500);
        let working = Working { entries: vec![mine.clone()], tombstones: Vec::new() };
        let payload: serde_json::Value = serde_json::from_slice(&working.payload()).unwrap();
        assert_eq!(payload["format"], 1);
        assert_eq!(payload["entries"][0]["last_used_at_ms"], serde_json::Value::Null);

        // A rename from another device wins, and this device's copy time stays.
        let mut renamed = mine.clone();
        renamed.issuer = "Renamed".into();
        renamed.stamp = Hlc { wall_ms: 20, counter: 0, device: 2 };
        renamed.last_used_at_ms = None;
        let mut working = working;
        let theirs = Working { entries: vec![renamed], tombstones: Vec::new() };
        assert!(working.absorb(&theirs.payload()).unwrap());
        assert_eq!((working.entries[0].issuer.as_str(), working.entries[0].last_used_at_ms), ("Renamed", Some(500)));
        // Nothing newer: nothing changes.
        assert!(!working.absorb(&theirs.payload()).unwrap());
    }

    #[test]
    fn a_payload_of_another_format_or_none_at_all_is_refused() {
        let mut working = Working { entries: Vec::new(), tombstones: Vec::new() };
        assert_eq!(working.absorb(br#"{"format":2,"entries":[]}"#), Err(SyncError::Unsupported(2)));
        assert_eq!(working.absorb(b"[]"), Err(SyncError::Corrupted));
        assert_eq!(working.absorb(br#"{"entries":[]}"#), Err(SyncError::Corrupted), "the format is required");
    }

    #[test]
    fn device_names_are_cleaned_and_never_empty() {
        assert_eq!(device_name("  Work laptop\n", Platform::Linux), "Work laptop");
        assert_eq!(device_name(&"x".repeat(100), Platform::Linux).chars().count(), MAX_DEVICE_NAME_CHARS);
        assert_eq!(device_name(" \u{7} ", Platform::Macos), "macOS");
        assert_eq!(device_name("", Platform::Windows), "Windows");
        assert_eq!(device_name("", Platform::Android), "Android");
    }

    #[test]
    fn a_space_without_this_devices_keyring_is_not_one_this_version_reads() {
        // A snapshot without a keyring would leave a space nobody can join: no such space is taken
        // in (the vault leaves it out, entry.rs).
        let storage = StorageConfig::Webdav {
            url: "https://dav.example.com/".into(),
            prefix: String::new(),
            username: "me".into(),
            password: Zeroizing::new("x".into()),
        };
        let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
        let whole = serde_json::to_value(SyncLocal::new(storage, &keys, &SyncKey::generate().unwrap(), "Laptop".into(), b"keyring")).unwrap();
        assert!(serde_json::from_value::<SyncLocal>(whole.clone()).is_ok());
        let mut missing = whole.clone();
        missing.as_object_mut().unwrap().remove("keyring");
        assert!(serde_json::from_value::<SyncLocal>(missing).is_err());
        // Nor an empty one (a build that defaulted it saved it so), nor one that is not Base64.
        for text in ["", "not base64!"] {
            let mut kept = whole.clone();
            kept["keyring"] = serde_json::json!(text);
            assert!(serde_json::from_value::<SyncLocal>(kept).is_err(), "{text:?}");
        }
    }

    #[test]
    fn a_space_kept_before_the_key_reminder_counts_its_key_as_saved() {
        let storage = StorageConfig::Webdav {
            url: "https://dav.example.com".into(),
            prefix: String::new(),
            username: "me".into(),
            password: Zeroizing::new("pw".into()),
        };
        let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
        let mut kept = SyncLocal::new(storage, &keys, &SyncKey::generate().unwrap(), "Laptop".into(), b"keyring");
        kept.key_saved = false;
        let whole = serde_json::to_value(&kept).unwrap();
        assert!(!serde_json::from_value::<SyncLocal>(whole.clone()).unwrap().key_saved);
        // Written by 0.7.2 or earlier: no such field, and the key was shown when the space was made.
        let mut older = whole;
        older.as_object_mut().unwrap().remove("key_saved");
        assert!(serde_json::from_value::<SyncLocal>(older).unwrap().key_saved);
    }

    #[test]
    fn every_failure_has_a_code() {
        let cases = [
            (SyncError::Network("x".into()), ErrorCode::SyncNetwork),
            (SyncError::Denied, ErrorCode::SyncDenied),
            (SyncError::Conflict, ErrorCode::SyncStorageFailed),
            (SyncError::Storage("x".into()), ErrorCode::SyncStorageFailed),
            (SyncError::DeviceClash, ErrorCode::SyncStorageFailed),
            (SyncError::NotLockra, ErrorCode::SyncDataCorrupted),
            (SyncError::Corrupted, ErrorCode::SyncDataCorrupted),
            (SyncError::Misplaced, ErrorCode::SyncDataCorrupted),
            (SyncError::Unsupported(9), ErrorCode::SyncUnsupported),
            (SyncError::WrongCredentials, ErrorCode::SyncWrongCredentials),
            (SyncError::NoSpace, ErrorCode::SyncSpaceNotFound),
            (SyncError::BadSyncKey, ErrorCode::SyncKeyInvalid),
            (SyncError::BadInvite, ErrorCode::SyncInviteInvalid),
            (SyncError::BadInviteCode, ErrorCode::SyncInviteCodeWrong),
            (SyncError::Interrupted, ErrorCode::Locked),
            (SyncError::Random, ErrorCode::Internal),
        ];
        for (error, code) in cases {
            assert_eq!(sync_error(&error).code, code, "{error:?}");
        }
        assert_eq!(config_error(ConfigError::Insecure).code, ErrorCode::SyncInsecure);
        assert_eq!(config_error(ConfigError::Missing).code, ErrorCode::SyncConfigInvalid);
        assert_eq!(config_error(ConfigError::Address).code, ErrorCode::SyncConfigInvalid);
    }
}
