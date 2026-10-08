//! The spaces, on disk and indexed in memory.
//!
//! `<data>/lockra-relay-v1/spaces/<space id>/access` holds the SHA-256 of the space's access token
//! (hex): all the relay keeps of it. `…/devices/<tag>.lks` holds each device's snapshot, written to
//! a temporary file and moved into place, so a reader sees the old snapshot or the new one, never
//! half of one. The index (sizes, etags, the token's hash, the last use) is rebuilt from the files
//! when the relay starts. A space is created by its first write, bound to the token that wrote it.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use data_encoding::HEXLOWER;
use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::watch;
use uuid::Uuid;

use crate::config::{Limits, MAX_OBJECT_BYTES_CEILING};

const ROOT: &str = "lockra-relay-v1";
const ACCESS: &str = "access";
const DEVICES: &str = "devices";
const TEMP_SUFFIX: &str = ".tmp";
/// How often a space's last use reaches the disk: only the expiry needs it to survive a restart.
const TOUCH_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// The SHA-256 of a space's access token: all the relay keeps of it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Access([u8; 32]);

impl Access {
    /// The hash of `token` as the client sent it.
    pub fn of_token(token: &[u8]) -> Self {
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&Sha256::digest(token));
        Self(hash)
    }

    /// Whether `other` is the same token's: every byte compared, whatever the first difference.
    fn matches(&self, other: &Self) -> bool {
        self.0.iter().zip(other.0.iter()).fold(0u8, |diff, (a, b)| diff | (a ^ b)) == 0
    }
}

impl std::fmt::Debug for Access {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Access(…)")
    }
}

/// When a write may replace what is there (the client's `If-None-Match: *` and `If-Match`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    /// Always.
    Always,
    /// Only when nothing is there yet.
    IfAbsent,
    /// Only when the snapshot still has this etag.
    IfMatch(String),
}

/// Why the store refused a request.
#[derive(Debug)]
pub enum StoreError {
    /// Another token than the one the space was created with.
    Forbidden,
    /// The write's condition failed.
    Precondition,
    /// A snapshot larger than the relay keeps.
    TooLarge,
    /// A limit of the space or of the relay: which one.
    Full(&'static str),
    /// The space was removed (it had been idle) while the request was on its way: ask again.
    Gone,
    /// The disk failed.
    Io(io::Error),
}

/// A space's snapshots, and the revision of that listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listing {
    /// Changes whenever a snapshot is written or removed: the listing's etag.
    #[serde(skip)]
    pub revision: String,
    /// The snapshots, by name.
    pub objects: Vec<Listed>,
}

/// A snapshot in a listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listed {
    /// `<tag>.lks`.
    pub name: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its etag (a quoted hash of its bytes).
    pub etag: String,
}

/// Whether `name` is a device snapshot's: a 16-byte tag in lowercase hex and `.lks`, nothing else
/// (lockra-sync's device tags). The relay keeps nothing but snapshots.
pub fn is_object_name(name: &str) -> bool {
    name.len() == 36 && name.ends_with(".lks") && name.as_bytes()[..32].iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The space id `text` names, written the one way a directory is named after it.
pub fn space_id(text: &str) -> Option<Uuid> {
    Uuid::try_parse(text).ok().filter(|id| id.hyphenated().to_string() == text)
}

/// The listing revision of a space without snapshots.
pub fn empty_revision() -> String {
    revision_of(&BTreeMap::new())
}

/// All spaces.
pub struct Store {
    root: PathBuf,
    limits: Limits,
    index: Mutex<Index>,
}

struct Index {
    spaces: HashMap<Uuid, Arc<Space>>,
    /// The snapshots of all spaces together.
    bytes: u64,
}

struct Space {
    access: Access,
    dir: PathBuf,
    /// Writes and removals, one at a time: a condition is checked and the write made under it.
    write: tokio::sync::Mutex<()>,
    state: Mutex<SpaceState>,
    /// The listing's revision, for the clients waiting for a change.
    revision: watch::Sender<String>,
}

struct SpaceState {
    objects: BTreeMap<String, Object>,
    bytes: u64,
    touched: SystemTime,
    touched_on_disk: SystemTime,
    /// Its directory and access file are written (a space is in the index from its first write on,
    /// before that write reaches the disk).
    on_disk: bool,
    /// Removed: requests still holding it find nothing.
    gone: bool,
}

#[derive(Debug, Clone)]
struct Object {
    size: u64,
    etag: String,
}

impl Space {
    fn new(access: Access, dir: PathBuf, objects: BTreeMap<String, Object>, on_disk: bool, touched: SystemTime) -> Self {
        let bytes = objects.values().map(|o| o.size).sum();
        let revision = revision_of(&objects);
        Self {
            access,
            dir,
            write: tokio::sync::Mutex::new(()),
            state: Mutex::new(SpaceState { objects, bytes, touched, touched_on_disk: touched, on_disk, gone: false }),
            revision: watch::Sender::new(revision),
        }
    }

    fn listing(&self) -> Listing {
        let state = self.state.lock();
        Listing {
            revision: revision_of(&state.objects),
            objects: state.objects.iter().map(|(name, o)| Listed { name: name.clone(), size: o.size, etag: o.etag.clone() }).collect(),
        }
    }

    /// Record a change of the snapshots and tell whoever waits for one.
    fn changed(&self, state: &SpaceState) {
        self.revision.send_replace(revision_of(&state.objects));
    }
}

impl Store {
    /// The spaces kept in `data_dir`, read in (and made, the first time).
    pub fn open(data_dir: &Path, limits: Limits) -> io::Result<Self> {
        let root = data_dir.join(ROOT).join("spaces");
        fs::create_dir_all(&root)?;
        let mut spaces = HashMap::new();
        let mut bytes = 0;
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let Some(id) = entry.file_name().to_str().and_then(space_id) else {
                tracing::warn!(entry = ?entry.file_name(), "not a space; left alone");
                continue;
            };
            match load_space(&entry.path())? {
                Some(space) => {
                    bytes += space.state.lock().bytes;
                    spaces.insert(id, Arc::new(space));
                }
                None => tracing::warn!("a space without a readable access file; left alone"),
            }
        }
        Ok(Self { root, limits, index: Mutex::new(Index { spaces, bytes }) })
    }

    /// The spaces kept.
    pub fn spaces(&self) -> usize {
        self.index.lock().spaces.len()
    }

    /// The bytes kept, all spaces together.
    pub fn bytes(&self) -> u64 {
        self.index.lock().bytes
    }

    /// Whether the relay keeps space `id`.
    pub fn exists(&self, id: Uuid) -> bool {
        self.index.lock().spaces.contains_key(&id)
    }

    /// The snapshots of space `id`; none when the relay does not keep it.
    pub async fn list(&self, id: Uuid, access: &Access) -> Result<Listing, StoreError> {
        match self.space(id, access)? {
            Some(space) => {
                self.touch(&space).await;
                Ok(space.listing())
            }
            None => Ok(Listing { revision: empty_revision(), objects: Vec::new() }),
        }
    }

    /// The revision of space `id`'s listing, to wait on; `None` when the relay does not keep it.
    pub async fn watch(&self, id: Uuid, access: &Access) -> Result<Option<watch::Receiver<String>>, StoreError> {
        match self.space(id, access)? {
            Some(space) => {
                self.touch(&space).await;
                Ok(Some(space.revision.subscribe()))
            }
            None => Ok(None),
        }
    }

    /// The snapshot `name` of space `id` and its etag; `None` when there is none.
    pub async fn get(&self, id: Uuid, access: &Access, name: &str) -> Result<Option<(Bytes, String)>, StoreError> {
        let Some(space) = self.space(id, access)? else { return Ok(None) };
        self.touch(&space).await;
        if !space.state.lock().objects.contains_key(name) {
            return Ok(None);
        }
        let path = space.dir.join(DEVICES).join(name);
        let max = self.limits.max_object_bytes;
        match tokio::task::spawn_blocking(move || read_bounded(&path, max)).await {
            // The etag of what was read: a write that replaced it meanwhile has its own.
            Ok(Ok(bytes)) => {
                let etag = etag_of(&bytes);
                Ok(Some((bytes, etag)))
            }
            Ok(Err(e)) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Ok(Err(e)) => Err(StoreError::Io(e)),
            Err(e) => Err(StoreError::Io(io::Error::other(e))),
        }
    }

    /// Write snapshot `name` of space `id` when `condition` holds; the space is created by its first
    /// write, bound to `access`. Answers the new etag.
    pub async fn put(&self, id: Uuid, access: &Access, name: &str, bytes: Bytes, condition: Condition) -> Result<String, StoreError> {
        if bytes.len() as u64 > self.limits.max_object_bytes {
            return Err(StoreError::TooLarge);
        }
        let space = self.space_for_write(id, access)?;
        let _write = space.write.lock().await;
        let (gone, on_disk) = {
            let state = space.state.lock();
            (state.gone, state.on_disk)
        };
        if gone {
            return Err(StoreError::Gone);
        }
        if !on_disk {
            self.persist(id, &space).await?;
        }
        let current = space.state.lock().objects.get(name).cloned();
        let allowed = match (&condition, &current) {
            (Condition::Always, _) | (Condition::IfAbsent, None) => true,
            (Condition::IfAbsent, Some(_)) | (Condition::IfMatch(_), None) => false,
            (Condition::IfMatch(etag), Some(object)) => object.etag == *etag,
        };
        if !allowed {
            return Err(StoreError::Precondition);
        }
        let (old, new) = (current.as_ref().map_or(0, |o| o.size), bytes.len() as u64);
        {
            let state = space.state.lock();
            if current.is_none() && state.objects.len() >= self.limits.max_objects {
                return Err(StoreError::Full("devices"));
            }
            if state.bytes.saturating_sub(old) + new > self.limits.max_space_bytes {
                return Err(StoreError::Full("space"));
            }
        }
        self.reserve(old, new)?;
        let etag = etag_of(&bytes);
        let path = space.dir.join(DEVICES).join(name);
        let written = tokio::task::spawn_blocking(move || write_atomic(&path, &bytes)).await.unwrap_or_else(|e| Err(io::Error::other(e)));
        if let Err(e) = written {
            self.reserve(new, old).ok();
            return Err(StoreError::Io(e));
        }
        let mut state = space.state.lock();
        state.objects.insert(name.to_owned(), Object { size: new, etag: etag.clone() });
        state.bytes = state.bytes.saturating_sub(old) + new;
        state.touched = SystemTime::now();
        space.changed(&state);
        Ok(etag)
    }

    /// Remove snapshot `name` of space `id`; nothing there is no error.
    pub async fn delete(&self, id: Uuid, access: &Access, name: &str) -> Result<(), StoreError> {
        let Some(space) = self.space(id, access)? else { return Ok(()) };
        self.touch(&space).await;
        let _write = space.write.lock().await;
        let object = {
            let state = space.state.lock();
            if state.gone { None } else { state.objects.get(name).cloned() }
        };
        let Some(object) = object else { return Ok(()) };
        let path = space.dir.join(DEVICES).join(name);
        let removed = tokio::task::spawn_blocking(move || match fs::remove_file(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        })
        .await
        .unwrap_or_else(|e| Err(io::Error::other(e)));
        removed.map_err(StoreError::Io)?;
        self.reserve(object.size, 0).ok();
        let mut state = space.state.lock();
        state.objects.remove(name);
        state.bytes = state.bytes.saturating_sub(object.size);
        space.changed(&state);
        Ok(())
    }

    /// Remove the spaces no device reached for the idle time, as of `now`; answers how many went.
    /// A space with a write under way is left for the next time.
    pub async fn expire(&self, now: SystemTime) -> usize {
        let idle = self.limits.idle;
        let stale = |space: &Space| now.duration_since(space.state.lock().touched).is_ok_and(|age| age >= idle);
        let candidates: Vec<(Uuid, Arc<Space>)> = self.index.lock().spaces.iter().filter(|(_, s)| stale(s)).map(|(id, s)| (*id, Arc::clone(s))).collect();
        let mut removed = 0;
        for (id, space) in candidates {
            let Ok(_write) = space.write.try_lock() else { continue };
            if !stale(&space) {
                continue;
            }
            let dir = space.dir.clone();
            let deleted = tokio::task::spawn_blocking(move || match fs::remove_dir_all(&dir) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            })
            .await
            .unwrap_or_else(|e| Err(io::Error::other(e)));
            if let Err(error) = deleted {
                tracing::warn!(%error, "an idle space could not be removed");
                continue;
            }
            let bytes = {
                let mut state = space.state.lock();
                state.gone = true;
                state.bytes
            };
            let mut index = self.index.lock();
            if index.spaces.get(&id).is_some_and(|s| Arc::ptr_eq(s, &space)) {
                index.spaces.remove(&id);
                index.bytes = index.bytes.saturating_sub(bytes);
            }
            removed += 1;
        }
        removed
    }

    /// Space `id`, when `access` is its token's; `None` when the relay does not keep it.
    fn space(&self, id: Uuid, access: &Access) -> Result<Option<Arc<Space>>, StoreError> {
        match self.index.lock().spaces.get(&id) {
            Some(space) if space.access.matches(access) => Ok(Some(Arc::clone(space))),
            Some(_) => Err(StoreError::Forbidden),
            None => Ok(None),
        }
    }

    /// Space `id` for a write: made, bound to `access`, when the relay does not keep it yet.
    fn space_for_write(&self, id: Uuid, access: &Access) -> Result<Arc<Space>, StoreError> {
        let mut index = self.index.lock();
        if let Some(space) = index.spaces.get(&id) {
            return if space.access.matches(access) { Ok(Arc::clone(space)) } else { Err(StoreError::Forbidden) };
        }
        if index.spaces.len() >= self.limits.max_spaces {
            return Err(StoreError::Full("spaces"));
        }
        let space = Arc::new(Space::new(*access, self.root.join(id.hyphenated().to_string()), BTreeMap::new(), false, SystemTime::now()));
        index.spaces.insert(id, Arc::clone(&space));
        Ok(space)
    }

    /// Write a new space's directory and access file. Whatever was left of a space by that name
    /// (one the index could not read in) goes first: the index and the disk say the same.
    async fn persist(&self, id: Uuid, space: &Arc<Space>) -> Result<(), StoreError> {
        let (dir, access) = (space.dir.clone(), space.access);
        let written = tokio::task::spawn_blocking(move || {
            match fs::remove_dir_all(&dir) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
            fs::create_dir_all(dir.join(DEVICES))?;
            write_atomic(&dir.join(ACCESS), HEXLOWER.encode(&access.0).as_bytes())
        })
        .await
        .unwrap_or_else(|e| Err(io::Error::other(e)));
        match written {
            Ok(()) => {
                space.state.lock().on_disk = true;
                Ok(())
            }
            Err(e) => {
                space.state.lock().gone = true;
                let mut index = self.index.lock();
                if index.spaces.get(&id).is_some_and(|s| Arc::ptr_eq(s, space)) {
                    index.spaces.remove(&id);
                }
                Err(StoreError::Io(e))
            }
        }
    }

    /// Take `new` bytes in place of `old` from the relay's total, within its limit.
    fn reserve(&self, old: u64, new: u64) -> Result<(), StoreError> {
        let mut index = self.index.lock();
        let total = index.bytes.saturating_sub(old) + new;
        if new > old && total > self.limits.max_total_bytes {
            return Err(StoreError::Full("relay"));
        }
        index.bytes = total;
        Ok(())
    }

    /// Note that a device reached `space` now (on disk at most once a day).
    async fn touch(&self, space: &Space) {
        let now = SystemTime::now();
        let due = {
            let mut state = space.state.lock();
            state.touched = now;
            let due = state.on_disk && now.duration_since(state.touched_on_disk).is_ok_and(|age| age >= TOUCH_EVERY);
            if due {
                state.touched_on_disk = now;
            }
            due
        };
        if due {
            let path = space.dir.join(ACCESS);
            let _ = tokio::task::spawn_blocking(move || fs::File::options().write(true).open(path).and_then(|f| f.set_modified(now))).await;
        }
    }
}

/// A space's directory as the relay left it; `None` without a readable access file.
fn load_space(dir: &Path) -> io::Result<Option<Space>> {
    let Some(access) = fs::read_to_string(dir.join(ACCESS)).ok().and_then(|text| HEXLOWER.decode(text.trim().as_bytes()).ok()) else { return Ok(None) };
    let Ok(access) = <[u8; 32]>::try_from(access.as_slice()) else { return Ok(None) };
    let mut touched = modified(&dir.join(ACCESS));
    let mut objects = BTreeMap::new();
    let devices = dir.join(DEVICES);
    let entries = match fs::read_dir(&devices) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Some(Space::new(Access(access), dir.to_owned(), objects, true, touched))),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
        if name.starts_with('.') && name.ends_with(TEMP_SUFFIX) {
            // A write the relay did not finish.
            let _ = fs::remove_file(&path);
            continue;
        }
        if !is_object_name(&name) {
            continue;
        }
        let Ok(bytes) = read_bounded(&path, MAX_OBJECT_BYTES_CEILING) else { continue };
        touched = touched.max(modified(&path));
        objects.insert(name, Object { size: bytes.len() as u64, etag: etag_of(&bytes) });
    }
    Ok(Some(Space::new(Access(access), dir.to_owned(), objects, true, touched)))
}

fn modified(path: &Path) -> SystemTime {
    fs::metadata(path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH)
}

/// The file at `path`, refused when larger than `max`.
fn read_bounded(path: &Path, max: u64) -> io::Result<Bytes> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "larger than the relay keeps"));
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "larger than the relay keeps"));
    }
    Ok(Bytes::from(bytes))
}

/// Write `bytes` to `path` through a temporary file beside it, on disk before it takes the name.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return Err(io::Error::other("no place to write"));
    };
    let temp = dir.join(format!(".{name}.{}{TEMP_SUFFIX}", Uuid::new_v4().simple()));
    let result = fs::File::create_new(&temp)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temp, path));
    match result {
        Ok(()) => {
            // The new name on disk too (POSIX; elsewhere the rename is what there is).
            #[cfg(unix)]
            if let Ok(dir) = fs::File::open(dir) {
                let _ = dir.sync_all();
            }
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&temp);
            Err(e)
        }
    }
}

/// A snapshot's etag: its bytes' SHA-256, quoted as an HTTP entity tag.
fn etag_of(bytes: &[u8]) -> String {
    format!("\"{}\"", &HEXLOWER.encode(&Sha256::digest(bytes))[..32])
}

/// A listing's revision: a hash of its names and etags, quoted.
fn revision_of(objects: &BTreeMap<String, Object>) -> String {
    let mut hash = Sha256::new();
    for (name, object) in objects {
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(object.etag.as_bytes());
        hash.update(b"\n");
    }
    format!("\"{}\"", &HEXLOWER.encode(&hash.finalize())[..32])
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0123456789abcdef0123456789abcdef.lks";
    const B: &str = "fedcba9876543210fedcba9876543210.lks";

    fn id() -> Uuid {
        Uuid::new_v4()
    }

    fn token(n: u8) -> Access {
        Access::of_token(&[n; 32])
    }

    fn limits() -> Limits {
        Limits { max_object_bytes: 64, max_space_bytes: 100, max_objects: 2, max_total_bytes: 150, max_spaces: 3, ..Limits::default() }
    }

    fn store(dir: &Path) -> Store {
        Store::open(dir, limits()).unwrap()
    }

    #[test]
    fn only_device_snapshots_and_canonical_space_ids_are_names() {
        assert!(is_object_name(A));
        for bad in [
            "0123456789ABCDEF0123456789abcdef.lks",
            "0123456789abcdef0123456789abcde.lks",
            "0123456789abcdef0123456789abcdef.txt",
            "../23456789abcdef0123456789abcdef.lks",
            "",
        ] {
            assert!(!is_object_name(bad), "{bad}");
        }
        let id = id();
        assert_eq!(space_id(&id.to_string()), Some(id));
        assert_eq!(space_id(&id.to_string().to_uppercase()), None);
        assert_eq!(space_id(&id.simple().to_string()), None);
        assert_eq!(space_id("../etc"), None);
    }

    #[tokio::test]
    async fn a_space_is_made_by_its_first_write_and_keeps_its_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let (space, access) = (id(), token(1));
        assert_eq!(store.list(space, &access).await.unwrap(), Listing { revision: empty_revision(), objects: vec![] });
        assert!(store.get(space, &access, A).await.unwrap().is_none());
        assert!(store.watch(space, &access).await.unwrap().is_none());
        store.delete(space, &access, A).await.unwrap();
        assert!(!store.exists(space));

        let etag = store.put(space, &access, A, Bytes::from_static(b"one"), Condition::IfAbsent).await.unwrap();
        assert!(store.exists(space));
        let (bytes, read_etag) = store.get(space, &access, A).await.unwrap().unwrap();
        assert_eq!((bytes.as_ref(), read_etag.as_str()), (b"one".as_slice(), etag.as_str()));
        let listing = store.list(space, &access).await.unwrap();
        assert_eq!(listing.objects, vec![Listed { name: A.into(), size: 3, etag: etag.clone() }]);
        assert_ne!(listing.revision, empty_revision());
        assert_eq!((store.spaces(), store.bytes()), (1, 3));

        // The same bytes, the same etag; other bytes, another.
        let again = store.put(space, &access, A, Bytes::from_static(b"one"), Condition::Always).await.unwrap();
        assert_eq!(again, etag);
        let two = store.put(space, &access, A, Bytes::from_static(b"two!"), Condition::IfMatch(etag.clone())).await.unwrap();
        assert_ne!(two, etag);
        assert_eq!(store.bytes(), 4);
        store.delete(space, &access, A).await.unwrap();
        assert!(store.get(space, &access, A).await.unwrap().is_none());
        assert_eq!(store.list(space, &access).await.unwrap().revision, empty_revision());
        assert_eq!(store.bytes(), 0);
        // The space stays, bound to its token.
        assert!(store.exists(space));
    }

    #[tokio::test]
    async fn another_token_is_refused_everywhere() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let space = id();
        store.put(space, &token(1), A, Bytes::from_static(b"one"), Condition::IfAbsent).await.unwrap();
        let other = token(2);
        assert!(matches!(store.list(space, &other).await, Err(StoreError::Forbidden)));
        assert!(matches!(store.watch(space, &other).await, Err(StoreError::Forbidden)));
        assert!(matches!(store.get(space, &other, A).await, Err(StoreError::Forbidden)));
        assert!(matches!(store.put(space, &other, B, Bytes::from_static(b"x"), Condition::Always).await, Err(StoreError::Forbidden)));
        assert!(matches!(store.delete(space, &other, A).await, Err(StoreError::Forbidden)));
        assert!(store.get(space, &token(1), A).await.unwrap().is_some());
        assert_eq!(format!("{:?}", token(1)), "Access(…)");
    }

    #[tokio::test]
    async fn conditions_hold() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let (space, access) = (id(), token(1));
        assert!(matches!(store.put(space, &access, A, Bytes::from_static(b"x"), Condition::IfMatch("\"nope\"".into())).await, Err(StoreError::Precondition)));
        let etag = store.put(space, &access, A, Bytes::from_static(b"x"), Condition::IfAbsent).await.unwrap();
        assert!(matches!(store.put(space, &access, A, Bytes::from_static(b"y"), Condition::IfAbsent).await, Err(StoreError::Precondition)));
        assert!(matches!(store.put(space, &access, A, Bytes::from_static(b"y"), Condition::IfMatch("\"nope\"".into())).await, Err(StoreError::Precondition)));
        store.put(space, &access, A, Bytes::from_static(b"y"), Condition::IfMatch(etag)).await.unwrap();
        assert_eq!(store.get(space, &access, A).await.unwrap().unwrap().0.as_ref(), b"y");
    }

    #[tokio::test]
    async fn every_limit_holds() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let access = token(1);
        let (one, two, three, four) = (id(), id(), id(), id());
        assert!(matches!(store.put(one, &access, A, Bytes::from(vec![0; 65]), Condition::Always).await, Err(StoreError::TooLarge)));
        store.put(one, &access, A, Bytes::from(vec![0; 60]), Condition::Always).await.unwrap();
        // The space's bytes (100).
        assert!(matches!(store.put(one, &access, B, Bytes::from(vec![0; 41]), Condition::Always).await, Err(StoreError::Full("space"))));
        store.put(one, &access, B, Bytes::from(vec![0; 40]), Condition::Always).await.unwrap();
        // Devices in a space (2).
        assert!(matches!(
            store.put(one, &access, "00000000000000000000000000000000.lks", Bytes::from_static(b"x"), Condition::Always).await,
            Err(StoreError::Full("devices"))
        ));
        // Replacing a snapshot counts the difference only.
        store.put(one, &access, B, Bytes::from(vec![0; 30]), Condition::Always).await.unwrap();
        assert_eq!(store.bytes(), 90);
        // The relay's bytes (150).
        assert!(matches!(store.put(two, &access, A, Bytes::from(vec![0; 61]), Condition::Always).await, Err(StoreError::Full("relay"))));
        store.put(two, &access, A, Bytes::from(vec![0; 60]), Condition::Always).await.unwrap();
        // Shrinking is always allowed.
        store.put(two, &access, A, Bytes::from(vec![0; 10]), Condition::Always).await.unwrap();
        assert_eq!(store.bytes(), 100);
        // Spaces (3): the refused write left no space behind.
        store.put(three, &access, A, Bytes::from_static(b"x"), Condition::Always).await.unwrap();
        assert!(matches!(store.put(four, &access, A, Bytes::from_static(b"x"), Condition::Always).await, Err(StoreError::Full("spaces"))));
        assert_eq!(store.spaces(), 3);
    }

    #[tokio::test]
    async fn the_relay_reads_back_what_it_kept_and_drops_what_it_did_not_finish() {
        let dir = tempfile::tempdir().unwrap();
        let (space, access) = (id(), token(1));
        {
            let store = store(dir.path());
            store.put(space, &access, A, Bytes::from_static(b"one"), Condition::Always).await.unwrap();
            store.put(space, &access, B, Bytes::from_static(b"two"), Condition::Always).await.unwrap();
        }
        let spaces = dir.path().join(ROOT).join("spaces");
        let devices = spaces.join(space.to_string()).join(DEVICES);
        // A write cut short, a stranger's file, and directories that are no spaces.
        fs::write(devices.join(format!(".{A}.cafe{TEMP_SUFFIX}")), b"half").unwrap();
        fs::write(devices.join("notes.txt"), b"x").unwrap();
        fs::create_dir(spaces.join("not-a-space")).unwrap();
        let unreadable = id();
        fs::create_dir_all(spaces.join(unreadable.to_string()).join(DEVICES)).unwrap();
        fs::write(spaces.join(unreadable.to_string()).join(ACCESS), b"not hex").unwrap();
        let empty = id();
        fs::create_dir_all(spaces.join(empty.to_string())).unwrap();
        fs::write(spaces.join(empty.to_string()).join(ACCESS), HEXLOWER.encode(&token(3).0)).unwrap();

        let store = store(dir.path());
        assert_eq!((store.spaces(), store.bytes()), (2, 6));
        let listing = store.list(space, &access).await.unwrap();
        assert_eq!(listing.objects.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), [A, B]);
        assert!(!devices.join(format!(".{A}.cafe{TEMP_SUFFIX}")).exists());
        assert!(matches!(store.list(space, &token(2)).await, Err(StoreError::Forbidden)));
        assert!(store.list(empty, &token(3)).await.unwrap().objects.is_empty());
        // A space the index could not read is made anew by its next write, its leftovers gone.
        store.put(unreadable, &token(4), A, Bytes::from_static(b"new"), Condition::IfAbsent).await.unwrap();
        assert_eq!(store.list(unreadable, &token(4)).await.unwrap().objects.len(), 1);
        assert_eq!(fs::read_to_string(spaces.join(unreadable.to_string()).join(ACCESS)).unwrap(), HEXLOWER.encode(&token(4).0));
    }

    #[tokio::test]
    async fn a_waiting_client_hears_every_change() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let (space, access) = (id(), token(1));
        store.put(space, &access, A, Bytes::from_static(b"one"), Condition::Always).await.unwrap();
        let mut revision = store.watch(space, &access).await.unwrap().unwrap();
        let first = revision.borrow_and_update().clone();
        store.put(space, &access, B, Bytes::from_static(b"two"), Condition::Always).await.unwrap();
        revision.changed().await.unwrap();
        let second = revision.borrow_and_update().clone();
        assert_ne!(first, second);
        assert_eq!(second, store.list(space, &access).await.unwrap().revision);
        store.delete(space, &access, B).await.unwrap();
        revision.changed().await.unwrap();
        assert_eq!(*revision.borrow(), first);
    }

    #[tokio::test]
    async fn idle_spaces_go_and_the_others_stay() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), Limits { idle: Duration::from_secs(60), ..limits() }).unwrap();
        let (old, fresh, access) = (id(), id(), token(1));
        store.put(old, &access, A, Bytes::from_static(b"one"), Condition::Always).await.unwrap();
        store.put(fresh, &access, A, Bytes::from_static(b"two"), Condition::Always).await.unwrap();
        assert_eq!(store.expire(SystemTime::now()).await, 0);
        // Seen later than the old one.
        let later = SystemTime::now() + Duration::from_secs(61);
        store.index.lock().spaces[&fresh].state.lock().touched = later;
        assert_eq!(store.expire(later).await, 1);
        assert!(!store.exists(old) && store.exists(fresh));
        assert_eq!(store.bytes(), 3);
        assert!(!dir.path().join(ROOT).join("spaces").join(old.to_string()).exists());
        // A space with a write under way waits for the next time.
        let busy = Arc::clone(&store.index.lock().spaces[&fresh]);
        let _write = busy.write.lock().await;
        assert_eq!(store.expire(later + Duration::from_secs(61)).await, 0);
    }

    #[tokio::test]
    async fn the_last_use_reaches_the_disk_once_a_day() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let (space, access) = (id(), token(1));
        store.put(space, &access, A, Bytes::from_static(b"one"), Condition::Always).await.unwrap();
        let file = dir.path().join(ROOT).join("spaces").join(space.to_string()).join(ACCESS);
        let long_ago = SystemTime::now() - Duration::from_secs(3 * 24 * 60 * 60);
        fs::File::options().write(true).open(&file).unwrap().set_modified(long_ago).unwrap();
        store.index.lock().spaces[&space].state.lock().touched_on_disk = long_ago;
        store.list(space, &access).await.unwrap();
        assert!(modified(&file) > long_ago + Duration::from_secs(24 * 60 * 60));
    }

    #[test]
    fn a_file_larger_than_allowed_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big");
        fs::write(&path, vec![0u8; 10]).unwrap();
        assert_eq!(read_bounded(&path, 10).unwrap().len(), 10);
        assert_eq!(read_bounded(&path, 9).unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert!(write_atomic(Path::new("/"), b"x").is_err());
        // Nowhere to write: the temporary file is not left behind either.
        assert!(write_atomic(&dir.path().join("missing").join("x"), b"x").is_err());
    }
}
