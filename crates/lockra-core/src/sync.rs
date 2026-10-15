//! Multi-device sync in the core: what the vault keeps of its space, the replica a run works on,
//! the views and the error codes. The runs and the commands are in `session.rs`, beside the
//! others; lockra-sync does the cryptography and the merge.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::AddAssign;
use std::time::Duration;

use data_encoding::BASE64;
use lockra_sync::{ConfigError, Replica, SpaceAccess, SpaceKeys, StorageConfig, SyncError, SyncKey, SyncState, Tombstone, merge};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::entry::{Entry, clean_name};
use crate::error::{CoreError, ErrorCode};
use crate::item::{SiteRule, VaultItem};
use crate::ui::{Notice, Platform, SnapshotWarningView, StorageView, SyncDeviceView};

/// Delay between the last change and the sync run it causes.
pub const SYNC_DEBOUNCE: Duration = Duration::from_secs(3);
/// Time between runs while the vault is unlocked, behind other windows, and after a failed run.
pub const SYNC_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// Time between runs while the app is in front.
pub const SYNC_INTERVAL_FOREGROUND: Duration = Duration::from_secs(60);
/// Coming to the front runs sync once the last run is this old.
pub const SYNC_FOCUS_MIN: Duration = Duration::from_secs(30);
/// A folder on this computer, in front: looking at it costs nothing (no request, no rate limit).
pub const SYNC_INTERVAL_FOLDER_FOREGROUND: Duration = Duration::from_secs(15);
/// A folder on this computer, behind other windows and after a failed run.
pub const SYNC_INTERVAL_FOLDER: Duration = Duration::from_secs(60);
/// How long after the drive's first change in the folder the run comes: the changes within that
/// time make one run.
pub const SYNC_FOLDER_SETTLE: Duration = Duration::from_secs(1);
/// The longest device name kept (characters), as a snapshot carries it.
pub const MAX_DEVICE_NAME_CHARS: usize = 64;
/// The format of the payload inside a snapshot: the accounts and the deletions, and since the
/// logins arrived, the logins, the site rules and their deletions beside them. Still 1: a device
/// of 0.9 reads a snapshot of another format as unreadable, while it ignores what it does not know
/// in one of its own.
const PAYLOAD_FORMAT: u32 = 1;
/// What this version reads of a payload ([`Replica::reader`]): 0.9 read the accounts (1), this one
/// the logins and the site rules as well (2).
const PAYLOAD_READER: u32 = 2;
/// From this reader on, a device keeps the logins: one below (0.9 says none, 0) does not.
const LOGINS_READER: u32 = 2;
/// The most a relay keeps of a snapshot, header and padding included: its default
/// (`--max-object-bytes`, docs/relay.md). A relay set to keep more or less does not say so.
pub const RELAY_SNAPSHOT_LIMIT: u64 = 4 * 1024 * 1024;

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
    /// This device joined by the biometric check, so its keyring is sealed under random bytes that
    /// open nothing: until the next master password seals its own, the space recovers with the
    /// other devices' passwords, not this one's.
    #[serde(default)]
    pub keyring_unsealed: bool,
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
            .field("keyring_unsealed", &self.keyring_unsealed)
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
            keyring_unsealed: false,
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

    /// What the storage is shown for the space beyond its own credentials: a relay's access
    /// token, from the sync key.
    pub fn access(&self) -> Result<SpaceAccess, SyncError> {
        self.sync_key().map(|key| SpaceAccess::of(&key))
    }

    pub fn keyring(&self) -> Result<Vec<u8>, SyncError> {
        BASE64.decode(self.keyring.as_bytes()).map_err(|_| SyncError::Corrupted)
    }

    /// A new master password's keyring is still to reach the storage: this device's snapshot
    /// there carries the one before.
    pub fn keyring_pending(&self) -> bool {
        self.state.own_seq > 0 && !self.keyring_written
    }

    /// The devices as the last runs found them, this one (`own_tag`, writing with `app_version`)
    /// first, the others by name.
    pub fn devices(&self, own_tag: String, app_version: &str) -> Vec<SyncDeviceView> {
        let mut others: Vec<SyncDeviceView> = self
            .state
            .seen
            .iter()
            .map(|(tag, seen)| SyncDeviceView {
                tag: tag.clone(),
                name: seen.name.clone(),
                written_at_ms: Some(seen.written_at_ms),
                this_device: false,
                app_version: seen.app_version.clone(),
                passwords: seen.reader >= LOGINS_READER,
            })
            .collect();
        others.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.tag.cmp(&b.tag)));
        let mut devices = vec![SyncDeviceView {
            tag: own_tag,
            name: self.device_name.clone(),
            written_at_ms: self.state.own_written_at_ms,
            this_device: true,
            app_version: Some(app_version.to_owned()),
            passwords: true,
        }];
        devices.extend(others);
        devices
    }
}

/// What Settings › Sync says of this device's snapshot of `bytes` on `storage`: only a relay keeps
/// a snapshot of at most [`RELAY_SNAPSHOT_LIMIT`], and the warning starts past three quarters of
/// it, before a write is refused.
pub fn snapshot_warning(storage: &StorageConfig, bytes: Option<u64>) -> Option<SnapshotWarningView> {
    let bytes = bytes?;
    (storage.is_relay() && bytes.saturating_mul(4) > RELAY_SNAPSHOT_LIMIT * 3).then_some(SnapshotWarningView { bytes, limit: RELAY_SNAPSHOT_LIMIT })
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
        StorageConfig::Folder { path } => StorageView::Folder { path: path.display().to_string() },
        StorageConfig::Relay { url } => StorageView::Relay { url: url.clone() },
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

/// The replica a run works on: a copy of the vault's accounts, logins, site rules and deletions
/// taken when the run started.
#[derive(Default)]
pub(crate) struct Working {
    pub entries: Vec<Entry>,
    pub tombstones: Vec<Tombstone>,
    pub items: Vec<VaultItem>,
    pub item_tombstones: Vec<Tombstone>,
    pub site_rules: Vec<SiteRule>,
    pub site_rule_tombstones: Vec<Tombstone>,
}

#[derive(Serialize)]
struct PayloadOut<'a> {
    format: u32,
    entries: Vec<Entry>,
    tombstones: &'a [Tombstone],
    // Left out when empty: a payload without logins is byte for byte what 0.9 writes, so that an
    // update alone writes no snapshot again.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    items: Vec<VaultItem>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    item_tombstones: &'a [Tombstone],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    site_rules: &'a [SiteRule],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    site_rule_tombstones: &'a [Tombstone],
}

#[derive(Deserialize)]
struct PayloadIn {
    format: u32,
    #[serde(default)]
    entries: Vec<Entry>,
    #[serde(default)]
    tombstones: Vec<Tombstone>,
    #[serde(default)]
    items: Vec<VaultItem>,
    #[serde(default)]
    item_tombstones: Vec<Tombstone>,
    #[serde(default)]
    site_rules: Vec<SiteRule>,
    #[serde(default)]
    site_rule_tombstones: Vec<Tombstone>,
}

impl Replica for Working {
    fn payload(&self) -> Zeroizing<Vec<u8>> {
        // The copy times stay on this device.
        let entries = self.entries.iter().map(|e| Entry { last_used_at_ms: None, ..e.clone() }).collect();
        let items = self.items.iter().map(|i| VaultItem { last_used_at_ms: None, ..i.clone() }).collect();
        let payload = PayloadOut {
            format: PAYLOAD_FORMAT,
            entries,
            tombstones: &self.tombstones,
            items,
            item_tombstones: &self.item_tombstones,
            site_rules: &self.site_rules,
            site_rule_tombstones: &self.site_rule_tombstones,
        };
        // Serializing plain data structures to JSON cannot fail.
        #[allow(clippy::expect_used)]
        Zeroizing::new(serde_json::to_vec(&payload).expect("a sync payload serializes"))
    }

    fn absorb(&mut self, payload: &[u8]) -> Result<bool, SyncError> {
        let theirs: PayloadIn = serde_json::from_slice(payload).map_err(|_| SyncError::Corrupted)?;
        if theirs.format != PAYLOAD_FORMAT {
            return Err(SyncError::Unsupported(theirs.format));
        }
        let mut changed = merge_entries(&mut self.entries, &mut self.tombstones, &theirs.entries, &theirs.tombstones);
        changed |= merge_items(&mut self.items, &mut self.item_tombstones, &theirs.items, &theirs.item_tombstones);
        changed |= merge(&mut self.site_rules, &mut self.site_rule_tombstones, &theirs.site_rules, &theirs.site_rule_tombstones);
        Ok(changed)
    }

    fn reader(&self) -> u32 {
        PAYLOAD_READER
    }
}

impl Working {
    /// Fold the run's replica into `data`, merging rather than replacing (a change made while the
    /// run was out stays); `true` when it changed.
    pub(crate) fn fold_into(&self, data: &mut crate::entry::VaultData) -> bool {
        let mut changed = merge_entries(&mut data.entries, &mut data.tombstones, &self.entries, &self.tombstones);
        changed |= merge_items(&mut data.items, &mut data.item_tombstones, &self.items, &self.item_tombstones);
        changed |= merge(&mut data.site_rules, &mut data.site_rule_tombstones, &self.site_rules, &self.site_rule_tombstones);
        changed
    }

    /// A copy of what `data` syncs.
    pub(crate) fn of(data: &crate::entry::VaultData) -> Self {
        Self {
            entries: data.entries.clone(),
            tombstones: data.tombstones.clone(),
            items: data.items.clone(),
            item_tombstones: data.item_tombstones.clone(),
            site_rules: data.site_rules.clone(),
            site_rule_tombstones: data.site_rule_tombstones.clone(),
        }
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

/// Fold another replica's logins and their deletions in, part by part, keeping this device's copy
/// times; `true` when something changed.
pub(crate) fn merge_items(items: &mut Vec<VaultItem>, tombstones: &mut Vec<Tombstone>, their_items: &[VaultItem], their_tombstones: &[Tombstone]) -> bool {
    let used: HashMap<Uuid, u64> = items.iter().filter_map(|i| i.last_used_at_ms.map(|at| (i.id, at))).collect();
    let mut changed = merge(items, tombstones, their_items, their_tombstones);
    for item in items.iter_mut() {
        if let Some(&at) = used.get(&item.id) {
            item.last_used_at_ms = Some(item.last_used_at_ms.map_or(at, |theirs| theirs.max(at)));
        }
        // An item taken in whole keeps the one order every replica holds it in.
        if !item.is_tidy() {
            item.normalize();
            changed = true;
        }
    }
    changed
}

/// What a run took into this device's accounts and logins.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Brought {
    pub added: u32,
    pub updated: u32,
    pub removed: u32,
    /// Logins new, changed or deleted here.
    pub logins: u32,
}

impl Brought {
    /// The accounts after a merge against the ones before it.
    pub(crate) fn between(before: &[Entry], after: &[Entry]) -> Self {
        let previous: HashMap<Uuid, &Entry> = before.iter().map(|e| (e.id, e)).collect();
        let kept: HashSet<Uuid> = after.iter().map(|e| e.id).collect();
        Self {
            added: count(after.iter().filter(|e| !previous.contains_key(&e.id))),
            updated: count(after.iter().filter(|e| previous.get(&e.id).is_some_and(|p| *p != *e))),
            removed: count(before.iter().filter(|e| !kept.contains(&e.id))),
            logins: 0,
        }
    }

    /// The logins new, changed or gone after a merge against the ones before it.
    pub(crate) fn logins_between(before: &[VaultItem], after: &[VaultItem]) -> u32 {
        let previous: HashMap<Uuid, &VaultItem> = before.iter().map(|i| (i.id, i)).collect();
        let kept: HashSet<Uuid> = after.iter().map(|i| i.id).collect();
        let changed = count(after.iter().filter(|i| previous.get(&i.id).is_none_or(|p| *p != *i)));
        changed.saturating_add(count(before.iter().filter(|i| !kept.contains(&i.id))))
    }

    /// Whether the run changed any account or login here.
    pub(crate) fn any(self) -> bool {
        self != Self::default()
    }

    /// The notice for it, naming the devices it came from.
    pub(crate) fn notice(self, devices: Vec<String>) -> Notice {
        Notice::SyncBrought { added: self.added, updated: self.updated, removed: self.removed, logins: self.logins, devices }
    }
}

impl AddAssign for Brought {
    fn add_assign(&mut self, other: Self) {
        self.added = self.added.saturating_add(other.added);
        self.updated = self.updated.saturating_add(other.updated);
        self.removed = self.removed.saturating_add(other.removed);
        self.logins = self.logins.saturating_add(other.logins);
    }
}

fn count<T>(items: impl Iterator<Item = T>) -> u32 {
    u32::try_from(items.count()).unwrap_or(u32::MAX)
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
        SyncError::FolderMissing => ErrorCode::SyncFolderMissing,
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
    use lockra_sync::{Hlc, RemoteStore};
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
        let working = Working { entries: vec![mine.clone()], ..Working::default() };
        let payload: serde_json::Value = serde_json::from_slice(&working.payload()).unwrap();
        assert_eq!(payload["format"], 1);
        assert_eq!(payload["entries"][0]["last_used_at_ms"], serde_json::Value::Null);

        // A rename from another device wins, and this device's copy time stays.
        let mut renamed = mine.clone();
        renamed.issuer = "Renamed".into();
        renamed.stamp = Hlc { wall_ms: 20, counter: 0, device: 2 };
        renamed.last_used_at_ms = None;
        let mut working = working;
        let theirs = Working { entries: vec![renamed], ..Working::default() };
        assert!(working.absorb(&theirs.payload()).unwrap());
        assert_eq!((working.entries[0].issuer.as_str(), working.entries[0].last_used_at_ms), ("Renamed", Some(500)));
        // Nothing newer: nothing changes.
        assert!(!working.absorb(&theirs.payload()).unwrap());
    }

    #[test]
    fn what_a_merge_brought_counts_new_changed_and_gone_accounts() {
        let a = entry("otpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP", 10, 1);
        let b = entry("otpauth://totp/B:b?secret=GEZDGNBV", 10, 1);
        let c = entry("otpauth://totp/C:c?secret=MFRGGZDF", 10, 1);
        let d = entry("otpauth://totp/D:d?secret=MZXW6YTB", 10, 1);
        let mut renamed = b.clone();
        renamed.issuer = "B2".into();
        let before = [a.clone(), b, c];
        let brought = Brought::between(&before, &[a.clone(), renamed, d]);
        assert_eq!(brought, Brought { added: 1, updated: 1, removed: 1, logins: 0 });
        assert!(brought.any());
        assert!(!Brought::between(&before, &before).any());

        let mut total = Brought::default();
        total += brought;
        total += Brought { added: 2, updated: 0, removed: u32::MAX, logins: 4 };
        assert_eq!(total, Brought { added: 3, updated: 1, removed: u32::MAX, logins: 4 });
        assert_eq!(
            total.notice(vec!["Phone".into()]),
            Notice::SyncBrought { added: 3, updated: 1, removed: u32::MAX, logins: 4, devices: vec!["Phone".into()] }
        );
    }

    #[test]
    fn a_payload_of_another_format_or_none_at_all_is_refused() {
        let mut working = Working::default();
        assert_eq!(working.absorb(br#"{"format":2,"entries":[]}"#), Err(SyncError::Unsupported(2)));
        assert_eq!(working.absorb(b"[]"), Err(SyncError::Corrupted));
        assert_eq!(working.absorb(br#"{"entries":[]}"#), Err(SyncError::Corrupted), "the format is required");
    }

    /// A login with a user name, an email, a password, two sites, notes and tags, as people keep
    /// them, and three earlier passwords.
    fn typical_login(n: usize, clock: &mut lockra_sync::Clock) -> VaultItem {
        use crate::item::{Purpose, UrlRule, random_write_id};
        let now = 1_790_000_000_000 + n as u64;
        let mut item = VaultItem::login(Uuid::new_v4(), now, clock.tick(now), random_write_id());
        item.title = format!("Service number {n}");
        item.urls = vec![
            UrlRule { url: format!("https://login.service{n}.example.com/"), matching: Default::default(), extra: Default::default() },
            UrlRule { url: format!("https://app.service{n}.example.com/account"), matching: Default::default(), extra: Default::default() },
        ];
        item.notes = Zeroizing::new(format!("Recovery codes are in the safe; support PIN {n:06}. ").repeat(2));
        item.tags = vec!["work".into(), "shared".into()];
        item.group = Some("Work".into());
        for (id, purpose, value) in [(1, Purpose::Username, format!("user{n}")), (2, Purpose::Email, format!("user{n}@example.com"))] {
            item.write_field(Uuid::from_u128(id), purpose, clock.tick(now), random_write_id(), now, |f| f.value = Zeroizing::new(value));
        }
        for round in 0..4 {
            item.write_field(Uuid::from_u128(3), Purpose::Password, clock.tick(now), random_write_id(), now, |f| {
                f.value = Zeroizing::new(format!("Kx9!vQ2#mZ7@pL4&{n}-{round}"));
                f.hidden = true;
            });
        }
        item
    }

    #[test]
    fn two_thousand_logins_fit_a_relays_snapshot_and_settings_warn_of_them() {
        let mut clock = lockra_sync::Clock::new(7);
        let items: Vec<VaultItem> = (0..2000).map(|n| typical_login(n, &mut clock)).collect();
        assert_eq!(items[0].history.len(), 3);
        let working = Working { items, ..Working::default() };
        // A relay keeps snapshots of at most 4 MiB (docs/relay.md), header and padding included.
        // These logins are on the heavy side (notes, two sites and three earlier passwords each):
        // 3.5 MiB measured on 2026-10-14, past the 75 % at which Settings › Sync warns.
        let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
        let tag = keys.device_tag(1);
        let snapshot = lockra_sync::Snapshot {
            seq: 1,
            written_at_ms: 1,
            device_name: "Desktop".into(),
            keyring: vec![0; 600],
            payload: working.payload(),
            app_version: Some("0.10.0".into()),
            reader: PAYLOAD_READER,
        };
        let size = u64::try_from(lockra_sync::seal_snapshot(&keys, &tag, &snapshot).unwrap().len()).unwrap();
        assert!(size < RELAY_SNAPSHOT_LIMIT, "2000 logins take {size} bytes");
        let relay = StorageConfig::Relay { url: "https://relay.example.com".into() };
        assert_eq!(snapshot_warning(&relay, Some(size)), Some(SnapshotWarningView { bytes: size, limit: RELAY_SNAPSHOT_LIMIT }));
    }

    #[test]
    fn only_a_relay_space_warns_and_only_past_three_quarters_of_its_limit() {
        let relay = StorageConfig::Relay { url: "https://relay.example.com".into() };
        let three_quarters = RELAY_SNAPSHOT_LIMIT / 4 * 3;
        assert_eq!(snapshot_warning(&relay, Some(three_quarters)), None);
        assert_eq!(snapshot_warning(&relay, Some(three_quarters + 1)), Some(SnapshotWarningView { bytes: three_quarters + 1, limit: RELAY_SNAPSHOT_LIMIT }));
        assert_eq!(snapshot_warning(&relay, None), None, "no snapshot yet");
        // The other storages keep a snapshot of any size.
        let folder = StorageConfig::Folder { path: "/home/me/Drive/Lockra".into() };
        assert_eq!(snapshot_warning(&folder, Some(RELAY_SNAPSHOT_LIMIT)), None);
    }

    #[test]
    fn the_devices_say_the_version_they_write_with_and_whether_they_keep_passwords() {
        let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
        let storage = StorageConfig::Relay { url: "https://relay.example.com".into() };
        let mut local = SyncLocal::new(storage, &keys, &SyncKey::generate().unwrap(), "Desktop".into(), b"keyring");
        let seen = |name: &str, app_version: Option<&str>, reader| lockra_sync::Seen {
            seq: 1,
            etag: None,
            name: name.into(),
            written_at_ms: 5,
            app_version: app_version.map(str::to_owned),
            reader,
        };
        local.state.seen.insert(keys.device_tag(2), seen("Phone", Some("0.10.0"), 2));
        // 0.9 says neither: it reads the accounts alone.
        local.state.seen.insert(keys.device_tag(3), seen("Old laptop", None, 0));
        let listed: Vec<_> = local.devices(keys.device_tag(1), "0.10.1").into_iter().map(|d| (d.name, d.app_version, d.passwords, d.this_device)).collect();
        assert_eq!(
            listed,
            [
                ("Desktop".to_owned(), Some("0.10.1".to_owned()), true, true),
                ("Old laptop".to_owned(), None, false, false),
                ("Phone".to_owned(), Some("0.10.0".to_owned()), true, false),
            ]
        );
    }

    /// What 0.9.1 reads of a snapshot's payload, frozen: the accounts and their deletions.
    #[derive(Deserialize)]
    struct PayloadIn09 {
        format: u32,
        #[serde(default)]
        entries: Vec<Entry>,
        #[serde(default)]
        tombstones: Vec<Tombstone>,
    }

    #[test]
    fn lockra_0_9_reads_the_accounts_of_a_snapshot_with_logins_and_leaves_the_logins() {
        let mut clock = lockra_sync::Clock::new(7);
        let working = Working {
            entries: vec![entry("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP", 10, 1)],
            items: vec![typical_login(1, &mut clock)],
            ..Working::default()
        };
        let read: PayloadIn09 = serde_json::from_slice(&working.payload()).unwrap();
        assert_eq!((read.format, read.entries.len(), read.tombstones.len()), (1, 1, 0), "the format it knows, its accounts");
        // A snapshot without logins is what 0.9 writes, byte for byte.
        let codes_only = Working { entries: working.entries.clone(), ..Working::default() };
        let text = String::from_utf8(codes_only.payload().to_vec()).unwrap();
        assert!(!text.contains("items") && !text.contains("site_rules"), "{text}");
        assert_eq!(working.reader(), 2);
    }

    #[tokio::test]
    async fn a_state_of_0_9_reads_once_more_what_it_had_taken_in() {
        use lockra_sync::{MemoryRemote, Space, SpaceKeys, step};
        let remote = MemoryRemote::new(false);
        let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
        let mut clock = lockra_sync::Clock::new(1);
        // A newer device writes a login beside its account.
        let mut desktop = Working {
            entries: vec![entry("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP", 10, 1)],
            items: vec![typical_login(1, &mut clock)],
            ..Working::default()
        };
        let desktop_space = Space { prefix: "", keys: &keys, device: 1, device_name: "Desktop", keyring: b"keyring", app_version: Some("0.10.0") };
        let mut desktop_state = lockra_sync::SyncState::default();
        step(&remote, &desktop_space, &mut desktop_state, &mut desktop, 100).await.unwrap();
        // The phone on 0.9 takes it in: its accounts only, its state as 0.9 saves it.
        struct Phone09(Vec<Entry>, Vec<Tombstone>);
        impl Replica for Phone09 {
            fn payload(&self) -> Zeroizing<Vec<u8>> {
                Zeroizing::new(serde_json::to_vec(&serde_json::json!({ "format": 1, "entries": self.0, "tombstones": self.1 })).unwrap())
            }
            fn absorb(&mut self, payload: &[u8]) -> Result<bool, SyncError> {
                let theirs: PayloadIn09 = serde_json::from_slice(payload).map_err(|_| SyncError::Corrupted)?;
                Ok(merge(&mut self.0, &mut self.1, &theirs.entries, &theirs.tombstones))
            }
        }
        let phone_space = Space { prefix: "", keys: &keys, device: 2, device_name: "Phone", keyring: b"keyring", app_version: None };
        let mut old = Phone09(Vec::new(), Vec::new());
        let mut state = lockra_sync::SyncState::default();
        step(&remote, &phone_space, &mut state, &mut old, 200).await.unwrap();
        let mut saved: serde_json::Value = serde_json::to_value(&state).unwrap();
        saved.as_object_mut().unwrap().remove("reader");
        let mut state: lockra_sync::SyncState = serde_json::from_value(saved).unwrap();
        // Updated in place; the desktop writes nothing new, its snapshot keeps its etag.
        let etag = remote.list(&lockra_sync::devices_dir("", keys.space_id())).await.unwrap().into_iter().map(|m| m.etag).collect::<Vec<_>>();
        let mut phone = Working { entries: old.0, tombstones: old.1, ..Working::default() };
        let outcome = step(&remote, &phone_space, &mut state, &mut phone, 300).await.unwrap();
        assert!(outcome.changed);
        assert_eq!(phone.items.len(), 1, "the logins arrive");
        assert_eq!(state.reader, 2);
        let after = remote.list(&lockra_sync::devices_dir("", keys.space_id())).await.unwrap();
        assert!(after.iter().any(|m| etag.contains(&m.etag)), "the desktop's snapshot is the one it was");
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
