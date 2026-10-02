//! One sync run: fold the other devices' snapshots into the replica, then write this device's own
//! snapshot when the replica holds something the storage does not have yet.
//!
//! Every device writes only its own object, so runs on different devices never write the same
//! object and no conditional write is needed. Where the storage honours conditions (S3), this
//! device's own writes still carry one, which catches a copied vault writing under the same name;
//! elsewhere the copy shows up as a snapshot of this device's that this device did not write.

use std::collections::BTreeMap;

use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::keys::is_tag;
use crate::object::device_name;
use crate::{ObjectMeta, PutCondition, RemoteStore, Snapshot, SpaceKeys, SyncError, open_snapshot, seal_snapshot};

const ROOT: &str = "lockra-sync-v1";
const EXTENSION: &str = ".lks";

/// `prefix` as a directory: empty, or ending in `/`, never starting with one.
fn base(prefix: &str) -> String {
    let trimmed = prefix.trim_matches('/');
    if trimmed.is_empty() { String::new() } else { format!("{trimmed}/") }
}

/// The directory the spaces under `prefix` live in.
pub fn spaces_dir(prefix: &str) -> String {
    format!("{}{ROOT}/", base(prefix))
}

/// Where the keyring object of space `space_id` lives under `prefix`.
pub fn keyring_path(prefix: &str, space_id: Uuid) -> String {
    format!("{}{ROOT}/{space_id}/keyring{EXTENSION}", base(prefix))
}

/// The directory of the space's device snapshots.
pub fn devices_dir(prefix: &str, space_id: Uuid) -> String {
    format!("{}{ROOT}/{space_id}/devices/", base(prefix))
}

/// Where the snapshot of device `tag` lives.
pub fn device_path(prefix: &str, space_id: Uuid, tag: &str) -> String {
    format!("{}{tag}{EXTENSION}", devices_dir(prefix, space_id))
}

/// What a device remembers about its space between runs; the vault keeps it in its encrypted
/// local part.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncState {
    /// The sequence number of this device's last write.
    pub own_seq: u64,
    /// The etag of this device's object as last written or read.
    pub own_etag: Option<String>,
    /// SHA-256 (hex) of the device name and payload last written: an unchanged replica is not
    /// written again.
    pub written_digest: Option<String>,
    /// When this device last wrote, Unix milliseconds.
    pub own_written_at_ms: Option<u64>,
    /// A write this device started but has no answer for yet: found on the storage later, it is
    /// this device's own, not a copy's.
    pub pending: Option<PendingWrite>,
    /// The other devices, by tag.
    pub seen: BTreeMap<String, Seen>,
}

/// A write of this device's snapshot, named before it goes out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingWrite {
    /// Its sequence number.
    pub seq: u64,
    /// The digest of its device name and payload.
    pub digest: String,
}

/// The latest snapshot read from another device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    /// Its sequence number: an older one later is a rollback.
    pub seq: u64,
    /// Its etag: the same etag again is not read again.
    pub etag: Option<String>,
    /// The device's name.
    pub name: String,
    /// When the device wrote it, Unix milliseconds.
    pub written_at_ms: u64,
}

/// The data a space keeps in step.
pub trait Replica {
    /// What this device writes: its whole state as it syncs.
    fn payload(&self) -> Zeroizing<Vec<u8>>;
    /// Fold another device's payload in; `true` when the replica changed. A payload that does not
    /// parse is an error, and that device's snapshot counts as unreadable.
    fn absorb(&mut self, payload: &[u8]) -> Result<bool, SyncError>;
}

/// The space and this device in it.
#[derive(Debug, Clone, Copy)]
pub struct Space<'a> {
    /// The storage prefix the space lives under.
    pub prefix: &'a str,
    /// The space's keys.
    pub keys: &'a SpaceKeys,
    /// This device's number.
    pub device: u64,
    /// This device's name, as the other devices list it.
    pub device_name: &'a str,
}

/// A device of the space, as a run found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceView {
    /// Its tag (the name of its object).
    pub tag: String,
    /// Its name.
    pub name: String,
    /// When it last wrote, Unix milliseconds.
    pub written_at_ms: Option<u64>,
    /// This device.
    pub this_device: bool,
}

/// What a run did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The replica took something in.
    pub changed: bool,
    /// This device's snapshot was written.
    pub wrote: bool,
    /// Devices whose snapshot went back in sequence: refused, the newer state stays.
    pub rolled_back: Vec<String>,
    /// Devices whose snapshot does not open (altered, moved, another key or a newer format).
    pub unreadable: Vec<String>,
    /// The devices listed now, this one first.
    pub devices: Vec<DeviceView>,
}

/// Keeps a run's progress before it writes: called with the state naming the write
/// ([`SyncState::pending`]) and the replica as it will be written, both folded in from the other
/// devices. The caller stores them, so a write whose answer never arrives (a lost connection, a
/// vault locked meanwhile) is recognised as this device's own on the next run. An error stops the
/// run before the write.
pub type Persist<'a, R> = dyn FnMut(&SyncState, &R) -> Result<(), SyncError> + Send + 'a;

/// One run for `space`: read what changed, fold it into `replica`, write this device's snapshot
/// if needed. `state` is updated only as far as the run got: a failure leaves it consistent for
/// the next run. Without a [`Persist`] step: [`step_with`] is the one for state that outlives the
/// process.
pub async fn step<R: Replica + Send + ?Sized>(
    remote: &dyn RemoteStore,
    space: &Space<'_>,
    state: &mut SyncState,
    replica: &mut R,
    now_ms: u64,
) -> Result<Outcome, SyncError> {
    step_with(remote, space, state, replica, now_ms, &mut |_: &SyncState, _: &R| Ok(())).await
}

/// [`step`], keeping its progress through `persist` before it writes.
pub async fn step_with<R: Replica + Send + ?Sized>(
    remote: &dyn RemoteStore,
    space: &Space<'_>,
    state: &mut SyncState,
    replica: &mut R,
    now_ms: u64,
    persist: &mut Persist<'_, R>,
) -> Result<Outcome, SyncError> {
    let keys = space.keys;
    let own_tag = keys.device_tag(space.device);
    let dir = devices_dir(space.prefix, keys.space_id());
    let own_path = format!("{dir}{own_tag}{EXTENSION}");
    let listing = remote.list(&dir).await?;
    let mut outcome = Outcome::default();
    let mut own_meta: Option<ObjectMeta> = None;
    let mut others: Vec<String> = Vec::new();
    for meta in listing {
        let Some(tag) = meta.name.strip_suffix(EXTENSION).filter(|t| is_tag(t)).map(str::to_owned) else { continue };
        if tag == own_tag {
            own_meta = Some(meta);
            continue;
        }
        others.push(tag.clone());
        if state.seen.get(&tag).is_some_and(|seen| seen.etag.is_some() && seen.etag == meta.etag) {
            continue;
        }
        let Some((bytes, etag)) = remote.get(&format!("{dir}{}", meta.name)).await? else { continue };
        let snapshot = match open_snapshot(keys, &tag, &bytes) {
            Ok(snapshot) => snapshot,
            Err(SyncError::Corrupted | SyncError::Misplaced | SyncError::NotLockra | SyncError::Unsupported(_)) => {
                outcome.unreadable.push(tag);
                continue;
            }
            Err(other) => return Err(other),
        };
        match state.seen.get(&tag) {
            Some(seen) if snapshot.seq < seen.seq => {
                outcome.rolled_back.push(tag);
                continue;
            }
            // The same write under a new etag: nothing new to fold in.
            Some(seen) if snapshot.seq == seen.seq => {}
            _ => match replica.absorb(&snapshot.payload) {
                Ok(changed) => outcome.changed |= changed,
                Err(_) => {
                    outcome.unreadable.push(tag);
                    continue;
                }
            },
        }
        state.seen.insert(tag, Seen { seq: snapshot.seq, etag: etag.or(meta.etag), name: snapshot.device_name, written_at_ms: snapshot.written_at_ms });
    }
    // A device whose snapshot is gone (removed) is no longer one of the space's.
    state.seen.retain(|tag, _| others.contains(tag));

    match &own_meta {
        // Absent: the first run, or the storage lost it. Write it.
        None => {
            state.own_etag = None;
            state.written_digest = None;
        }
        // Not what this device last wrote: another device writing under its name, or the storage
        // went back.
        Some(meta) if meta.etag.is_none() || meta.etag != state.own_etag => {
            if let Some((bytes, etag)) = remote.get(&own_path).await? {
                match open_snapshot(keys, &own_tag, &bytes) {
                    // This device's own write, whose answer never arrived.
                    Ok(snapshot)
                        if state.pending.as_ref().is_some_and(|p| p.seq == snapshot.seq && p.digest == digest(&snapshot.device_name, &snapshot.payload)) =>
                    {
                        state.own_seq = snapshot.seq;
                        state.written_digest = state.pending.take().map(|p| p.digest);
                        state.own_written_at_ms = Some(snapshot.written_at_ms);
                    }
                    Ok(snapshot) if snapshot.seq > state.own_seq => return Err(SyncError::DeviceClash),
                    Ok(snapshot) if snapshot.seq == state.own_seq && state.own_seq > 0 => {
                        if Some(digest(&snapshot.device_name, &snapshot.payload)) != state.written_digest {
                            return Err(SyncError::DeviceClash);
                        }
                    }
                    _ => state.written_digest = None,
                }
                state.own_etag = etag.or_else(|| meta.etag.clone());
            }
        }
        Some(_) => {}
    }

    let name = device_name(space.device_name);
    let payload = replica.payload();
    let payload_digest = digest(&name, &payload);
    if state.written_digest.as_deref() != Some(payload_digest.as_str()) {
        let seq = state.own_seq + 1;
        let object = seal_snapshot(keys, &own_tag, &Snapshot { seq, written_at_ms: now_ms, device_name: name.clone(), payload })?;
        state.pending = Some(PendingWrite { seq, digest: payload_digest.clone() });
        persist(state, replica)?;
        let condition = match (&own_meta, &state.own_etag) {
            _ if !remote.conditional_puts() => PutCondition::Always,
            (None, _) => PutCondition::IfAbsent,
            (Some(_), Some(etag)) => PutCondition::IfMatch(etag.clone()),
            (Some(_), None) => PutCondition::Always,
        };
        let etag = match remote.put(&own_path, object, condition).await {
            Err(SyncError::Conflict) => return Err(SyncError::DeviceClash),
            other => other?,
        };
        state.own_seq = seq;
        state.own_etag = etag;
        state.written_digest = Some(payload_digest);
        state.own_written_at_ms = Some(now_ms);
        state.pending = None;
        outcome.wrote = true;
    }

    outcome.devices.push(DeviceView { tag: own_tag, name, written_at_ms: state.own_written_at_ms, this_device: true });
    for tag in others {
        if let Some(seen) = state.seen.get(&tag) {
            outcome.devices.push(DeviceView { name: seen.name.clone(), written_at_ms: Some(seen.written_at_ms), tag, this_device: false });
        }
    }
    Ok(outcome)
}

/// Remove device `tag`'s snapshot (a lost or retired device): it stops being listed. A device that
/// is still in use writes its snapshot again on its next run.
pub async fn remove_device(remote: &dyn RemoteStore, space: &Space<'_>, state: &mut SyncState, tag: &str) -> Result<(), SyncError> {
    if tag == space.keys.device_tag(space.device) || !is_tag(tag) {
        return Err(SyncError::Misplaced);
    }
    remote.delete(&device_path(space.prefix, space.keys.space_id(), tag)).await?;
    state.seen.remove(tag);
    Ok(())
}

/// What tells two writes of this device apart: its name and its payload.
fn digest(name: &str, payload: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(u64::try_from(name.len()).unwrap_or(u64::MAX).to_le_bytes());
    hash.update(name.as_bytes());
    hash.update(payload);
    HEXLOWER.encode(&hash.finalize())
}

#[cfg(test)]
mod tests {
    use lockra_vault::KdfCost;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{Hlc, MemoryRemote, Record, SyncKey, Tombstone, merge, open_keyring, seal_keyring};

    const PREFIX: &str = "/backups/lockra/";
    const PASSWORD: &[u8] = b"correct horse battery";

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Item {
        id: Uuid,
        stamp: Hlc,
        value: String,
    }

    impl Record for Item {
        fn id(&self) -> Uuid {
            self.id
        }

        fn stamp(&self) -> Hlc {
            self.stamp
        }
    }

    #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
    struct Doc {
        items: Vec<Item>,
        tombstones: Vec<Tombstone>,
    }

    impl Replica for Doc {
        fn payload(&self) -> Zeroizing<Vec<u8>> {
            Zeroizing::new(serde_json::to_vec(self).unwrap())
        }

        fn absorb(&mut self, payload: &[u8]) -> Result<bool, SyncError> {
            let theirs: Doc = serde_json::from_slice(payload).map_err(|_| SyncError::Corrupted)?;
            Ok(merge(&mut self.items, &mut self.tombstones, &theirs.items, &theirs.tombstones))
        }
    }

    impl Doc {
        fn put(&mut self, n: u8, value: &str, wall_ms: u64, device: u64) {
            let id = Uuid::from_bytes([n; 16]);
            self.items.retain(|i| i.id != id);
            self.items.push(Item { id, stamp: Hlc { wall_ms, counter: 0, device }, value: value.into() });
        }

        fn delete(&mut self, n: u8, wall_ms: u64, device: u64) {
            let id = Uuid::from_bytes([n; 16]);
            self.items.retain(|i| i.id != id);
            self.tombstones.push(Tombstone { id, stamp: Hlc { wall_ms, counter: 0, device } });
        }

        fn values(&self) -> Vec<String> {
            let mut values: Vec<String> = self.items.iter().map(|i| i.value.clone()).collect();
            values.sort();
            values
        }
    }

    /// A device of a space: its number, its name, what it remembers and its replica.
    struct Device {
        number: u64,
        name: &'static str,
        state: SyncState,
        doc: Doc,
    }

    impl Device {
        fn new(number: u64, name: &'static str) -> Self {
            Self { number, name, state: SyncState::default(), doc: Doc::default() }
        }

        async fn sync(&mut self, remote: &dyn RemoteStore, keys: &SpaceKeys, now_ms: u64) -> Result<Outcome, SyncError> {
            let space = Space { prefix: PREFIX, keys, device: self.number, device_name: self.name };
            step(remote, &space, &mut self.state, &mut self.doc, now_ms).await
        }

        fn path(&self, keys: &SpaceKeys) -> String {
            device_path(PREFIX, keys.space_id(), &keys.device_tag(self.number))
        }
    }

    fn keys() -> SpaceKeys {
        SpaceKeys::generate(Uuid::new_v4()).unwrap()
    }

    #[test]
    fn objects_live_under_the_prefix_and_the_space() {
        let id = Uuid::nil();
        assert_eq!(keyring_path("/a/b/", id), format!("a/b/lockra-sync-v1/{id}/keyring.lks"));
        assert_eq!(keyring_path("", id), format!("lockra-sync-v1/{id}/keyring.lks"));
        assert_eq!(devices_dir("x", id), format!("x/lockra-sync-v1/{id}/devices/"));
        assert_eq!(device_path("x", id, "ab"), format!("x/lockra-sync-v1/{id}/devices/ab.lks"));
    }

    #[tokio::test]
    async fn two_devices_converge_and_a_quiet_run_writes_nothing() {
        for conditional in [true, false] {
            let remote = MemoryRemote::new(conditional);
            let keys = keys();
            let mut laptop = Device::new(1, "Laptop");
            let mut phone = Device::new(2, "Phone");
            laptop.doc.put(1, "GitHub", 10, 1);
            let first = laptop.sync(&remote, &keys, 100).await.unwrap();
            assert!(first.wrote && !first.changed);
            assert_eq!(first.devices.len(), 1);

            let joined = phone.sync(&remote, &keys, 200).await.unwrap();
            assert!(joined.changed && joined.wrote, "{joined:?}");
            assert_eq!(phone.doc.values(), ["GitHub"]);
            assert_eq!(joined.devices.iter().map(|d| (d.name.as_str(), d.this_device)).collect::<Vec<_>>(), [("Phone", true), ("Laptop", false)]);

            phone.doc.delete(1, 300, 2);
            phone.doc.put(2, "Mail", 300, 2);
            phone.sync(&remote, &keys, 300).await.unwrap();
            let back = laptop.sync(&remote, &keys, 400).await.unwrap();
            assert!(back.changed);
            assert_eq!(laptop.doc.values(), ["Mail"], "the deletion and the new account arrived");
            assert_eq!(laptop.doc, phone.doc, "conditional={conditional}");
            // The phone reads the laptop's merged snapshot: it holds nothing new, so nothing is written.
            assert!(!phone.sync(&remote, &keys, 450).await.unwrap().wrote);

            // Nothing new: both runs read nothing again and write nothing.
            let calls = remote.calls().len();
            assert!(!laptop.sync(&remote, &keys, 500).await.unwrap().wrote);
            assert!(!phone.sync(&remote, &keys, 500).await.unwrap().wrote);
            assert_eq!(remote.calls().len() - calls, 2, "one listing each: {:?}", &remote.calls()[calls..]);
        }
    }

    #[tokio::test]
    async fn a_rollback_of_another_devices_snapshot_is_refused() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let mut laptop = Device::new(1, "Laptop");
        let mut phone = Device::new(2, "Phone");
        laptop.doc.put(1, "v1", 10, 1);
        laptop.sync(&remote, &keys, 100).await.unwrap();
        let old = remote.object(&laptop.path(&keys)).unwrap();
        phone.sync(&remote, &keys, 150).await.unwrap();
        laptop.doc.put(1, "v2", 20, 1);
        laptop.sync(&remote, &keys, 200).await.unwrap();
        phone.sync(&remote, &keys, 250).await.unwrap();
        assert_eq!(phone.doc.values(), ["v2"]);

        // The storage serves the older snapshot again.
        remote.set_object(&laptop.path(&keys), old);
        let outcome = phone.sync(&remote, &keys, 300).await.unwrap();
        assert_eq!(outcome.rolled_back, [keys.device_tag(1)]);
        assert_eq!(phone.doc.values(), ["v2"], "the newer state stays");
        // The laptop finds its own snapshot went back and writes it again.
        let healed = laptop.sync(&remote, &keys, 400).await.unwrap();
        assert!(healed.wrote);
        assert!(phone.sync(&remote, &keys, 500).await.unwrap().rolled_back.is_empty());
    }

    #[tokio::test]
    async fn altered_moved_and_foreign_objects_are_reported_and_skipped() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let mut laptop = Device::new(1, "Laptop");
        let mut phone = Device::new(2, "Phone");
        laptop.doc.put(1, "GitHub", 10, 1);
        laptop.sync(&remote, &keys, 100).await.unwrap();
        let mut object = remote.object(&laptop.path(&keys)).unwrap();
        // Altered.
        let last = object.len() - 1;
        object[last] ^= 1;
        remote.set_object(&laptop.path(&keys), object);
        // Moved: the laptop's genuine object copied under a third device's name.
        laptop.doc.put(2, "Mail", 20, 1);
        laptop.state.written_digest = None;
        let genuine_path = laptop.path(&keys);
        laptop.sync(&remote, &keys, 150).await.unwrap();
        let third = device_path(PREFIX, keys.space_id(), &keys.device_tag(3));
        remote.set_object(&third, remote.object(&genuine_path).unwrap());
        // Foreign: an object of another space.
        let other = keys_and_object();
        remote.set_object(&device_path(PREFIX, keys.space_id(), &keys.device_tag(4)), other);
        // Not even a tag.
        remote.set_object(&format!("{}notes.txt", devices_dir(PREFIX, keys.space_id())), b"hello".to_vec());

        let outcome = phone.sync(&remote, &keys, 200).await.unwrap();
        let mut unreadable = outcome.unreadable.clone();
        unreadable.sort();
        let mut expected = vec![keys.device_tag(3), keys.device_tag(4)];
        expected.sort();
        assert_eq!(unreadable, expected);
        assert_eq!(phone.doc.values(), ["GitHub", "Mail"], "the genuine snapshot was read");
    }

    fn keys_and_object() -> Vec<u8> {
        let other = keys();
        seal_snapshot(&other, &other.device_tag(4), &Snapshot { seq: 1, written_at_ms: 1, device_name: "x".into(), payload: Zeroizing::new(b"{}".to_vec()) })
            .unwrap()
    }

    #[tokio::test]
    async fn this_devices_snapshot_comes_back_when_the_storage_loses_it() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let mut laptop = Device::new(1, "Laptop");
        laptop.doc.put(1, "GitHub", 10, 1);
        laptop.sync(&remote, &keys, 100).await.unwrap();
        remote.delete(&laptop.path(&keys)).await.unwrap();
        let outcome = laptop.sync(&remote, &keys, 200).await.unwrap();
        assert!(outcome.wrote);
        assert_eq!(laptop.state.own_seq, 2);
        assert!(remote.object(&laptop.path(&keys)).is_some());
    }

    #[tokio::test]
    async fn a_copied_vault_writing_under_the_same_name_is_a_device_clash() {
        for conditional in [true, false] {
            let remote = MemoryRemote::new(conditional);
            let keys = keys();
            let mut original = Device::new(1, "Laptop");
            original.doc.put(1, "GitHub", 10, 1);
            original.sync(&remote, &keys, 100).await.unwrap();
            // A copy of the vault, local part included, on another computer.
            let mut copy = Device { number: 1, name: "Copy", state: original.state.clone(), doc: original.doc.clone() };
            copy.doc.put(2, "Mail", 20, 1);
            copy.sync(&remote, &keys, 150).await.unwrap();
            original.doc.put(3, "Bank", 30, 1);
            assert_eq!(original.sync(&remote, &keys, 200).await.err(), Some(SyncError::DeviceClash), "conditional={conditional}");
        }
    }

    #[tokio::test]
    async fn a_fresh_device_finding_its_name_taken_is_a_device_clash() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let mut first = Device::new(1, "Laptop");
        first.sync(&remote, &keys, 100).await.unwrap();
        let mut same_number = Device::new(1, "Other");
        same_number.doc.put(1, "x", 1, 1);
        assert_eq!(same_number.sync(&remote, &keys, 200).await.err(), Some(SyncError::DeviceClash));
    }

    #[tokio::test]
    async fn storage_failures_leave_the_state_consistent() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let mut laptop = Device::new(1, "Laptop");
        laptop.doc.put(1, "GitHub", 10, 1);
        remote.fail_next(SyncError::Network("offline".into()));
        assert_eq!(laptop.sync(&remote, &keys, 100).await.err(), Some(SyncError::Network("offline".into())));
        assert_eq!(laptop.state, SyncState::default());
        // A failed run changes nothing: the next one does everything again.
        let mut phone = Device::new(2, "Phone");
        laptop.sync(&remote, &keys, 150).await.unwrap();
        phone.doc.put(2, "Mail", 20, 2);
        remote.fail_next(SyncError::Denied);
        assert_eq!(phone.sync(&remote, &keys, 200).await.err(), Some(SyncError::Denied));
        let retried = phone.sync(&remote, &keys, 300).await.unwrap();
        assert!(retried.wrote && retried.changed);
    }

    #[tokio::test]
    async fn a_payload_that_does_not_parse_is_unreadable_not_fatal() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let tag = keys.device_tag(9);
        let object =
            seal_snapshot(&keys, &tag, &Snapshot { seq: 1, written_at_ms: 1, device_name: "Future".into(), payload: Zeroizing::new(b"not json".to_vec()) })
                .unwrap();
        remote.set_object(&device_path(PREFIX, keys.space_id(), &tag), object);
        let mut laptop = Device::new(1, "Laptop");
        let outcome = laptop.sync(&remote, &keys, 100).await.unwrap();
        assert_eq!(outcome.unreadable, [tag]);
        assert!(outcome.wrote);
    }

    #[tokio::test]
    async fn a_removed_device_stops_being_listed_until_it_writes_again() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let mut laptop = Device::new(1, "Laptop");
        let mut phone = Device::new(2, "Phone");
        laptop.sync(&remote, &keys, 100).await.unwrap();
        phone.sync(&remote, &keys, 100).await.unwrap();
        laptop.sync(&remote, &keys, 150).await.unwrap();
        let space = Space { prefix: PREFIX, keys: &keys, device: 1, device_name: "Laptop" };
        let phone_tag = keys.device_tag(2);
        // Removed elsewhere: the next run forgets it too.
        let mut elsewhere = laptop.state.clone();
        remove_device(&remote, &space, &mut elsewhere, &phone_tag).await.unwrap();
        assert!(!elsewhere.seen.contains_key(&phone_tag));
        assert!(laptop.state.seen.contains_key(&phone_tag));
        assert_eq!(laptop.sync(&remote, &keys, 200).await.unwrap().devices.len(), 1);
        assert!(!laptop.state.seen.contains_key(&phone_tag));
        assert_eq!(remove_device(&remote, &space, &mut laptop.state, &keys.device_tag(1)).await.err(), Some(SyncError::Misplaced), "not this device");
        assert_eq!(remove_device(&remote, &space, &mut laptop.state, "../keyring").await.err(), Some(SyncError::Misplaced));
        // The phone is still in use: its next run lists it again.
        phone.doc.put(1, "x", 1, 2);
        phone.sync(&remote, &keys, 300).await.unwrap();
        assert_eq!(laptop.sync(&remote, &keys, 400).await.unwrap().devices.len(), 2);
    }

    #[tokio::test]
    async fn a_device_joins_with_the_keyring_the_master_password_and_the_sync_key() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let sync_key = SyncKey::generate().unwrap();
        let path = keyring_path(PREFIX, keys.space_id());
        let keyring = seal_keyring(&keys, &sync_key, PASSWORD, KdfCost::FAST_INSECURE, 1).unwrap();
        remote.put(&path, keyring.clone(), PutCondition::IfAbsent).await.unwrap();
        assert_eq!(remote.put(&path, keyring, PutCondition::IfAbsent).await.err(), Some(SyncError::Conflict), "a space is never overwritten");
        let mut laptop = Device::new(1, "Laptop");
        laptop.doc.put(1, "GitHub", 10, 1);
        laptop.sync(&remote, &keys, 100).await.unwrap();

        // The phone knows the space id, the sync key and the master password; nothing else.
        let (bytes, _) = remote.get(&path).await.unwrap().unwrap();
        let joined = open_keyring(&bytes, keys.space_id(), &SyncKey::from_text(&sync_key.to_text()).unwrap(), PASSWORD).unwrap();
        let mut phone = Device::new(2, "Phone");
        phone.sync(&remote, &joined, 200).await.unwrap();
        assert_eq!(phone.doc.values(), ["GitHub"]);
        // Nothing on the storage reads as an account name or the device names.
        for path in remote.paths() {
            let text = String::from_utf8_lossy(&remote.object(&path).unwrap()).into_owned();
            assert!(!text.contains("GitHub") && !text.contains("Laptop") && !text.contains("Phone"), "{path}: {text}");
        }
    }

    #[tokio::test]
    async fn the_progress_is_kept_before_the_write_and_a_refusal_stops_it() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let mut phone = Device::new(2, "Phone");
        phone.doc.put(2, "Mail", 20, 2);
        phone.sync(&remote, &keys, 100).await.unwrap();
        let mut laptop = Device::new(1, "Laptop");
        laptop.doc.put(1, "GitHub", 10, 1);
        let space = Space { prefix: PREFIX, keys: &keys, device: 1, device_name: "Laptop" };
        let mut kept: Vec<(SyncState, Doc)> = Vec::new();
        let mut keep = |state: &SyncState, doc: &Doc| {
            kept.push((state.clone(), doc.clone()));
            Ok(())
        };
        step_with(&remote, &space, &mut laptop.state, &mut laptop.doc, 200, &mut keep).await.unwrap();
        let (state, doc) = &kept[0];
        assert_eq!(state.pending.as_ref().map(|p| p.seq), Some(1), "the write is named before it goes out");
        assert!(state.seen.contains_key(&keys.device_tag(2)), "with what it read");
        assert_eq!(doc.values(), ["GitHub", "Mail"], "and the replica as it is written");
        assert_eq!(laptop.state.pending, None, "answered: nothing pending");

        // Refused: nothing is written.
        laptop.doc.put(3, "Bank", 30, 1);
        let before = remote.object(&laptop.path(&keys));
        let mut refuse = |_: &SyncState, _: &Doc| Err(SyncError::Interrupted);
        let refused = step_with(&remote, &space, &mut laptop.state, &mut laptop.doc, 300, &mut refuse).await;
        assert_eq!(refused.err(), Some(SyncError::Interrupted));
        assert_eq!(remote.object(&laptop.path(&keys)), before);
    }

    #[tokio::test]
    async fn a_write_whose_answer_was_lost_is_this_devices_own() {
        for conditional in [true, false] {
            let remote = MemoryRemote::new(conditional);
            let keys = keys();
            let mut laptop = Device::new(1, "Laptop");
            laptop.doc.put(1, "GitHub", 10, 1);
            laptop.sync(&remote, &keys, 100).await.unwrap();
            // The next write reaches the storage, but its answer does not reach the vault: what the
            // vault holds is what the run kept before writing.
            laptop.doc.put(2, "Mail", 20, 1);
            let space = Space { prefix: PREFIX, keys: &keys, device: 1, device_name: "Laptop" };
            let mut kept: Option<SyncState> = None;
            let mut keep = |state: &SyncState, _: &Doc| {
                kept = Some(state.clone());
                Ok(())
            };
            step_with(&remote, &space, &mut laptop.state, &mut laptop.doc, 200, &mut keep).await.unwrap();
            laptop.state = kept.unwrap();
            let next = laptop.sync(&remote, &keys, 300).await.unwrap();
            assert!(!next.wrote, "the snapshot is already there: conditional={conditional}");
            assert_eq!((laptop.state.own_seq, laptop.state.pending.clone()), (2, None));
            laptop.doc.put(3, "Bank", 30, 1);
            assert!(laptop.sync(&remote, &keys, 400).await.unwrap().wrote);
        }
    }

    #[tokio::test]
    async fn a_new_device_name_is_written_even_when_nothing_else_changed() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let mut laptop = Device::new(1, "Laptop");
        laptop.sync(&remote, &keys, 100).await.unwrap();
        laptop.name = "Work laptop";
        assert!(laptop.sync(&remote, &keys, 200).await.unwrap().wrote);
        let mut phone = Device::new(2, "Phone");
        let outcome = phone.sync(&remote, &keys, 300).await.unwrap();
        assert_eq!(outcome.devices[1].name, "Work laptop");
        // A name longer than a snapshot carries does not read as another device's write.
        laptop.name = "A very long device name that goes on and on beyond the sixty-four characters";
        assert!(laptop.sync(&remote, &keys, 400).await.unwrap().wrote);
        assert!(!laptop.sync(&remote, &keys, 500).await.unwrap().wrote);
    }
}
