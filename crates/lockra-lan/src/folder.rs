//! The hub's copy of a space: a folder on this computer, read and written as a sync storage. The
//! hub's own runs use it directly; its server reads and writes it for the paired devices.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use data_encoding::HEXLOWER;
use lockra_sync::{MAX_OBJECT_BYTES, ObjectMeta, PutCondition, RemoteFuture, RemoteStore, SyncError};
use parking_lot::RwLock;
use sha2::{Digest as _, Sha256};

/// The extension of every object a space keeps.
const EXTENSION: &str = ".lks";

/// A sync storage in a folder. Paths are those of a space (lowercase letters, digits and dashes,
/// an object's name ending in `.lks`); anything else is refused, `..` included. Writes replace an
/// object atomically (lockra-vault's `write_atomic`, which keeps the one before as `.prev`), and a
/// lock lets no read meet a rename in this process (Windows refuses to open a file being renamed).
/// An object's etag is the SHA-256 of its bytes; the conditions are honoured.
#[derive(Debug, Clone)]
pub struct FolderStore {
    root: PathBuf,
    lock: Arc<RwLock<()>>,
}

impl FolderStore {
    /// The store in `root`, made on its first write.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), lock: Arc::new(RwLock::new(())) }
    }

    /// Where `path` (a directory when it ends in `/`) is in the folder.
    fn place(&self, path: &str) -> Result<PathBuf, SyncError> {
        let dir = path.ends_with('/');
        let body = if dir { &path[..path.len() - 1] } else { path };
        let segments: Vec<&str> = body.split('/').collect();
        let mut place = self.root.clone();
        for (i, segment) in segments.iter().enumerate() {
            let name = if !dir && i == segments.len() - 1 { segment.strip_suffix(EXTENSION).unwrap_or("") } else { segment };
            if !plain(name) {
                return Err(SyncError::Storage(format!("not a path of a space: {path}")));
            }
            place.push(segment);
        }
        Ok(place)
    }

    /// Run `work` off the async runtime: file I/O blocks.
    async fn blocking<T: Send + 'static>(&self, work: impl FnOnce(&Path, &RwLock<()>) -> Result<T, SyncError> + Send + 'static) -> Result<T, SyncError> {
        let (root, lock) = (self.root.clone(), Arc::clone(&self.lock));
        tokio::task::spawn_blocking(move || work(&root, &lock)).await.map_err(|_| SyncError::Interrupted)?
    }
}

/// A name the store keeps: lowercase letters, digits and dashes.
fn plain(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// An object's name in a listing: a plain name and the extension (not the copies `write_atomic`
/// keeps beside it).
fn object_name(name: &str) -> bool {
    name.strip_suffix(EXTENSION).is_some_and(plain)
}

fn etag(bytes: &[u8]) -> String {
    HEXLOWER.encode(&Sha256::digest(bytes))
}

fn failed(error: &io::Error) -> SyncError {
    SyncError::Storage(error.to_string())
}

/// The object at `place` and its etag, when there is one; a larger one than an object may be is
/// not read.
fn read(place: &Path) -> Result<Option<(Vec<u8>, String)>, SyncError> {
    match fs::metadata(place) {
        Ok(meta) if meta.len() > MAX_OBJECT_BYTES => Err(SyncError::Corrupted),
        Ok(_) => {
            let bytes = fs::read(place).map_err(|e| failed(&e))?;
            let etag = etag(&bytes);
            Ok(Some((bytes, etag)))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(failed(&error)),
    }
}

fn sibling(place: &Path, suffix: &str) -> PathBuf {
    let mut name = place.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    place.with_file_name(name)
}

impl RemoteStore for FolderStore {
    fn conditional_puts(&self) -> bool {
        true
    }

    fn list<'a>(&'a self, dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>> {
        Box::pin(async move {
            if !dir.ends_with('/') {
                return Err(SyncError::Storage(format!("not a directory: {dir}")));
            }
            let place = self.place(dir)?;
            self.blocking(move |_, lock| {
                let _reading = lock.read();
                let entries = match fs::read_dir(&place) {
                    Ok(entries) => entries,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
                    Err(error) => return Err(failed(&error)),
                };
                let mut listing = Vec::new();
                for entry in entries {
                    let entry = entry.map_err(|e| failed(&e))?;
                    let Ok(name) = entry.file_name().into_string() else { continue };
                    if !object_name(&name) || !entry.file_type().map_err(|e| failed(&e))?.is_file() {
                        continue;
                    }
                    let size = entry.metadata().map_err(|e| failed(&e))?.len();
                    // Larger than any snapshot: listed with its size, never read.
                    let etag = if size > MAX_OBJECT_BYTES { None } else { read(&entry.path())?.map(|(_, etag)| etag) };
                    listing.push(ObjectMeta { name, etag, size });
                }
                listing.sort_by(|a, b| a.name.cmp(&b.name));
                Ok(listing)
            })
            .await
        })
    }

    fn get<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, Option<(Vec<u8>, Option<String>)>> {
        Box::pin(async move {
            let place = self.place(path)?;
            self.blocking(move |_, lock| {
                let _reading = lock.read();
                Ok(read(&place)?.map(|(bytes, etag)| (bytes, Some(etag))))
            })
            .await
        })
    }

    fn put<'a>(&'a self, path: &'a str, bytes: Vec<u8>, condition: PutCondition) -> RemoteFuture<'a, Option<String>> {
        Box::pin(async move {
            let place = self.place(path)?;
            if path.ends_with('/') || bytes.len() as u64 > MAX_OBJECT_BYTES {
                return Err(SyncError::Storage(format!("not an object a space keeps: {path}")));
            }
            self.blocking(move |root, lock| {
                let _writing = lock.write();
                let current = match read(&place) {
                    Ok(found) => found.map(|(_, etag)| etag),
                    // Too large to be one: whatever it is, it is there.
                    Err(SyncError::Corrupted) => Some(String::new()),
                    Err(other) => return Err(other),
                };
                let allowed = match &condition {
                    PutCondition::Always => true,
                    PutCondition::IfAbsent => current.is_none(),
                    PutCondition::IfMatch(etag) => current.as_ref() == Some(etag),
                };
                if !allowed {
                    return Err(SyncError::Conflict);
                }
                if let Some(parent) = place.parent() {
                    fs::create_dir_all(parent).map_err(|e| failed(&e))?;
                }
                debug_assert!(place.starts_with(root));
                lockra_vault::write_atomic(&place, &bytes).map_err(|e| failed(&e))?;
                Ok(Some(etag(&bytes)))
            })
            .await
        })
    }

    fn delete<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, ()> {
        Box::pin(async move {
            let place = self.place(path)?;
            if path.ends_with('/') {
                return Err(SyncError::Storage(format!("not an object: {path}")));
            }
            self.blocking(move |_, lock| {
                let _writing = lock.write();
                match fs::remove_file(&place) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(failed(&error)),
                }
                // The copy the last write kept goes with it.
                let _ = fs::remove_file(sibling(&place, ".prev"));
                Ok(())
            })
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = "lockra-sync-v1/0f3f1a1e-8d4b-4c8e-9f7a-000000000001/devices/";

    #[tokio::test]
    async fn it_lists_reads_writes_and_deletes_with_the_contents_etag() {
        let folder = tempfile::tempdir().unwrap();
        let store = FolderStore::new(folder.path());
        assert!(store.conditional_puts());
        assert!(store.list(DIR).await.unwrap().is_empty(), "no folder yet");
        let path = format!("{DIR}ab12.lks");
        let first = store.put(&path, b"one".to_vec(), PutCondition::IfAbsent).await.unwrap().unwrap();
        assert_eq!(first, etag(b"one"));
        assert_eq!(store.list(DIR).await.unwrap(), [ObjectMeta { name: "ab12.lks".into(), etag: Some(first.clone()), size: 3 }]);
        assert_eq!(store.get(&path).await.unwrap(), Some((b"one".to_vec(), Some(first.clone()))));
        // The conditions hold.
        assert_eq!(store.put(&path, b"two".to_vec(), PutCondition::IfAbsent).await.err(), Some(SyncError::Conflict));
        assert_eq!(store.put(&path, b"two".to_vec(), PutCondition::IfMatch("stale".into())).await.err(), Some(SyncError::Conflict));
        let second = store.put(&path, b"two".to_vec(), PutCondition::IfMatch(first)).await.unwrap().unwrap();
        // The copy write_atomic keeps is no object of the listing.
        assert!(folder.path().join(DIR).join("ab12.lks.prev").exists());
        assert_eq!(store.list(DIR).await.unwrap().len(), 1);
        assert_eq!(store.get(&path).await.unwrap().unwrap().1, Some(second));
        store.delete(&path).await.unwrap();
        store.delete(&path).await.unwrap();
        assert_eq!(store.get(&path).await.unwrap(), None);
        assert!(!folder.path().join(DIR).join("ab12.lks.prev").exists(), "nor its copy");
        assert!(store.list(DIR).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn only_a_spaces_paths_and_never_one_too_large() {
        let folder = tempfile::tempdir().unwrap();
        let store = FolderStore::new(folder.path().join("store"));
        for path in [
            "../outside.lks",
            "lockra-sync-v1/../../outside.lks",
            "/etc/passwd",
            "lockra-sync-v1/x/devices/AB.lks",
            "lockra-sync-v1/x/devices/ab.txt",
            "lockra-sync-v1/x//ab.lks",
            "lockra-sync-v1/x/devices/.lks",
            "lockra-sync-v1\\x\\ab.lks",
            "",
        ] {
            assert!(matches!(store.put(path, b"x".to_vec(), PutCondition::Always).await, Err(SyncError::Storage(_))), "{path}");
            assert!(matches!(store.get(path).await, Err(SyncError::Storage(_))), "{path}");
            assert!(matches!(store.delete(path).await, Err(SyncError::Storage(_))), "{path}");
        }
        assert!(matches!(store.list("lockra-sync-v1/../").await, Err(SyncError::Storage(_))));
        assert!(matches!(store.list(DIR.trim_end_matches('/')).await, Err(SyncError::Storage(_))));
        assert!(!folder.path().join("outside.lks").exists());
        let path = format!("{DIR}ab.lks");
        let too_large = vec![0u8; usize::try_from(MAX_OBJECT_BYTES).unwrap() + 1];
        assert!(matches!(store.put(&path, too_large.clone(), PutCondition::Always).await, Err(SyncError::Storage(_))));
        // One left there some other way is listed with its size and never read.
        fs::create_dir_all(folder.path().join("store").join(DIR)).unwrap();
        fs::write(folder.path().join("store").join(&path), &too_large).unwrap();
        let listing = store.list(DIR).await.unwrap();
        assert_eq!((listing[0].size, listing[0].etag.clone()), (MAX_OBJECT_BYTES + 1, None));
        assert_eq!(store.get(&path).await.err(), Some(SyncError::Corrupted));
        assert_eq!(store.put(&path, b"x".to_vec(), PutCondition::IfAbsent).await.err(), Some(SyncError::Conflict));
        // Files of another kind in the folder are no objects.
        fs::write(folder.path().join("store").join(DIR).join("notes.txt"), b"x").unwrap();
        fs::create_dir_all(folder.path().join("store").join(DIR).join("sub.lks")).unwrap();
        assert_eq!(store.list(DIR).await.unwrap().len(), 1);
    }
}
