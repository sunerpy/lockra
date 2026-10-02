//! Finding a space in its storage: a device joining it opens the data key from one of the
//! keyrings its devices' snapshots carry; a device moving it to new storage settings checks that
//! its snapshots are there. Both only read.

use std::future::Future;

use uuid::Uuid;

use crate::step::tag_of;
use crate::{MAX_OBJECT_BYTES, RemoteStore, SpaceKeys, SyncError, devices_dir, open_snapshot, snapshot_keyring};

/// The most snapshots a search reads, and so the most keyrings a join tries (one Argon2id run
/// each): far above the devices a person has, and a bound on the work that objects planted in the
/// storage can cause.
pub const MAX_SNAPSHOTS_TRIED: usize = 32;

/// Open space `space_id` under `prefix`. Every device's snapshot carries the data key wrapped
/// under that device's master password and the sync key; `open` tries one such keyring
/// ([`crate::open_keyring`] with the password typed in: Argon2id, so the caller runs it where
/// blocking is fine), and the first that opens gives the space's keys.
///
/// [`SyncError::NoSpace`] when no device snapshot is there, [`SyncError::WrongCredentials`] when
/// keyrings are there and none opens, [`SyncError::Unsupported`] when a snapshot comes from a
/// newer Lockra, [`SyncError::Corrupted`] when nothing there is a snapshot of this space.
pub async fn open_space<F, Fut>(remote: &dyn RemoteStore, prefix: &str, space_id: Uuid, mut open: F) -> Result<SpaceKeys, SyncError>
where
    F: FnMut(Vec<u8>) -> Fut + Send,
    Fut: Future<Output = Result<SpaceKeys, SyncError>> + Send,
{
    let dir = devices_dir(prefix, space_id);
    let mut search = Search::default();
    for (tag, name) in candidates(remote, &dir).await? {
        let Some(bytes) = search.read(remote, &dir, name).await? else { continue };
        let keyring = match snapshot_keyring(&bytes, space_id, &tag) {
            Ok(keyring) => keyring,
            Err(error) => {
                search.unusable(error)?;
                continue;
            }
        };
        match open(keyring).await {
            Ok(keys) => return Ok(keys),
            Err(SyncError::WrongCredentials) => search.refused = true,
            Err(error) => search.unusable(error)?,
        }
    }
    Err(search.failure())
}

/// Check that space `keys` is under `prefix` (new storage settings): one of its snapshots opens
/// under the space's data key. Device `device`'s own is read first.
pub async fn find_space(remote: &dyn RemoteStore, prefix: &str, keys: &SpaceKeys, device: u64) -> Result<(), SyncError> {
    let dir = devices_dir(prefix, keys.space_id());
    let own = keys.device_tag(device);
    let mut search = Search::default();
    let mut listed = candidates(remote, &dir).await?;
    listed.sort_by_key(|(tag, _)| *tag != own);
    for (tag, name) in listed {
        let Some(bytes) = search.read(remote, &dir, name).await? else { continue };
        match open_snapshot(keys, &tag, &bytes) {
            Ok(_) => return Ok(()),
            Err(error) => search.unusable(error)?,
        }
    }
    Err(search.failure())
}

/// The device snapshots listed in `dir`, at most [`MAX_SNAPSHOTS_TRIED`]: the tag, and the name
/// unless the object is larger than any snapshot Lockra writes (counted, never read).
async fn candidates(remote: &dyn RemoteStore, dir: &str) -> Result<Vec<(String, Option<String>)>, SyncError> {
    let mut found: Vec<(String, Option<String>)> = remote
        .list(dir)
        .await?
        .into_iter()
        .filter_map(|meta| tag_of(&meta.name).map(|tag| (tag, (meta.size <= MAX_OBJECT_BYTES).then_some(meta.name))))
        .collect();
    found.truncate(MAX_SNAPSHOTS_TRIED);
    Ok(found)
}

/// What a search met on its way, for the answer when nothing opened.
#[derive(Default)]
struct Search {
    found: bool,
    refused: bool,
    newer: Option<u32>,
}

impl Search {
    /// The object `name` in `dir`; `None` for one that is gone or too large to be a snapshot.
    async fn read(&mut self, remote: &dyn RemoteStore, dir: &str, name: Option<String>) -> Result<Option<Vec<u8>>, SyncError> {
        self.found = true;
        let Some(name) = name else { return Ok(None) };
        match remote.get(&format!("{dir}{name}")).await {
            Ok(found) => Ok(found.map(|(bytes, _)| bytes)),
            Err(SyncError::Corrupted) => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// An object that is no snapshot of this space (damaged, moved, foreign, or from a newer
    /// Lockra) is passed over; any other failure ends the search.
    fn unusable(&mut self, error: SyncError) -> Result<(), SyncError> {
        match error {
            SyncError::Unsupported(format) => self.newer = Some(self.newer.map_or(format, |seen| seen.max(format))),
            SyncError::Corrupted | SyncError::Misplaced | SyncError::NotLockra => {}
            other => return Err(other),
        }
        Ok(())
    }

    fn failure(&self) -> SyncError {
        match (self.found, self.newer, self.refused) {
            (false, _, _) => SyncError::NoSpace,
            (true, Some(format), _) => SyncError::Unsupported(format),
            (true, None, true) => SyncError::WrongCredentials,
            (true, None, false) => SyncError::Corrupted,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use lockra_vault::KdfCost;
    use zeroize::Zeroizing;

    use super::*;
    use crate::{MemoryRemote, Snapshot, SyncKey, device_path, open_keyring, seal_keyring, seal_snapshot};

    const PREFIX: &str = "lockra";

    /// A space with one snapshot per password given, devices 1, 2, …
    fn space_with(remote: &MemoryRemote, passwords: &[&[u8]]) -> (SpaceKeys, SyncKey) {
        let sync_key = SyncKey::generate().unwrap();
        let keys = SpaceKeys::generate(sync_key.space_id()).unwrap();
        for (device, password) in (1u64..).zip(passwords) {
            let keyring = seal_keyring(&keys, &sync_key, password, KdfCost::FAST_INSECURE).unwrap();
            let tag = keys.device_tag(device);
            let snapshot = Snapshot { seq: 1, written_at_ms: 1, device_name: format!("Device {device}"), keyring, payload: Zeroizing::new(b"{}".to_vec()) };
            remote.set_object(&device_path(PREFIX, keys.space_id(), &tag), seal_snapshot(&keys, &tag, &snapshot).unwrap());
        }
        (keys, sync_key)
    }

    async fn join(remote: &MemoryRemote, sync_key: &SyncKey, password: &'static [u8]) -> (Result<SpaceKeys, SyncError>, usize) {
        let tries = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&tries);
        let space_id = sync_key.space_id();
        let opened = open_space(remote, PREFIX, space_id, |keyring| {
            counter.fetch_add(1, Ordering::SeqCst);
            let sync_key = sync_key.clone();
            async move { open_keyring(&keyring, space_id, &sync_key, password) }
        })
        .await;
        (opened, tries.load(Ordering::SeqCst))
    }

    #[tokio::test]
    async fn any_devices_master_password_opens_the_space() {
        let remote = MemoryRemote::new(false);
        let (keys, sync_key) = space_with(&remote, &[b"laptop password", b"phone password"]);
        let (laptop, _) = join(&remote, &sync_key, b"laptop password").await;
        assert_eq!(*laptop.unwrap().data_key_text(), *keys.data_key_text());
        let (phone, _) = join(&remote, &sync_key, b"phone password").await;
        assert_eq!(*phone.unwrap().data_key_text(), *keys.data_key_text());
        let (refused, tries) = join(&remote, &sync_key, b"neither").await;
        assert_eq!(refused.err(), Some(SyncError::WrongCredentials));
        assert_eq!(tries, 2, "every keyring was tried");
        // The right password with another sync key opens nothing either.
        let other = SyncKey::generate().unwrap();
        assert_eq!(join(&remote, &other, b"laptop password").await.0.err(), Some(SyncError::NoSpace), "another key names another space");
    }

    #[tokio::test]
    async fn a_search_tells_an_empty_place_a_damaged_space_and_a_newer_one_apart() {
        let remote = MemoryRemote::new(true);
        let sync_key = SyncKey::generate().unwrap();
        assert_eq!(join(&remote, &sync_key, b"x").await.0.err(), Some(SyncError::NoSpace));

        let (keys, sync_key) = space_with(&remote, &[b"pw"]);
        let dir = devices_dir(PREFIX, keys.space_id());
        let only = remote.paths().pop().unwrap();
        // Not a device's name: not part of the space.
        remote.set_object(&format!("{dir}notes.txt"), b"hello".to_vec());
        let mut damaged = remote.object(&only).unwrap();
        damaged[3] ^= 1;
        remote.set_object(&only, damaged);
        let (failed, tries) = join(&remote, &sync_key, b"pw").await;
        assert_eq!((failed.err(), tries), (Some(SyncError::Corrupted), 0));
        assert_eq!(find_space(&remote, PREFIX, &keys, 1).await.err(), Some(SyncError::Corrupted));

        let newer = br#"{"format":9,"space_id":"x","tag":"y"}"#;
        let mut object = b"LKSDEVS1".to_vec();
        object.extend_from_slice(&u32::try_from(newer.len()).unwrap().to_le_bytes());
        object.extend_from_slice(newer);
        object.extend_from_slice(&[0; 16]);
        remote.set_object(&only, object);
        assert_eq!(join(&remote, &sync_key, b"pw").await.0.err(), Some(SyncError::Unsupported(9)));
        assert_eq!(find_space(&remote, PREFIX, &keys, 1).await.err(), Some(SyncError::Unsupported(9)));
    }

    #[tokio::test]
    async fn planted_objects_cost_a_bounded_number_of_tries() {
        let remote = MemoryRemote::new(true);
        let (keys, sync_key) = space_with(&remote, &[b"pw"]);
        // Garbage that reads as keyrings, under device names, beyond the bound.
        for device in 100..140 {
            let tag = keys.device_tag(device);
            let snapshot = Snapshot { seq: 1, written_at_ms: 1, device_name: String::new(), keyring: b"LKSKEYR1 junk".to_vec(), payload: Zeroizing::default() };
            let object = seal_snapshot(&SpaceKeys::generate(keys.space_id()).unwrap(), &tag, &snapshot).unwrap();
            remote.set_object(&device_path(PREFIX, keys.space_id(), &tag), object);
        }
        let (result, tries) = join(&remote, &sync_key, b"wrong").await;
        assert!(tries <= MAX_SNAPSHOTS_TRIED, "{tries}");
        assert!(matches!(result, Err(SyncError::WrongCredentials | SyncError::Corrupted)), "{result:?}");
        assert!(remote.calls().iter().filter(|c| c.starts_with("get ")).count() <= MAX_SNAPSHOTS_TRIED);
    }

    #[tokio::test]
    async fn an_object_too_large_to_be_a_snapshot_is_never_read() {
        let remote = MemoryRemote::new(true);
        let sync_key = SyncKey::generate().unwrap();
        let tag = SpaceKeys::generate(sync_key.space_id()).unwrap().device_tag(1);
        let huge = vec![0u8; usize::try_from(MAX_OBJECT_BYTES).unwrap() + 1];
        remote.set_object(&device_path(PREFIX, sync_key.space_id(), &tag), huge);
        assert_eq!(join(&remote, &sync_key, b"pw").await.0.err(), Some(SyncError::Corrupted));
        assert!(remote.calls().iter().all(|c| !c.starts_with("get ")), "{:?}", remote.calls());
    }

    #[tokio::test]
    async fn new_storage_settings_must_hold_this_spaces_snapshots() {
        let remote = MemoryRemote::new(true);
        let (keys, _) = space_with(&remote, &[b"a", b"b"]);
        find_space(&remote, PREFIX, &keys, 2).await.unwrap();
        // This device's own snapshot is read first.
        let first_get = remote.calls().into_iter().find(|c| c.starts_with("get ")).unwrap();
        assert!(first_get.ends_with(&format!("{}.lks", keys.device_tag(2))), "{first_get}");
        // Another space's keys open nothing here.
        let stranger = SpaceKeys::generate(keys.space_id()).unwrap();
        assert_eq!(find_space(&remote, PREFIX, &stranger, 1).await.err(), Some(SyncError::Corrupted));
        assert_eq!(find_space(&remote, "elsewhere", &keys, 1).await.err(), Some(SyncError::NoSpace));
        // A storage failure is the storage's, not the space's.
        remote.fail_next(SyncError::Denied);
        assert_eq!(find_space(&remote, PREFIX, &keys, 1).await.err(), Some(SyncError::Denied));
    }
}
