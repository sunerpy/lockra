//! Multi-device sync in the core: what the vault keeps of its space, the replica a run works on,
//! the views and the error codes. The runs and the commands are in `session.rs`, beside the
//! others; lockra-sync does the cryptography and the merge.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::ops::AddAssign;
use std::time::Duration;

use data_encoding::BASE64;
use lockra_sync::{ConfigError, Replica, Seen, SpaceKeys, StorageConfig, SyncError, SyncKey, SyncState, Tombstone, merge};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::entry::{Entry, clean_name};
use crate::error::{CoreError, ErrorCode};
use crate::ports::LanClientConfig;
use crate::ui::{Notice, Platform, StorageView, SyncDeviceView};

/// Delay between the last change and the sync run it causes.
pub const SYNC_DEBOUNCE: Duration = Duration::from_secs(3);
/// Time between runs while the vault is unlocked, behind other windows, and after a failed run.
pub const SYNC_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// Time between runs while the app is in front.
pub const SYNC_INTERVAL_FOREGROUND: Duration = Duration::from_secs(60);
/// Time between runs on the LAN hub while the app is in front.
pub const SYNC_INTERVAL_LAN_FOREGROUND: Duration = Duration::from_secs(20);
/// Coming to the front runs sync once the last run is this old.
pub const SYNC_FOCUS_MIN: Duration = Duration::from_secs(30);
/// The longest device name kept (characters), as a snapshot carries it.
pub const MAX_DEVICE_NAME_CHARS: usize = 64;
/// The format of the payload inside a snapshot: the accounts and the deletions.
const PAYLOAD_FORMAT: u32 = 1;

/// One of a space's storages. A run takes them in this order: the LAN first, near and quick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Store {
    /// The hub's folder: on this computer for the hub, over the local network for its clients.
    Lan,
    /// The user's own storage (S3 or WebDAV).
    Cloud,
}

impl Store {
    pub const ALL: [Self; 2] = [Self::Lan, Self::Cloud];
}

/// What the runs on one storage remember.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransportLocal {
    /// This device's object there, and the other devices' as last read.
    #[serde(default)]
    pub state: SyncState,
    /// The storage holds this device's keyring: a run wrote it, or found it there, since it was
    /// sealed.
    #[serde(default)]
    pub keyring_written: bool,
    /// When a run there last finished without error, Unix milliseconds.
    #[serde(default)]
    pub last_ok_ms: Option<u64>,
}

/// The space's storage of the user's own, with its credentials.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CloudLocal {
    pub storage: StorageConfig,
    pub sync: TransportLocal,
}

/// This device's part in a sync over the local network.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub(crate) enum LanLocal {
    /// This computer keeps a copy of the space for the devices paired with it.
    Hub {
        /// The installation that took the role: a vault opened by another one leaves it.
        install_id: Uuid,
        /// The hub, as its clients know it.
        hub_id: Uuid,
        /// The port it listens on.
        port: u16,
        #[serde(default)]
        sync: TransportLocal,
    },
    /// Paired with a hub.
    Client {
        /// The installation that paired: a vault opened by another one leaves the role.
        install_id: Uuid,
        hub_id: Uuid,
        hub_name: String,
        /// This device, as the hub knows it.
        peer_id: Uuid,
        /// The key the hub gave this device (Base64 of 32 bytes).
        psk: Zeroizing<String>,
        port: u16,
        /// Where the hub was last reached.
        #[serde(default)]
        addrs: Vec<String>,
        #[serde(default)]
        sync: TransportLocal,
    },
}

impl fmt::Debug for LanLocal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hub { hub_id, port, sync, .. } => {
                f.debug_struct("Hub").field("hub_id", hub_id).field("port", port).field("sync", sync).finish_non_exhaustive()
            }
            Self::Client { hub_id, peer_id, port, addrs, sync, .. } => f
                .debug_struct("Client")
                .field("hub_id", hub_id)
                .field("peer_id", peer_id)
                .field("port", port)
                .field("addrs", addrs)
                .field("sync", sync)
                .finish_non_exhaustive(),
        }
    }
}

impl LanLocal {
    pub fn sync(&self) -> &TransportLocal {
        match self {
            Self::Hub { sync, .. } | Self::Client { sync, .. } => sync,
        }
    }

    pub fn sync_mut(&mut self) -> &mut TransportLocal {
        match self {
            Self::Hub { sync, .. } | Self::Client { sync, .. } => sync,
        }
    }

    pub fn install_id(&self) -> Uuid {
        match self {
            Self::Hub { install_id, .. } | Self::Client { install_id, .. } => *install_id,
        }
    }

    /// Where the runs find the space's copy.
    fn store_key(&self) -> Option<StoreKey> {
        match self {
            Self::Hub { hub_id, .. } => Some(StoreKey::Hub { hub_id: *hub_id }),
            Self::Client { hub_id, peer_id, psk, port, addrs, .. } => {
                let psk = Zeroizing::new(BASE64.decode(psk.as_bytes()).ok()?);
                Some(StoreKey::Client(LanClientConfig { hub_id: *hub_id, peer_id: *peer_id, psk, port: *port, addrs: addrs.clone() }))
            }
        }
    }

    /// A role this version can use: a client's key is 32 bytes. Anything else is left out, the
    /// space kept.
    fn usable(&self) -> bool {
        match self {
            Self::Hub { .. } => true,
            Self::Client { psk, .. } => BASE64.decode(psk.as_bytes()).is_ok_and(|key| key.len() == 32),
        }
    }
}

/// What names one storage of the space: a run that finds another there by the time it writes
/// stops, and an opened storage is kept for the runs while its key stays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StoreKey {
    Cloud(StorageConfig),
    Hub { hub_id: Uuid },
    Client(LanClientConfig),
}

impl StoreKey {
    /// The same storage: a LAN client's hub found at other addresses is still its hub.
    pub fn same_store(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Client(a), Self::Client(b)) => (a.hub_id, a.peer_id) == (b.hub_id, b.peer_id),
            _ => self == other,
        }
    }

    /// The prefix the space lives under there.
    pub fn prefix(&self) -> &str {
        match self {
            Self::Cloud(storage) => storage.prefix(),
            Self::Hub { .. } | Self::Client(_) => "",
        }
    }
}

/// This device's sync space, in the vault's local part. Kept in the shape earlier versions read
/// for the cloud storage (`storage`, `state`, `keyring_written`, `last_sync_ms`), the LAN role
/// beside it: an earlier version goes on syncing with the cloud storage and leaves the LAN out.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SyncLocal {
    /// The storage of the user's own.
    pub cloud: CloudLocal,
    /// The space.
    pub space_id: Uuid,
    /// The space's data key (Base64).
    pub data_key: Zeroizing<String>,
    /// The space's sync key (`LKS1-…`), for invitations.
    pub sync_key: Zeroizing<String>,
    /// This device's name in the space.
    pub device_name: String,
    /// This device's keyring: the data key under this vault's master password and the sync key
    /// (Base64). Its snapshots carry it; a new master password seals it again. A space kept
    /// without one, or with an empty one, does not read as a space: the vault leaves it out
    /// (`VaultData::from_bytes`), and no snapshot ever goes out without a keyring.
    pub keyring: String,
    /// The sync key was saved or written down. Only the device that made the space starts
    /// without; spaces kept by earlier versions count as saved (they showed the key when made).
    pub key_saved: bool,
    /// This device's part in a sync over the local network.
    pub lan: Option<LanLocal>,
}

/// [`SyncLocal`] as the vault file holds it.
#[derive(Serialize)]
struct SyncLocalOut<'a> {
    storage: &'a StorageConfig,
    space_id: Uuid,
    data_key: &'a str,
    sync_key: &'a str,
    device_name: &'a str,
    state: &'a SyncState,
    keyring: &'a str,
    keyring_written: bool,
    last_sync_ms: Option<u64>,
    key_saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    lan: Option<&'a LanLocal>,
}

#[derive(Deserialize)]
struct SyncLocalIn {
    storage: StorageConfig,
    space_id: Uuid,
    data_key: Zeroizing<String>,
    sync_key: Zeroizing<String>,
    device_name: String,
    #[serde(default)]
    state: SyncState,
    #[serde(deserialize_with = "keyring_text")]
    keyring: String,
    #[serde(default)]
    keyring_written: bool,
    #[serde(default)]
    last_sync_ms: Option<u64>,
    #[serde(default = "saved")]
    key_saved: bool,
    #[serde(default)]
    lan: Option<LanLocal>,
}

impl Serialize for SyncLocal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let CloudLocal { storage, sync } = &self.cloud;
        SyncLocalOut {
            storage,
            space_id: self.space_id,
            data_key: &self.data_key,
            sync_key: &self.sync_key,
            device_name: &self.device_name,
            state: &sync.state,
            keyring: &self.keyring,
            keyring_written: sync.keyring_written,
            last_sync_ms: sync.last_ok_ms,
            key_saved: self.key_saved,
            lan: self.lan.as_ref(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SyncLocal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let kept = SyncLocalIn::deserialize(deserializer)?;
        Ok(Self {
            cloud: CloudLocal {
                storage: kept.storage,
                sync: TransportLocal { state: kept.state, keyring_written: kept.keyring_written, last_ok_ms: kept.last_sync_ms },
            },
            space_id: kept.space_id,
            data_key: kept.data_key,
            sync_key: kept.sync_key,
            device_name: kept.device_name,
            keyring: kept.keyring,
            key_saved: kept.key_saved,
            lan: kept.lan.filter(LanLocal::usable),
        })
    }
}

fn saved() -> bool {
    true
}

impl fmt::Debug for SyncLocal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncLocal")
            .field("storage", &self.cloud.storage)
            .field("cloud", &self.cloud.sync)
            .field("space_id", &self.space_id)
            .field("device_name", &self.device_name)
            .field("key_saved", &self.key_saved)
            .field("lan", &self.lan)
            .finish_non_exhaustive()
    }
}

impl SyncLocal {
    /// A space just created or joined, with this device's `keyring`.
    pub fn new(storage: StorageConfig, keys: &SpaceKeys, sync_key: &SyncKey, device_name: String, keyring: &[u8]) -> Self {
        Self {
            cloud: CloudLocal { storage, sync: TransportLocal::default() },
            space_id: keys.space_id(),
            data_key: keys.data_key_text(),
            sync_key: sync_key.to_text(),
            device_name,
            keyring: BASE64.encode(keyring),
            key_saved: true,
            lan: None,
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

    /// The storages this device syncs with, in the order a run takes them.
    pub fn stores(&self) -> impl Iterator<Item = Store> + '_ {
        Store::ALL.into_iter().filter(|store| self.transport(*store).is_some())
    }

    pub fn transport(&self, store: Store) -> Option<&TransportLocal> {
        match store {
            Store::Cloud => Some(&self.cloud.sync),
            Store::Lan => self.lan.as_ref().map(LanLocal::sync),
        }
    }

    pub fn transport_mut(&mut self, store: Store) -> Option<&mut TransportLocal> {
        match store {
            Store::Cloud => Some(&mut self.cloud.sync),
            Store::Lan => self.lan.as_mut().map(LanLocal::sync_mut),
        }
    }

    pub fn store_key(&self, store: Store) -> Option<StoreKey> {
        match store {
            Store::Cloud => Some(StoreKey::Cloud(self.cloud.storage.clone())),
            Store::Lan => self.lan.as_ref().and_then(LanLocal::store_key),
        }
    }

    /// The highest number this device gave a write on any of its storages, or meant to: the
    /// next write anywhere takes a higher one.
    pub fn seq_floor(&self) -> u64 {
        Store::ALL
            .into_iter()
            .filter_map(|store| self.transport(store))
            .map(|t| t.state.own_seq.max(t.state.pending.as_ref().map_or(0, |p| p.seq)))
            .max()
            .unwrap_or(0)
    }

    /// When a run last finished without error on any storage.
    pub fn last_sync_ms(&self) -> Option<u64> {
        self.stores().filter_map(|store| self.transport(store).and_then(|t| t.last_ok_ms)).max()
    }

    /// A new master password's keyring is still to reach a storage: this device's snapshot there
    /// carries the one before.
    pub fn keyring_pending(&self) -> bool {
        self.stores().filter_map(|store| self.transport(store)).any(|t| t.state.own_seq > 0 && !t.keyring_written)
    }

    /// Every storage is to receive a new keyring.
    pub fn keyring_sealed_again(&mut self) {
        for store in Store::ALL {
            if let Some(transport) = self.transport_mut(store) {
                transport.keyring_written = false;
            }
        }
    }

    /// Leave the LAN role taken by another installation (the vault was copied there): its pairing
    /// is that installation's. Whether there was one to leave.
    pub fn leave_lan_of_other_install(&mut self, install_id: Uuid) -> bool {
        if self.lan.as_ref().is_some_and(|lan| lan.install_id() != install_id) {
            self.lan = None;
            return true;
        }
        false
    }

    /// The devices as the last runs found them on any storage, this one (`own_tag`) first, the
    /// others by name; a device read on two storages shows its latest snapshot.
    pub fn devices(&self, own_tag: String) -> Vec<SyncDeviceView> {
        let mut latest: BTreeMap<&str, &Seen> = BTreeMap::new();
        let mut own_written_at_ms = None;
        for transport in self.stores().filter_map(|store| self.transport(store)) {
            own_written_at_ms = own_written_at_ms.max(transport.state.own_written_at_ms);
            for (tag, seen) in &transport.state.seen {
                let newer = latest.get(tag.as_str()).is_none_or(|kept| (seen.seq, seen.written_at_ms) > (kept.seq, kept.written_at_ms));
                if newer {
                    latest.insert(tag, seen);
                }
            }
        }
        let mut others: Vec<SyncDeviceView> = latest
            .into_iter()
            .map(|(tag, seen)| SyncDeviceView { tag: tag.to_owned(), name: seen.name.clone(), written_at_ms: Some(seen.written_at_ms), this_device: false })
            .collect();
        others.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.tag.cmp(&b.tag)));
        let mut devices = vec![SyncDeviceView { tag: own_tag, name: self.device_name.clone(), written_at_ms: own_written_at_ms, this_device: true }];
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

/// What a run took into this device's accounts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Brought {
    pub added: u32,
    pub updated: u32,
    pub removed: u32,
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
        }
    }

    /// Whether the run changed any account here.
    pub(crate) fn any(self) -> bool {
        self != Self::default()
    }

    /// The notice for it, naming the devices it came from.
    pub(crate) fn notice(self, devices: Vec<String>) -> Notice {
        Notice::SyncBrought { added: self.added, updated: self.updated, removed: self.removed, devices }
    }
}

impl AddAssign for Brought {
    fn add_assign(&mut self, other: Self) {
        self.added = self.added.saturating_add(other.added);
        self.updated = self.updated.saturating_add(other.updated);
        self.removed = self.removed.saturating_add(other.removed);
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
    fn what_a_merge_brought_counts_new_changed_and_gone_accounts() {
        let a = entry("otpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP", 10, 1);
        let b = entry("otpauth://totp/B:b?secret=GEZDGNBV", 10, 1);
        let c = entry("otpauth://totp/C:c?secret=MFRGGZDF", 10, 1);
        let d = entry("otpauth://totp/D:d?secret=MZXW6YTB", 10, 1);
        let mut renamed = b.clone();
        renamed.issuer = "B2".into();
        let before = [a.clone(), b, c];
        let brought = Brought::between(&before, &[a.clone(), renamed, d]);
        assert_eq!(brought, Brought { added: 1, updated: 1, removed: 1 });
        assert!(brought.any());
        assert!(!Brought::between(&before, &before).any());

        let mut total = Brought::default();
        total += brought;
        total += Brought { added: 2, updated: 0, removed: u32::MAX };
        assert_eq!(total, Brought { added: 3, updated: 1, removed: u32::MAX });
        assert_eq!(total.notice(vec!["Phone".into()]), Notice::SyncBrought { added: 3, updated: 1, removed: u32::MAX, devices: vec!["Phone".into()] });
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

    fn laptop_space() -> SyncLocal {
        let storage = StorageConfig::Webdav {
            url: "https://dav.example.com/".into(),
            prefix: String::new(),
            username: "me".into(),
            password: Zeroizing::new("pw".into()),
        };
        let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
        SyncLocal::new(storage, &keys, &SyncKey::generate().unwrap(), "Laptop".into(), b"keyring")
    }

    fn client_role(key: &[u8]) -> LanLocal {
        LanLocal::Client {
            install_id: Uuid::new_v4(),
            hub_id: Uuid::new_v4(),
            hub_name: "Desktop".into(),
            peer_id: Uuid::new_v4(),
            psk: Zeroizing::new(BASE64.encode(key)),
            port: 47_100,
            addrs: vec!["192.168.1.20".into()],
            sync: TransportLocal::default(),
        }
    }

    #[test]
    fn a_space_keeps_the_cloud_where_earlier_versions_read_it_and_the_lan_beside() {
        let mut kept = laptop_space();
        kept.cloud.sync.state.own_seq = 4;
        kept.cloud.sync.keyring_written = true;
        kept.cloud.sync.last_ok_ms = Some(1_000);
        // Without a LAN role: the fields earlier versions write, and nothing more.
        let whole = serde_json::to_value(&kept).unwrap();
        let mut names: Vec<&str> = whole.as_object().unwrap().keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["data_key", "device_name", "key_saved", "keyring", "keyring_written", "last_sync_ms", "space_id", "state", "storage", "sync_key"]);
        assert_eq!(
            (&whole["state"]["own_seq"], &whole["keyring_written"], &whole["last_sync_ms"]),
            (&serde_json::json!(4), &serde_json::json!(true), &serde_json::json!(1000))
        );
        assert_eq!(serde_json::from_value::<SyncLocal>(whole).unwrap(), kept);
        // With one, beside them: an earlier version syncs on with the cloud storage alone.
        kept.lan = Some(LanLocal::Hub { install_id: Uuid::new_v4(), hub_id: Uuid::new_v4(), port: 47_100, sync: TransportLocal::default() });
        let whole = serde_json::to_value(&kept).unwrap();
        assert_eq!((&whole["lan"]["role"], &whole["state"]["own_seq"]), (&serde_json::json!("hub"), &serde_json::json!(4)));
        assert_eq!(serde_json::from_value::<SyncLocal>(whole).unwrap(), kept);
    }

    #[test]
    fn a_lan_role_this_version_cannot_use_is_left_out_not_the_space() {
        let mut kept = laptop_space();
        kept.lan = Some(client_role(&[7; 32]));
        let back: SyncLocal = serde_json::from_value(serde_json::to_value(&kept).unwrap()).unwrap();
        assert_eq!(back, kept);
        assert!(back.store_key(Store::Lan).is_some());
        // Its key is not one a hub gives: the role goes, the space and its cloud storage stay.
        kept.lan = Some(client_role(&[7; 16]));
        let back: SyncLocal = serde_json::from_value(serde_json::to_value(&kept).unwrap()).unwrap();
        assert!(back.lan.is_none());
        assert!(back.cloud == kept.cloud && back.space_id == kept.space_id);
        // Nor does the key show in what is logged.
        let LanLocal::Client { psk, .. } = client_role(&[9; 32]) else { unreachable!() };
        let mut logged = laptop_space();
        logged.lan = Some(client_role(&[9; 32]));
        assert!(!format!("{logged:?}").contains(psk.as_str()));
    }

    #[test]
    fn the_numbers_a_device_gave_on_any_storage_set_its_floor() {
        let mut kept = laptop_space();
        assert_eq!(kept.seq_floor(), 0);
        kept.cloud.sync.state.own_seq = 3;
        let mut lan = client_role(&[7; 32]);
        lan.sync_mut().state.pending = Some(lockra_sync::PendingWrite { seq: 6, digest: "d".into() });
        kept.lan = Some(lan);
        assert_eq!(kept.seq_floor(), 6, "a write whose answer never came counts");
        // Every storage is to receive a new keyring.
        kept.cloud.sync.keyring_written = true;
        kept.keyring_sealed_again();
        assert!(kept.keyring_pending());
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
