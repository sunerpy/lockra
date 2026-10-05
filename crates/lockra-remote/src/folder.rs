//! A sync space in a folder of this computer that a cloud drive's client keeps in sync (OneDrive,
//! iCloud Drive, Dropbox, Jianguoyun, Nextcloud, Synology Drive, Syncthing, …): the drive carries
//! the snapshots between devices, Lockra only reads and writes files.
//!
//! The drive is a storage with no conditions at all, like WebDAV: whatever this computer checks,
//! the drive may still bring another device's copy in later. It also leaves files of its own
//! beside the snapshots (conflicted copies, files it is downloading), which are no object of the
//! space; and it may deliver a snapshot in pieces, which then does not open this round and is read
//! again the next (docs/security.md, "Sync").

use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use data_encoding::HEXLOWER;
use lockra_sync::{MAX_OBJECT_BYTES, ObjectMeta, PutCondition, RemoteFuture, RemoteStore, SyncError};
use parking_lot::RwLock;
use sha2::{Digest as _, Sha256};

/// The extension of every object a space keeps.
const EXTENSION: &str = ".lks";

/// A sync space's storage in a folder. Paths are those of a space (lowercase letters, digits and
/// dashes, an object's name ending in `.lks`); anything else is refused, `..` included. A link
/// below the folder (a drive or a sync peer may bring one) is never followed: a linked directory
/// of the space is refused, a linked object is no object, so nothing outside the folder is read or
/// written. A write replaces its file atomically and keeps no copy beside it (the drive would carry
/// that too). The folder itself is never made: while it is missing (moved, deleted, a drive not
/// connected) every call is [`SyncError::FolderMissing`]. An object's etag is the SHA-256 of its
/// bytes.
#[derive(Debug, Clone)]
pub struct FolderStore {
    root: PathBuf,
    lock: Arc<RwLock<()>>,
}

impl FolderStore {
    /// The store in the folder `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), lock: Arc::new(RwLock::new(())) }
    }

    /// Where `path` (a directory when it ends in `/`) is in the folder, as segments below it.
    fn segments(path: &str) -> Result<Vec<&str>, SyncError> {
        let dir = path.ends_with('/');
        let body = if dir { &path[..path.len() - 1] } else { path };
        let segments: Vec<&str> = body.split('/').collect();
        let last = segments.len() - 1;
        for (i, segment) in segments.iter().enumerate() {
            let name = if !dir && i == last { segment.strip_suffix(EXTENSION).unwrap_or("") } else { segment };
            if !plain(name) {
                return Err(SyncError::Storage(format!("not a path of a space: {path}")));
            }
        }
        Ok(segments)
    }

    /// Run `work` off the async runtime, the folder checked first: file I/O blocks.
    async fn blocking<T: Send + 'static>(&self, work: impl FnOnce(&Path, &RwLock<()>) -> Result<T, SyncError> + Send + 'static) -> Result<T, SyncError> {
        let (root, lock) = (self.root.clone(), Arc::clone(&self.lock));
        tokio::task::spawn_blocking(move || {
            present(&root)?;
            work(&root, &lock)
        })
        .await
        .map_err(|_| SyncError::Interrupted)?
    }
}

/// A name the store keeps: lowercase letters, digits and dashes.
fn plain(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// An object's name in a listing: a plain name and the extension. The drive's own files (its
/// conflicted copies, the files it is downloading) are not.
pub(crate) fn object_name(name: &str) -> bool {
    name.strip_suffix(EXTENSION).is_some_and(plain)
}

fn etag(bytes: &[u8]) -> String {
    HEXLOWER.encode(&Sha256::digest(bytes))
}

/// The space's directories `dirs` below `root`, each one there as a directory of its own, not a
/// link. `make` makes the missing ones (a write); otherwise a missing one answers `false`.
fn walk(root: &Path, dirs: &[String], make: bool) -> Result<bool, SyncError> {
    let mut place = root.to_path_buf();
    for dir in dirs {
        place.push(dir);
        match fs::symlink_metadata(&place) {
            Ok(meta) if meta.file_type().is_dir() => {}
            Ok(_) => return Err(SyncError::Storage(format!("not a folder of the space (a link?): {}", place.display()))),
            Err(error) if error.kind() == io::ErrorKind::NotFound && make => match fs::create_dir(&place) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists && fs::symlink_metadata(&place).is_ok_and(|m| m.file_type().is_dir()) => {}
                Err(error) => return Err(failed(&error)),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(failed(&error)),
        }
    }
    Ok(true)
}

/// `path`'s directories and, for an object, its name.
fn split(path: &str) -> Result<(Vec<String>, Option<String>), SyncError> {
    let mut segments: Vec<String> = FolderStore::segments(path)?.into_iter().map(str::to_owned).collect();
    let name = if path.ends_with('/') { None } else { segments.pop() };
    Ok((segments, name))
}

fn under(root: &Path, dirs: &[String]) -> PathBuf {
    dirs.iter().fold(root.to_path_buf(), |place, dir| place.join(dir))
}

/// The folder is there, and is one.
fn present(root: &Path) -> Result<(), SyncError> {
    match fs::metadata(root) {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(SyncError::Storage(format!("not a folder: {}", root.display()))),
        Err(error) => Err(failed(&error)),
    }
}

/// An I/O error as the sync engine tells them apart: a folder gone, a refusal, anything else.
fn failed(error: &io::Error) -> SyncError {
    match error.kind() {
        io::ErrorKind::NotFound => SyncError::FolderMissing,
        io::ErrorKind::PermissionDenied => SyncError::Denied,
        _ => SyncError::Storage(error.to_string()),
    }
}

/// The drive's client holding the file (Windows: a sharing or lock violation, or access refused
/// while it has the file open): worth another try in a moment.
fn held(error: &io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

/// `work` again while the drive holds the file, up to about two seconds.
fn patiently<T>(mut work: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut tries = 0;
    loop {
        match work() {
            Err(error) if held(&error) && tries < 20 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            other => return other,
        }
    }
}

/// The object at `place` and its etag when there is one; one larger than an object may be is not
/// read, even when it grows while being read, and a link or a directory where it goes is no
/// object (reported as a damaged one, which the next write replaces).
fn read(place: &Path) -> Result<Option<(Vec<u8>, String)>, SyncError> {
    match fs::symlink_metadata(place) {
        Ok(meta) if !meta.file_type().is_file() => return Err(SyncError::Corrupted),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failed(&error)),
    }
    let file = match patiently(|| File::open(place)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failed(&error)),
    };
    if file.metadata().map_err(|e| failed(&e))?.len() > MAX_OBJECT_BYTES {
        return Err(SyncError::Corrupted);
    }
    let mut bytes = Vec::new();
    file.take(MAX_OBJECT_BYTES + 1).read_to_end(&mut bytes).map_err(|e| failed(&e))?;
    if bytes.len() as u64 > MAX_OBJECT_BYTES {
        return Err(SyncError::Corrupted);
    }
    let etag = etag(&bytes);
    Ok(Some((bytes, etag)))
}

impl RemoteStore for FolderStore {
    fn conditional_puts(&self) -> bool {
        false
    }

    fn list<'a>(&'a self, dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>> {
        Box::pin(async move {
            if !dir.ends_with('/') {
                return Err(SyncError::Storage(format!("not a directory: {dir}")));
            }
            let (dirs, _) = split(dir)?;
            self.blocking(move |root, lock| {
                let _reading = lock.read();
                // No space here yet (the folder itself is there).
                if !walk(root, &dirs, false)? {
                    return Ok(Vec::new());
                }
                let entries = match fs::read_dir(under(root, &dirs)) {
                    Ok(entries) => entries,
                    // No space here yet (the folder itself is there).
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
                    let etag = if size > MAX_OBJECT_BYTES {
                        None
                    } else {
                        match read(&entry.path()) {
                            Ok(found) => found.map(|(_, etag)| etag),
                            // Grown past the limit meanwhile: the engine refuses it by its size.
                            Err(SyncError::Corrupted) => None,
                            Err(other) => return Err(other),
                        }
                    };
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
            let (dirs, Some(name)) = split(path)? else {
                return Err(SyncError::Storage(format!("not an object: {path}")));
            };
            self.blocking(move |root, lock| {
                let _reading = lock.read();
                if !walk(root, &dirs, false)? {
                    return Ok(None);
                }
                Ok(read(&under(root, &dirs).join(name))?.map(|(bytes, etag)| (bytes, Some(etag))))
            })
            .await
        })
    }

    fn put<'a>(&'a self, path: &'a str, bytes: Vec<u8>, _condition: PutCondition) -> RemoteFuture<'a, Option<String>> {
        Box::pin(async move {
            let (dirs, Some(name)) = split(path)? else {
                return Err(SyncError::Storage(format!("not an object a space keeps: {path}")));
            };
            if bytes.len() as u64 > MAX_OBJECT_BYTES {
                return Err(SyncError::Storage(format!("not an object a space keeps: {path}")));
            }
            self.blocking(move |root, lock| {
                let _writing = lock.write();
                // The space's folders, one by one under the folder (which is never made here: a
                // drive that went away would otherwise get a new, empty copy of it), none a link.
                walk(root, &dirs, true)?;
                let place = under(root, &dirs).join(name);
                // The temporary file is made anew and renamed over the object: a link at either
                // name is replaced, never written through.
                patiently(|| lockra_vault::replace_atomic(&place, &bytes)).map_err(|e| failed(&e))?;
                Ok(Some(etag(&bytes)))
            })
            .await
        })
    }

    fn delete<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, ()> {
        Box::pin(async move {
            let (dirs, Some(name)) = split(path)? else {
                return Err(SyncError::Storage(format!("not an object: {path}")));
            };
            self.blocking(move |root, lock| {
                let _writing = lock.write();
                if !walk(root, &dirs, false)? {
                    return Ok(());
                }
                // A link where the object goes is removed itself, not what it points to.
                let place = under(root, &dirs).join(name);
                match patiently(|| fs::remove_file(&place)) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(failed(&error)),
                }
            })
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = "lockra-sync-v1/7d1f3c2e-0b4a-4c55-9a43-1e2f3a4b5c6d/devices/";
    const TAG: &str = "0123456789abcdef0123456789abcdef";

    fn object() -> String {
        format!("{DIR}{TAG}{EXTENSION}")
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn objects_are_written_read_listed_and_deleted_with_their_hash_as_etag() {
        let folder = tempfile::tempdir().unwrap();
        let store = FolderStore::new(folder.path());
        assert!(!store.conditional_puts(), "a cloud drive has no conditions, like WebDAV");
        assert_eq!(store.list(DIR).await.unwrap(), Vec::new(), "no space yet: nothing listed");
        assert_eq!(store.get(&object()).await.unwrap(), None);
        let etag = store.put(&object(), b"one".to_vec(), PutCondition::IfAbsent).await.unwrap();
        assert_eq!(etag.as_deref(), Some(HEXLOWER.encode(&Sha256::digest(b"one")).as_str()));
        assert_eq!(store.get(&object()).await.unwrap(), Some((b"one".to_vec(), etag.clone())));
        assert_eq!(store.list(DIR).await.unwrap(), [ObjectMeta { name: format!("{TAG}{EXTENSION}"), etag: etag.clone(), size: 3 }]);
        // Conditions are not honoured: the drive could not keep them anyway.
        let second = store.put(&object(), b"two".to_vec(), PutCondition::IfMatch("stale".into())).await.unwrap();
        assert_ne!(second, etag);
        let devices = folder.path().join(DIR.trim_end_matches('/'));
        assert_eq!(names(&devices), [format!("{TAG}{EXTENSION}")], "no .prev and no .tmp for the drive to carry");
        store.delete(&object()).await.unwrap();
        store.delete(&object()).await.unwrap();
        assert_eq!(store.list(DIR).await.unwrap(), Vec::new());
    }

    #[tokio::test]
    async fn only_paths_of_a_space_are_reached() {
        let folder = tempfile::tempdir().unwrap();
        let store = FolderStore::new(folder.path().join("space"));
        fs::create_dir(folder.path().join("space")).unwrap();
        for bad in [
            "../outside.lks",
            "lockra-sync-v1/../../outside.lks",
            "lockra-sync-v1/x/devices/UPPER.lks",
            "lockra-sync-v1\\x\\devices\\a.lks",
            "/etc/passwd",
            "lockra-sync-v1//devices/a.lks",
            "lockra-sync-v1/x/devices/a.txt",
            "lockra-sync-v1/x/devices/.lks",
        ] {
            assert!(matches!(store.put(bad, b"x".to_vec(), PutCondition::Always).await, Err(SyncError::Storage(_))), "{bad}");
            assert!(matches!(store.get(bad).await, Err(SyncError::Storage(_))), "{bad}");
        }
        assert!(matches!(store.list("lockra-sync-v1/x/devices").await, Err(SyncError::Storage(_))), "a directory ends in /");
        assert!(matches!(store.put(DIR, b"x".to_vec(), PutCondition::Always).await, Err(SyncError::Storage(_))));
        assert_eq!(names(folder.path()), ["space"], "nothing written outside");
    }

    /// A link inside the folder (a drive or a sync peer such as Syncthing can bring one) is never
    /// followed: no write lands outside the folder, and nothing outside is read.
    #[cfg(unix)]
    #[tokio::test]
    async fn links_inside_the_folder_are_never_followed() {
        use std::os::unix::fs::symlink;
        let folder = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let store = FolderStore::new(folder.path());
        // A folder of the space that is a link to another place.
        symlink(elsewhere.path(), folder.path().join("lockra-sync-v1")).unwrap();
        assert!(matches!(store.put(&object(), b"x".to_vec(), PutCondition::Always).await, Err(SyncError::Storage(_))));
        assert!(matches!(store.list(DIR).await, Err(SyncError::Storage(_))));
        assert!(matches!(store.get(&object()).await, Err(SyncError::Storage(_))));
        assert!(matches!(store.delete(&object()).await, Err(SyncError::Storage(_))));
        assert_eq!(names(elsewhere.path()), Vec::<String>::new(), "nothing written outside");
        fs::remove_file(folder.path().join("lockra-sync-v1")).unwrap();
        // The temporary file's name, a link to a file outside: replaced, never written through.
        store.put(&object(), b"one".to_vec(), PutCondition::Always).await.unwrap();
        let victim = elsewhere.path().join("victim");
        fs::write(&victim, b"keep").unwrap();
        let devices = folder.path().join(DIR.trim_end_matches('/'));
        symlink(&victim, devices.join(format!("{TAG}{EXTENSION}.tmp"))).unwrap();
        store.put(&object(), b"two".to_vec(), PutCondition::Always).await.unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        assert_eq!(store.get(&object()).await.unwrap().unwrap().0, b"two");
        // An object that is a link: not read (it does not open, as a damaged one), and the next
        // write puts a file in its place.
        fs::remove_file(devices.join(format!("{TAG}{EXTENSION}"))).unwrap();
        symlink(&victim, devices.join(format!("{TAG}{EXTENSION}"))).unwrap();
        assert_eq!(store.get(&object()).await, Err(SyncError::Corrupted));
        store.put(&object(), b"three".to_vec(), PutCondition::Always).await.unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        assert!(!fs::symlink_metadata(devices.join(format!("{TAG}{EXTENSION}"))).unwrap().file_type().is_symlink());
    }

    #[tokio::test]
    async fn the_drives_own_files_are_no_objects() {
        let folder = tempfile::tempdir().unwrap();
        let store = FolderStore::new(folder.path());
        store.put(&object(), b"mine".to_vec(), PutCondition::Always).await.unwrap();
        let devices = folder.path().join(DIR.trim_end_matches('/'));
        for other in [
            format!("{TAG} (Laptop's conflicted copy 2026-10-05){EXTENSION}"),
            format!("{TAG}-DESKTOP-4F2K{EXTENSION}"),
            format!("{TAG}.sync-conflict-20261005-101010-ABCDEFG{EXTENSION}"),
            format!("{TAG}{EXTENSION}.tmp"),
            format!(".~{TAG}{EXTENSION}"),
            "desktop.ini".to_owned(),
        ] {
            fs::write(devices.join(other), b"x").unwrap();
        }
        fs::create_dir(devices.join(format!("folder{EXTENSION}"))).unwrap();
        let listed: Vec<String> = store.list(DIR).await.unwrap().into_iter().map(|m| m.name).collect();
        assert_eq!(listed, [format!("{TAG}{EXTENSION}")]);
    }

    #[tokio::test]
    async fn an_object_larger_than_any_snapshot_is_listed_and_never_read() {
        let folder = tempfile::tempdir().unwrap();
        let store = FolderStore::new(folder.path());
        store.put(&object(), b"small".to_vec(), PutCondition::Always).await.unwrap();
        let file = File::options().write(true).open(folder.path().join(object())).unwrap();
        file.set_len(MAX_OBJECT_BYTES + 1).unwrap();
        assert_eq!(store.list(DIR).await.unwrap(), [ObjectMeta { name: format!("{TAG}{EXTENSION}"), etag: None, size: MAX_OBJECT_BYTES + 1 }]);
        assert_eq!(store.get(&object()).await, Err(SyncError::Corrupted));
        assert!(matches!(
            store.put(&object(), vec![0; usize::try_from(MAX_OBJECT_BYTES).unwrap() + 1], PutCondition::Always).await,
            Err(SyncError::Storage(_))
        ));
    }

    #[tokio::test]
    async fn a_missing_folder_is_reported_and_never_made_again() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("OneDrive").join("Lockra");
        let store = FolderStore::new(&root);
        assert_eq!(store.list(DIR).await, Err(SyncError::FolderMissing));
        assert_eq!(store.get(&object()).await, Err(SyncError::FolderMissing));
        assert_eq!(store.put(&object(), b"x".to_vec(), PutCondition::Always).await, Err(SyncError::FolderMissing));
        assert_eq!(store.delete(&object()).await, Err(SyncError::FolderMissing));
        assert!(!root.exists() && !parent.path().join("OneDrive").exists(), "the drive's folder is not made here");
        // Back again (the drive reconnected): the space's folders under it are made as needed.
        fs::create_dir_all(&root).unwrap();
        store.put(&object(), b"x".to_vec(), PutCondition::Always).await.unwrap();
        assert_eq!(store.list(DIR).await.unwrap().len(), 1);
        // A file where the folder should be is no folder either.
        let file = parent.path().join("file");
        fs::write(&file, b"x").unwrap();
        assert!(matches!(FolderStore::new(&file).list(DIR).await, Err(SyncError::Storage(_))));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_folder_this_user_may_not_write_is_refused_as_denied() {
        use std::os::unix::fs::PermissionsExt as _;
        let folder = tempfile::tempdir().unwrap();
        fs::set_permissions(folder.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let store = FolderStore::new(folder.path());
        let result = store.put(&object(), b"x".to_vec(), PutCondition::Always).await;
        fs::set_permissions(folder.path(), fs::Permissions::from_mode(0o700)).unwrap();
        // Root ignores permissions: only an ordinary user sees the refusal.
        if result.is_err() {
            assert_eq!(result, Err(SyncError::Denied));
        }
    }

    /// The drive's client holds the file while it uploads it: Windows refuses to replace it until
    /// the client lets go, and the write tries again meanwhile.
    #[cfg(windows)]
    #[tokio::test]
    async fn a_file_the_drive_holds_for_a_moment_is_replaced_once_it_lets_go() {
        use std::os::windows::fs::OpenOptionsExt as _;
        let folder = tempfile::tempdir().unwrap();
        let store = FolderStore::new(folder.path());
        store.put(&object(), b"one".to_vec(), PutCondition::Always).await.unwrap();
        let held = File::options().read(true).share_mode(0).open(folder.path().join(object())).unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(held);
        });
        store.put(&object(), b"two".to_vec(), PutCondition::Always).await.unwrap();
        release.join().unwrap();
        assert_eq!(store.get(&object()).await.unwrap().unwrap().0, b"two");
    }
}
