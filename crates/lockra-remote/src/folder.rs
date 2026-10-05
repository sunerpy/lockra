//! A sync space in a folder of this computer that a cloud drive's client keeps in sync (OneDrive,
//! iCloud Drive, Dropbox, Jianguoyun, Nextcloud, Synology Drive, Syncthing, …): the drive carries
//! the snapshots between devices, Lockra only reads and writes files.
//!
//! The drive is a storage with no conditions at all, like WebDAV: whatever this computer checks,
//! the drive may still bring another device's copy in later. It also leaves files of its own
//! beside the snapshots (conflicted copies, files it is downloading), which are no object of the
//! space; and it may deliver a snapshot in pieces, which then does not open this round and is read
//! again the next (docs/security.md, "Sync").

use std::ffi::OsStr;
use std::io::{self, Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _, OpenOptionsMaybeDirExt as _};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use data_encoding::HEXLOWER;
use lockra_sync::{MAX_OBJECT_BYTES, ObjectMeta, PutCondition, RemoteFuture, RemoteStore, SyncError};
use parking_lot::RwLock;
use sha2::{Digest as _, Sha256};

/// The extension of every object a space keeps.
const EXTENSION: &str = ".lks";

/// A sync space's storage in a folder. Paths are those of a space (lowercase letters, digits and
/// dashes, an object's name ending in `.lks`); anything else is refused, `..` included.
///
/// Every operation goes through directory handles opened one folder at a time from the top of the
/// file system, none of them through a link (the core keeps the folder's path with every link
/// resolved when it was chosen), and acts on names inside the last one only. So nothing outside
/// the folder is reached, even while another program swaps a folder on the way for a link (a
/// drive or a sync peer may bring one): a linked folder is refused, a linked object is no object,
/// and a write that started goes on in the folder it opened. A write replaces its file atomically
/// and keeps no copy beside it (the drive would carry that too). The folder itself is never made:
/// while it is missing (moved, deleted, a drive not connected) every call is
/// [`SyncError::FolderMissing`]. An object's etag is the SHA-256 of its bytes.
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

    /// Run `work` off the async runtime on the folder, opened first: file I/O blocks.
    async fn blocking<T: Send + 'static>(&self, work: impl FnOnce(&Dir, &RwLock<()>) -> Result<T, SyncError> + Send + 'static) -> Result<T, SyncError> {
        let (root, lock) = (self.root.clone(), Arc::clone(&self.lock));
        tokio::task::spawn_blocking(move || work(&open_root(&root)?, &lock)).await.map_err(|_| SyncError::Interrupted)?
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

/// The folder `root` as a handle: the top of its file system (`/`, `C:\`), then each folder on the
/// way opened without following a link. A link anywhere on the way is refused; a folder missing
/// on the way is [`SyncError::FolderMissing`].
fn open_root(root: &Path) -> Result<Dir, SyncError> {
    let parts: Vec<Component<'_>> = root.components().collect();
    let first = parts.iter().position(|part| matches!(part, Component::Normal(_))).unwrap_or(parts.len());
    let top: PathBuf = parts[..first].iter().collect();
    if !root.is_absolute() || top.as_os_str().is_empty() {
        return Err(SyncError::Storage(format!("not a folder named in full: {}", root.display())));
    }
    let mut dir = Dir::open_ambient_dir(&top, ambient_authority()).map_err(|e| failed(&e))?;
    for part in &parts[first..] {
        let Component::Normal(name) = part else {
            return Err(SyncError::Storage(format!("not a folder named in full: {}", root.display())));
        };
        dir = step(&dir, name, false)?.ok_or(SyncError::FolderMissing)?;
    }
    Ok(dir)
}

/// The folder `name` in `dir`, opened without following a link; `None` when it is not there, or
/// made first with `make`. Anything else at `name` (a link, a file) is refused.
fn step(dir: &Dir, name: &OsStr, make: bool) -> Result<Option<Dir>, SyncError> {
    let refused = || SyncError::Storage(format!("not a folder (a link?): {}", Path::new(name).display()));
    match dir.symlink_metadata(name) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(refused()),
        Err(error) if error.kind() == io::ErrorKind::NotFound && make => match dir.create_dir(name) {
            // Made meanwhile by another program: opened below, where a link is refused.
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(failed(&error)),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failed(&error)),
    }
    match dir.open_dir_nofollow(name) {
        Ok(opened) => Ok(Some(opened)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Err(SyncError::Denied),
        // Swapped for a link (or a file) since it was looked at.
        Err(_) => Err(refused()),
    }
}

/// The space's directories `dirs` in the folder, opened one by one (made with `make`); `None`
/// when one is not there yet.
fn walk(root: &Dir, dirs: &[String], make: bool) -> Result<Option<Dir>, SyncError> {
    let mut dir = root.try_clone().map_err(|e| failed(&e))?;
    for name in dirs {
        match step(&dir, OsStr::new(name), make)? {
            Some(next) => dir = next,
            None => return Ok(None),
        }
    }
    Ok(Some(dir))
}

/// `path`'s directories and, for an object, its name.
fn split(path: &str) -> Result<(Vec<String>, Option<String>), SyncError> {
    let mut segments: Vec<String> = FolderStore::segments(path)?.into_iter().map(str::to_owned).collect();
    let name = if path.ends_with('/') { None } else { segments.pop() };
    Ok((segments, name))
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

/// The object `name` in `dir` and its etag when there is one; one larger than an object may be is
/// not read, even when it grows while being read, and a link or a directory where it goes is no
/// object (reported as a damaged one, which the next write replaces).
fn read(dir: &Dir, name: &str) -> Result<Option<(Vec<u8>, String)>, SyncError> {
    match dir.symlink_metadata(name) {
        Ok(meta) if !meta.is_file() => return Err(SyncError::Corrupted),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failed(&error)),
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = match patiently(|| dir.open_with(name, &options)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Err(SyncError::Denied),
        // Swapped for a link since it was looked at: no object.
        Err(_) => return Err(SyncError::Corrupted),
    };
    let meta = file.metadata().map_err(|e| failed(&e))?;
    if !meta.is_file() || meta.len() > MAX_OBJECT_BYTES {
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

/// Replace the object `name` in `dir` with `bytes`: written to `<name>.tmp` made anew (whatever was
/// at that name goes first, a link itself rather than what it points to), flushed, and renamed over
/// the object, which replaces a link there rather than writing through it. On any failure before
/// the rename the old object is untouched and the temporary file goes.
fn replace(dir: &Dir, name: &str, bytes: &[u8]) -> io::Result<()> {
    let tmp = format!("{name}.tmp");
    match dir.remove_file(&tmp) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).follow(FollowSymlinks::No);
    #[cfg(unix)]
    cap_std::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = (|| {
        let mut file = dir.open_with(&tmp, &options)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        dir.rename(&tmp, dir, name)
    })();
    if written.is_err() {
        // Best effort: the error that stopped the write is the one the caller gets.
        let _ = dir.remove_file(&tmp);
    }
    written?;
    // The new directory entry, flushed on Unix (Windows offers no handle to flush it through),
    // through the directory opened for reading: the handle the walk holds may not be flushed.
    #[cfg(unix)]
    {
        let mut readable = OpenOptions::new();
        readable.read(true).maybe_dir(true).follow(FollowSymlinks::No);
        dir.open_with(".", &readable)?.sync_all()?;
    }
    Ok(())
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
                let Some(devices) = walk(root, &dirs, false)? else { return Ok(Vec::new()) };
                let mut listing = Vec::new();
                for entry in devices.entries().map_err(|e| failed(&e))? {
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
                        match read(&devices, &name) {
                            Ok(found) => found.map(|(_, etag)| etag),
                            // Grown past the limit, or swapped for a link, meanwhile: the engine
                            // reads it again, and finds what it is.
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
                let Some(devices) = walk(root, &dirs, false)? else { return Ok(None) };
                Ok(read(&devices, &name)?.map(|(bytes, etag)| (bytes, Some(etag))))
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
                let devices = walk(root, &dirs, true)?.ok_or_else(|| SyncError::Storage("the space's folders were not made".into()))?;
                patiently(|| replace(&devices, &name, &bytes)).map_err(|e| failed(&e))?;
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
                let Some(devices) = walk(root, &dirs, false)? else { return Ok(()) };
                // A link where the object goes is removed itself, not what it points to.
                match patiently(|| devices.remove_file(&name)) {
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
    use std::fs::{self, File};

    use super::*;
    /// A temporary folder named as the core keeps a chosen one, every link resolved (macOS keeps its
    /// temporary folders under `/var`, a link to `/private/var`).
    struct Temp {
        _dir: tempfile::TempDir,
        path: std::path::PathBuf,
    }

    impl Temp {
        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    fn tempdir() -> Temp {
        let dir = tempfile::TempDir::new().unwrap();
        let path = std::fs::canonicalize(dir.path()).unwrap();
        Temp { _dir: dir, path }
    }

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
        let folder = tempdir();
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
        let folder = tempdir();
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
        let folder = tempdir();
        let elsewhere = tempdir();
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

    /// The chosen folder replaced by a link after it was chosen (by a sync peer, say): the store
    /// goes nowhere through it, and writes nothing where it points.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_folder_replaced_by_a_link_is_not_followed() {
        let parent = tempdir();
        let elsewhere = tempdir();
        let root = parent.path().join("Lockra");
        fs::create_dir(&root).unwrap();
        let store = FolderStore::new(&root);
        store.put(&object(), b"one".to_vec(), PutCondition::Always).await.unwrap();
        fs::remove_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), &root).unwrap();
        assert!(matches!(store.put(&object(), b"two".to_vec(), PutCondition::Always).await, Err(SyncError::Storage(_))));
        assert!(matches!(store.list(DIR).await, Err(SyncError::Storage(_))));
        assert_eq!(names(elsewhere.path()), Vec::<String>::new(), "nothing written where the link points");
    }

    /// The folders swapped for links after they were checked and opened (by a sync peer, while a
    /// write runs): the write goes on in the folder it opened, nothing lands where the links point,
    /// and the next call refuses them.
    #[cfg(unix)]
    #[tokio::test]
    async fn folders_swapped_for_links_while_a_write_runs_keep_the_write_inside() {
        use std::os::unix::fs::symlink;
        let parent = tempdir();
        let elsewhere = tempdir();
        let root = parent.path().join("Lockra");
        fs::create_dir(&root).unwrap();
        let (dirs, name) = split(&object()).unwrap();
        let name = name.unwrap();
        let devices = walk(&open_root(&root).unwrap(), &dirs, true).unwrap().unwrap();
        // Checked and opened; now the space's folder and the chosen folder itself become links.
        fs::rename(root.join("lockra-sync-v1"), parent.path().join("space-moved")).unwrap();
        symlink(elsewhere.path(), root.join("lockra-sync-v1")).unwrap();
        fs::rename(&root, parent.path().join("Lockra-moved")).unwrap();
        symlink(elsewhere.path(), &root).unwrap();
        replace(&devices, &name, b"inside").unwrap();
        assert_eq!(names(elsewhere.path()), Vec::<String>::new(), "nothing where the links point");
        let inside = parent.path().join("space-moved").join(dirs[1..].join("/")).join(&name);
        assert_eq!(fs::read(inside).unwrap(), b"inside", "the write stayed in the folder it opened");
        let store = FolderStore::new(&root);
        assert!(matches!(store.put(&object(), b"next".to_vec(), PutCondition::Always).await, Err(SyncError::Storage(_))));
        assert_eq!(names(elsewhere.path()), Vec::<String>::new());
    }

    #[tokio::test]
    async fn the_drives_own_files_are_no_objects() {
        let folder = tempdir();
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
        let folder = tempdir();
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
        let parent = tempdir();
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
        let folder = tempdir();
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
        let folder = tempdir();
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
