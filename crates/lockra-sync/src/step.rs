//! One sync run: fold the other devices' snapshots into the replica, then write this device's own
//! snapshot when the replica holds something the storage does not have yet.
//!
//! Every object of a space has one writer: each device writes only its own snapshot, its keyring
//! inside. Runs on different devices never write the same object, so no lock and no conditional
//! write is needed, and S3 and WebDAV behave alike; the merge decides what the space holds,
//! whatever order the runs take. Where the storage honours conditions (S3), this device's own
//! writes still carry one, which catches a copied vault writing under the same name sooner;
//! elsewhere the copy shows up as a snapshot of this device's that this device did not write.

use std::collections::BTreeMap;

use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::keys::is_tag;
use crate::object::device_name;
use crate::{MAX_OBJECT_BYTES, ObjectMeta, PutCondition, RemoteStore, Snapshot, SpaceKeys, SyncError, open_snapshot, seal_snapshot};

const ROOT: &str = "lockra-sync-v1";
const EXTENSION: &str = ".lks";

/// `prefix` as a directory: empty, or ending in `/`, never starting with one.
fn base(prefix: &str) -> String {
    let trimmed = prefix.trim_matches('/');
    if trimmed.is_empty() { String::new() } else { format!("{trimmed}/") }
}

/// The directory of the space's device snapshots: all the space holds.
pub fn devices_dir(prefix: &str, space_id: Uuid) -> String {
    format!("{}{ROOT}/{space_id}/devices/", base(prefix))
}

/// Where the snapshot of device `tag` lives.
pub fn device_path(prefix: &str, space_id: Uuid, tag: &str) -> String {
    format!("{}{tag}{EXTENSION}", devices_dir(prefix, space_id))
}

/// The device tag an object in [`devices_dir`] is named after, if it is named after one.
pub(crate) fn tag_of(name: &str) -> Option<String> {
    name.strip_suffix(EXTENSION).filter(|t| is_tag(t)).map(str::to_owned)
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
    /// SHA-256 (hex) of the device name, keyring and payload last written: an unchanged replica
    /// is not written again.
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
    /// Its sequence number, never given to another write.
    pub seq: u64,
    /// The digest of its device name, keyring and payload.
    pub digest: String,
}

/// The latest snapshot read from another device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    /// Its sequence number: an older one later is a rollback.
    pub seq: u64,
    /// Its etag: the same etag again is not read again; another one is, at any sequence number.
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
    /// This device's keyring ([`crate::seal_keyring`] under its master password), carried in its
    /// snapshots.
    pub keyring: &'a [u8],
    /// The highest number this device gave a write on any of its storages: a write here takes a
    /// higher one, so one number names one snapshot wherever it is.
    pub seq_floor: u64,
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
    /// The names of the devices whose snapshot changed the replica, in the order read: what
    /// this run brought, whether or not the storage gives etags.
    pub brought: Vec<String>,
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
        let Some(tag) = tag_of(&meta.name) else { continue };
        if tag == own_tag {
            own_meta = Some(meta);
            continue;
        }
        others.push(tag.clone());
        // Larger than any snapshot Lockra writes: not read at all.
        if meta.size > MAX_OBJECT_BYTES {
            outcome.unreadable.push(tag);
            continue;
        }
        if state.seen.get(&tag).is_some_and(|seen| seen.etag.is_some() && seen.etag == meta.etag) {
            continue;
        }
        let (bytes, etag) = match remote.get(&format!("{dir}{}", meta.name)).await {
            Ok(Some(found)) => found,
            Ok(None) => continue,
            // Larger than the listing said: refused by the storage's read.
            Err(SyncError::Corrupted) => {
                outcome.unreadable.push(tag);
                continue;
            }
            Err(other) => return Err(other),
        };
        let snapshot = match open_snapshot(keys, &tag, &bytes) {
            Ok(snapshot) => snapshot,
            Err(SyncError::Corrupted | SyncError::Misplaced | SyncError::NotLockra | SyncError::Unsupported(_)) => {
                outcome.unreadable.push(tag);
                continue;
            }
            Err(other) => return Err(other),
        };
        if state.seen.get(&tag).is_some_and(|seen| snapshot.seq < seen.seq) {
            outcome.rolled_back.push(tag);
            continue;
        }
        // A new etag is folded in even at a sequence number already seen: a copied vault writes
        // under the same numbers, and folding the same state in twice changes nothing.
        match replica.absorb(&snapshot.payload) {
            Ok(changed) => {
                outcome.changed |= changed;
                if changed && !outcome.brought.contains(&snapshot.device_name) {
                    outcome.brought.push(snapshot.device_name.clone());
                }
            }
            Err(_) => {
                outcome.unreadable.push(tag);
                continue;
            }
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
        // Not a snapshot of this device's, whatever it is: written again.
        Some(meta) if meta.size > MAX_OBJECT_BYTES => {
            state.own_etag = meta.etag.clone();
            state.written_digest = None;
        }
        // Not what this device last wrote: another device writing under its name, or the storage
        // went back.
        Some(meta) if meta.etag.is_none() || meta.etag != state.own_etag => {
            let found = match remote.get(&own_path).await {
                Ok(found) => found,
                Err(SyncError::Corrupted) => {
                    state.written_digest = None;
                    None
                }
                Err(other) => return Err(other),
            };
            if let Some((bytes, etag)) = found {
                match open_snapshot(keys, &own_tag, &bytes) {
                    // This device's own write, whose answer never arrived.
                    Ok(snapshot) if state.pending.as_ref().is_some_and(|p| p.seq == snapshot.seq && p.digest == snapshot_digest(&snapshot)) => {
                        state.own_seq = snapshot.seq;
                        state.written_digest = state.pending.take().map(|p| p.digest);
                        state.own_written_at_ms = Some(snapshot.written_at_ms);
                    }
                    Ok(snapshot) if snapshot.seq > state.own_seq => return Err(SyncError::DeviceClash),
                    Ok(snapshot) if snapshot.seq == state.own_seq && state.own_seq > 0 => {
                        if Some(snapshot_digest(&snapshot)) != state.written_digest {
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
    let payload_digest = digest(&name, space.keyring, &payload);
    if state.written_digest.as_deref() != Some(payload_digest.as_str()) {
        // Never a number another write had: one whose answer never came may still have landed,
        // and another device may have read it; nor one this device gave a write on another storage.
        let seq = state.own_seq.max(state.pending.as_ref().map_or(0, |p| p.seq)).max(space.seq_floor) + 1;
        let snapshot = Snapshot { seq, written_at_ms: now_ms, device_name: name.clone(), keyring: space.keyring.to_vec(), payload };
        let object = seal_snapshot(keys, &own_tag, &snapshot)?;
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

/// What tells two writes of this device apart: its name, its keyring and its payload.
fn digest(name: &str, keyring: &[u8], payload: &[u8]) -> String {
    let mut hash = Sha256::new();
    for part in [name.as_bytes(), keyring] {
        hash.update(u64::try_from(part.len()).unwrap_or(u64::MAX).to_le_bytes());
        hash.update(part);
    }
    hash.update(payload);
    HEXLOWER.encode(&hash.finalize())
}

fn snapshot_digest(snapshot: &Snapshot) -> String {
    digest(&snapshot.device_name, &snapshot.keyring, &snapshot.payload)
}

#[cfg(test)]
mod tests {
    use lockra_vault::KdfCost;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{Hlc, MemoryRemote, Record, SyncKey, Tombstone, merge, open_keyring, open_space, seal_keyring, snapshot_keyring};

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
            self.tombstones.push(Tombstone { id, stamp: Hlc { wall_ms, counter: 0, device }, counter: None });
        }

        fn values(&self) -> Vec<String> {
            let mut values: Vec<String> = self.items.iter().map(|i| i.value.clone()).collect();
            values.sort();
            values
        }
    }

    /// A device of a space: its number, its name, its keyring (opaque here), what it remembers and
    /// its replica.
    struct Device {
        number: u64,
        name: &'static str,
        keyring: Vec<u8>,
        state: SyncState,
        doc: Doc,
        /// The highest number this device wrote on any other storage.
        floor: u64,
    }

    impl Device {
        fn new(number: u64, name: &'static str) -> Self {
            Self { number, name, keyring: format!("keyring of device {number}").into_bytes(), state: SyncState::default(), doc: Doc::default(), floor: 0 }
        }

        fn space<'a>(&'a self, keys: &'a SpaceKeys) -> Space<'a> {
            Space { prefix: PREFIX, keys, device: self.number, device_name: self.name, keyring: &self.keyring, seq_floor: self.floor }
        }

        async fn sync(&mut self, remote: &dyn RemoteStore, keys: &SpaceKeys, now_ms: u64) -> Result<Outcome, SyncError> {
            let space = Space { prefix: PREFIX, keys, device: self.number, device_name: self.name, keyring: &self.keyring, seq_floor: self.floor };
            step(remote, &space, &mut self.state, &mut self.doc, now_ms).await
        }

        /// This device's snapshot as stored.
        fn stored(&self, remote: &MemoryRemote, keys: &SpaceKeys) -> Snapshot {
            open_snapshot(keys, &keys.device_tag(self.number), &remote.object(&self.path(keys)).unwrap()).unwrap()
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
        assert_eq!(devices_dir("/a/b/", id), format!("a/b/lockra-sync-v1/{id}/devices/"));
        assert_eq!(devices_dir("", id), format!("lockra-sync-v1/{id}/devices/"));
        assert_eq!(device_path("x", id, "ab"), format!("x/lockra-sync-v1/{id}/devices/ab.lks"));
        let tag = keys().device_tag(1);
        assert_eq!(tag_of(&format!("{tag}.lks")), Some(tag.clone()));
        assert_eq!(tag_of(&tag), None);
        assert_eq!(tag_of("notes.lks"), None);
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
            assert!(first.brought.is_empty());
            assert_eq!(first.devices.len(), 1);

            let joined = phone.sync(&remote, &keys, 200).await.unwrap();
            assert!(joined.changed && joined.wrote, "{joined:?}");
            assert_eq!(joined.brought, ["Laptop"], "what came in, and from whom");
            assert_eq!(phone.doc.values(), ["GitHub"]);
            assert_eq!(joined.devices.iter().map(|d| (d.name.as_str(), d.this_device)).collect::<Vec<_>>(), [("Phone", true), ("Laptop", false)]);

            phone.doc.delete(1, 300, 2);
            phone.doc.put(2, "Mail", 300, 2);
            phone.sync(&remote, &keys, 300).await.unwrap();
            let back = laptop.sync(&remote, &keys, 400).await.unwrap();
            assert!(back.changed);
            assert_eq!(back.brought, ["Phone"]);
            assert_eq!(laptop.doc.values(), ["Mail"], "the deletion and the new account arrived");
            assert_eq!(laptop.doc, phone.doc, "conditional={conditional}");
            // The phone reads the laptop's merged snapshot: it holds nothing new, so nothing is written
            // and nothing is said to have come in.
            let quiet = phone.sync(&remote, &keys, 450).await.unwrap();
            assert!(!quiet.wrote && quiet.brought.is_empty(), "{quiet:?}");

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
        let snapshot = Snapshot { seq: 1, written_at_ms: 1, device_name: "x".into(), keyring: Vec::new(), payload: Zeroizing::new(b"{}".to_vec()) };
        seal_snapshot(&other, &other.device_tag(4), &snapshot).unwrap()
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
            let mut copy =
                Device { number: 1, name: "Copy", keyring: original.keyring.clone(), state: original.state.clone(), doc: original.doc.clone(), floor: 0 };
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
        let snapshot = Snapshot { seq: 1, written_at_ms: 1, device_name: "Future".into(), keyring: Vec::new(), payload: Zeroizing::new(b"not json".to_vec()) };
        let object = seal_snapshot(&keys, &tag, &snapshot).unwrap();
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
        let space = laptop.space(&keys);
        let phone_tag = keys.device_tag(2);
        // Removed elsewhere: the next run forgets it too.
        let mut elsewhere = laptop.state.clone();
        remove_device(&remote, &space, &mut elsewhere, &phone_tag).await.unwrap();
        assert!(!elsewhere.seen.contains_key(&phone_tag));
        assert!(laptop.state.seen.contains_key(&phone_tag));
        assert_eq!(laptop.sync(&remote, &keys, 200).await.unwrap().devices.len(), 1);
        assert!(!laptop.state.seen.contains_key(&phone_tag));
        let mut state = laptop.state.clone();
        let space = laptop.space(&keys);
        assert_eq!(remove_device(&remote, &space, &mut state, &keys.device_tag(1)).await.err(), Some(SyncError::Misplaced), "not this device");
        assert_eq!(remove_device(&remote, &space, &mut state, "../devices").await.err(), Some(SyncError::Misplaced));
        // The phone is still in use: its next run lists it again.
        phone.doc.put(1, "x", 1, 2);
        phone.sync(&remote, &keys, 300).await.unwrap();
        assert_eq!(laptop.sync(&remote, &keys, 400).await.unwrap().devices.len(), 2);
    }

    async fn join(remote: &MemoryRemote, sync_key: &SyncKey, password: &'static [u8]) -> Result<SpaceKeys, SyncError> {
        let space_id = sync_key.space_id();
        open_space(remote, PREFIX, space_id, |keyring| {
            let sync_key = sync_key.clone();
            async move { open_keyring(&keyring, space_id, &sync_key, password) }
        })
        .await
    }

    #[tokio::test]
    async fn a_device_joins_with_the_sync_key_and_any_devices_master_password() {
        for conditional in [true, false] {
            let remote = MemoryRemote::new(conditional);
            let sync_key = SyncKey::generate().unwrap();
            let keys = SpaceKeys::generate(sync_key.space_id()).unwrap();
            let mut laptop = Device::new(1, "Laptop");
            laptop.keyring = seal_keyring(&keys, &sync_key, PASSWORD, KdfCost::FAST_INSECURE).unwrap();
            laptop.doc.put(1, "GitHub", 10, 1);
            laptop.sync(&remote, &keys, 100).await.unwrap();

            // The phone knows the storage, the sync key and the laptop's master password; nothing else.
            let joined = join(&remote, &SyncKey::from_text(&sync_key.to_text()).unwrap(), PASSWORD).await.unwrap();
            let mut phone = Device::new(2, "Phone");
            phone.keyring = seal_keyring(&joined, &sync_key, b"phone password", KdfCost::FAST_INSECURE).unwrap();
            phone.sync(&remote, &joined, 200).await.unwrap();
            assert_eq!(phone.doc.values(), ["GitHub"]);
            // The phone's own snapshot carries its keyring: its master password opens the space too.
            assert_eq!(phone.stored(&remote, &keys).keyring, phone.keyring);
            assert_eq!(*join(&remote, &sync_key, b"phone password").await.unwrap().data_key_text(), *keys.data_key_text());
            assert_eq!(join(&remote, &sync_key, b"a guess").await.err(), Some(SyncError::WrongCredentials));
            // Two objects, one per device, each written by its device only: nothing else is stored.
            assert_eq!(remote.paths(), {
                let mut paths = vec![laptop.path(&keys), phone.path(&keys)];
                paths.sort();
                paths
            });
            // Nothing on the storage reads as an account name or the device names.
            for path in remote.paths() {
                let text = String::from_utf8_lossy(&remote.object(&path).unwrap()).into_owned();
                assert!(!text.contains("GitHub") && !text.contains("Laptop") && !text.contains("Phone"), "{path}: {text}");
            }
        }
    }

    #[tokio::test]
    async fn a_new_keyring_is_written_even_when_nothing_else_changed() {
        let remote = MemoryRemote::new(false);
        let keys = keys();
        let mut laptop = Device::new(1, "Laptop");
        laptop.sync(&remote, &keys, 100).await.unwrap();
        assert!(!laptop.sync(&remote, &keys, 150).await.unwrap().wrote);
        // A new master password: the keyring under it goes out with the next run.
        laptop.keyring = b"keyring under the new password".to_vec();
        assert!(laptop.sync(&remote, &keys, 200).await.unwrap().wrote);
        let stored = remote.object(&laptop.path(&keys)).unwrap();
        assert_eq!(snapshot_keyring(&stored, keys.space_id(), &keys.device_tag(1)).unwrap(), laptop.keyring);
        assert!(!laptop.sync(&remote, &keys, 300).await.unwrap().wrote);
    }

    #[tokio::test]
    async fn a_new_etag_is_folded_in_even_at_a_sequence_number_already_seen() {
        let remote = MemoryRemote::new(false);
        let keys = keys();
        let mut laptop = Device::new(1, "Laptop");
        let mut phone = Device::new(2, "Phone");
        laptop.doc.put(1, "GitHub", 10, 1);
        laptop.sync(&remote, &keys, 100).await.unwrap();
        phone.sync(&remote, &keys, 150).await.unwrap();
        // Another write under the laptop's name and number (a copy of its vault, say), holding more.
        let mut copy = laptop.doc.clone();
        copy.put(2, "Mail", 20, 1);
        let tag = keys.device_tag(1);
        let seq = laptop.state.own_seq;
        let object = seal_snapshot(
            &keys,
            &tag,
            &Snapshot { seq, written_at_ms: 120, device_name: "Laptop".into(), keyring: laptop.keyring.clone(), payload: copy.payload() },
        )
        .unwrap();
        remote.set_object(&laptop.path(&keys), object);
        let outcome = phone.sync(&remote, &keys, 200).await.unwrap();
        assert!(outcome.changed && outcome.rolled_back.is_empty(), "{outcome:?}");
        assert_eq!(phone.doc.values(), ["GitHub", "Mail"]);
    }

    #[tokio::test]
    async fn a_write_that_never_landed_does_not_lend_its_number() {
        for conditional in [true, false] {
            let remote = MemoryRemote::new(conditional);
            let keys = keys();
            let mut laptop = Device::new(1, "Laptop");
            let mut phone = Device::new(2, "Phone");
            laptop.doc.put(1, "GitHub", 10, 1);
            laptop.sync(&remote, &keys, 100).await.unwrap();
            // Cut off: the write never reaches the storage, and its answer never comes.
            laptop.doc.put(2, "Mail", 20, 1);
            remote.fail_next_put(SyncError::Network("cut".into()), false);
            assert!(laptop.sync(&remote, &keys, 200).await.is_err());
            assert_eq!(laptop.state.pending.as_ref().map(|p| p.seq), Some(2));
            // The next write takes a number nobody had, whatever the first one did.
            laptop.doc.put(3, "Bank", 30, 1);
            assert!(laptop.sync(&remote, &keys, 300).await.unwrap().wrote);
            assert_eq!((laptop.stored(&remote, &keys).seq, laptop.state.own_seq, laptop.state.pending.clone()), (3, 3, None), "conditional={conditional}");
            phone.sync(&remote, &keys, 400).await.unwrap();
            assert_eq!(phone.doc.values(), ["Bank", "GitHub", "Mail"]);
        }
    }

    #[tokio::test]
    async fn a_write_numbers_itself_above_what_another_storage_reached() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let mut laptop = Device::new(1, "Laptop");
        let mut phone = Device::new(2, "Phone");
        laptop.doc.put(1, "GitHub", 10, 1);
        // On another storage this device already wrote up to 7: here it goes on from there.
        laptop.floor = 7;
        laptop.sync(&remote, &keys, 100).await.unwrap();
        assert_eq!((laptop.stored(&remote, &keys).seq, laptop.state.own_seq), (8, 8));
        // A floor below this storage's own number changes nothing.
        laptop.floor = 3;
        laptop.doc.put(2, "Mail", 20, 1);
        laptop.sync(&remote, &keys, 200).await.unwrap();
        assert_eq!(laptop.stored(&remote, &keys).seq, 9);
        // Read elsewhere, the numbers are this storage's: nothing went back.
        let read = phone.sync(&remote, &keys, 300).await.unwrap();
        assert!(read.rolled_back.is_empty() && phone.doc.values() == ["GitHub", "Mail"], "{read:?}");
    }

    #[tokio::test]
    async fn a_write_whose_answer_never_came_back_is_recognised() {
        for conditional in [true, false] {
            let remote = MemoryRemote::new(conditional);
            let keys = keys();
            let mut laptop = Device::new(1, "Laptop");
            laptop.doc.put(1, "GitHub", 10, 1);
            remote.fail_next_put(SyncError::Network("lost".into()), true);
            assert!(laptop.sync(&remote, &keys, 100).await.is_err());
            let next = laptop.sync(&remote, &keys, 200).await.unwrap();
            assert!(!next.wrote, "the snapshot is there already: conditional={conditional}");
            assert_eq!((laptop.state.own_seq, laptop.state.pending.clone()), (1, None));
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
        let keyring = laptop.keyring.clone();
        let space = Space { prefix: PREFIX, keys: &keys, device: 1, device_name: "Laptop", keyring: &keyring, seq_floor: 0 };
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
            let keyring = laptop.keyring.clone();
            let space = Space { prefix: PREFIX, keys: &keys, device: 1, device_name: "Laptop", keyring: &keyring, seq_floor: 0 };
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

    #[tokio::test]
    async fn an_object_too_large_to_be_a_snapshot_is_never_read() {
        let remote = MemoryRemote::new(true);
        let keys = keys();
        let huge = vec![0u8; usize::try_from(MAX_OBJECT_BYTES).unwrap() + 1];
        let other = keys.device_tag(9);
        remote.set_object(&device_path(PREFIX, keys.space_id(), &other), huge.clone());
        let mut laptop = Device::new(1, "Laptop");
        // This device's own name, filled with garbage: written over.
        remote.set_object(&laptop.path(&keys), huge.clone());
        laptop.doc.put(1, "GitHub", 10, 1);
        let outcome = laptop.sync(&remote, &keys, 100).await.unwrap();
        assert_eq!(outcome.unreadable, std::slice::from_ref(&other));
        assert!(outcome.wrote);
        assert!(remote.calls().iter().all(|c| !c.starts_with("get ")), "{:?}", remote.calls());
        assert!(remote.object(&laptop.path(&keys)).unwrap().len() < 64 * 1024);
        // A storage whose listing lies about the size still refuses the read.
        assert_eq!(remote.get(&device_path(PREFIX, keys.space_id(), &other)).await.err(), Some(SyncError::Corrupted));
    }
}
