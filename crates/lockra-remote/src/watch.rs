//! Hearing when a cloud drive brings a snapshot into a space's folder: the operating system's file
//! events (inotify, FSEvents, ReadDirectoryChangesW, through notify), so that a run follows within
//! a moment rather than at the next look. A missed event costs only time: the runs still look at
//! the folder at their intervals.

use std::path::Path;

use lockra_sync::{StorageConfig, SyncError};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

use crate::folder::object_name;

/// A watch on one directory of a space's folder; dropping it stops the watch.
pub struct FolderWatch {
    _watcher: RecommendedWatcher,
}

impl std::fmt::Debug for FolderWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FolderWatch").finish_non_exhaustive()
    }
}

impl FolderWatch {
    /// Watch `dir` itself (not what is below it): `changed` is called, from the watcher's own
    /// thread, whenever an object of the space in it appears, changes or goes.
    pub fn start(dir: &Path, changed: Box<dyn Fn() + Send + Sync>) -> Result<Self, SyncError> {
        // Checked here: notify tells a missing path apart on some systems only (Windows calls it
        // neither a file nor a directory).
        if !dir.is_dir() {
            return Err(SyncError::FolderMissing);
        }
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if event.is_ok_and(|event| concerns(&event)) {
                changed();
            }
        })
        .map_err(|e| failed(&e))?;
        watcher.watch(dir, RecursiveMode::NonRecursive).map_err(|e| failed(&e))?;
        Ok(Self { _watcher: watcher })
    }
}

/// A watch on the directory `dir` (ending in `/`, as the sync engine names it) of `config`'s
/// storage, where the storage can tell: a folder of this computer. `None` for the others.
pub fn watch(config: &StorageConfig, dir: &str, changed: Box<dyn Fn() + Send + Sync>) -> Result<Option<FolderWatch>, SyncError> {
    let StorageConfig::Folder { path } = config else { return Ok(None) };
    let place = dir.trim_end_matches('/').split('/').filter(|s| !s.is_empty()).fold(path.clone(), |place, segment| place.join(segment));
    FolderWatch::start(&place, changed).map(Some)
}

/// An event about an object of the space: one of its files made, written, renamed or removed. Not
/// a read, and not the drive's own files beside them.
fn concerns(event: &Event) -> bool {
    !matches!(event.kind, EventKind::Access(_)) && event.paths.iter().any(|path| path.file_name().and_then(|n| n.to_str()).is_some_and(object_name))
}

fn failed(error: &notify::Error) -> SyncError {
    match &error.kind {
        notify::ErrorKind::PathNotFound => SyncError::FolderMissing,
        notify::ErrorKind::Io(io) if io.kind() == std::io::ErrorKind::NotFound => SyncError::FolderMissing,
        _ => SyncError::Storage(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;

    use lockra_sync::{PutCondition, RemoteStore as _};
    use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind, RenameMode};

    use super::*;
    use crate::FolderStore;

    const TAG: &str = "0123456789abcdef0123456789abcdef";

    fn event(kind: EventKind, names: &[&str]) -> Event {
        names.iter().fold(Event::new(kind), |event, name| event.add_path(PathBuf::from("/drive/devices").join(name)))
    }

    #[test]
    fn only_the_space_s_objects_count() {
        let object = format!("{TAG}.lks");
        for kind in [
            EventKind::Create(CreateKind::File),
            EventKind::Modify(ModifyKind::Any),
            EventKind::Remove(RemoveKind::File),
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
        ] {
            assert!(concerns(&event(kind, &[&object])), "{kind:?}");
        }
        // A rename from the drive's own name into the object's counts; reads and other files not.
        assert!(concerns(&event(EventKind::Modify(ModifyKind::Name(RenameMode::Both)), &[&format!("{object}.tmp"), &object])));
        assert!(!concerns(&event(EventKind::Access(AccessKind::Any), &[&object])));
        for other in [format!("{object}.tmp"), format!("{TAG} (conflicted copy).lks"), "desktop.ini".to_owned()] {
            assert!(!concerns(&event(EventKind::Create(CreateKind::File), &[&other])), "{other}");
        }
    }

    #[tokio::test]
    async fn a_snapshot_written_into_the_folder_is_heard() {
        let folder = tempfile::tempdir().unwrap();
        let dir = "lockra-sync-v1/7d1f3c2e-0b4a-4c55-9a43-1e2f3a4b5c6d/devices/";
        let store = FolderStore::new(folder.path());
        store.put(&format!("{dir}{TAG}.lks"), b"one".to_vec(), PutCondition::Always).await.unwrap();
        let (sender, heard) = mpsc::channel();
        let config = StorageConfig::Folder { path: folder.path().to_path_buf() };
        let _watch = watch(
            &config,
            dir,
            Box::new(move || {
                let _ = sender.send(());
            }),
        )
        .unwrap()
        .expect("a folder is watched");
        store.put(&format!("{dir}{TAG}.lks"), b"two".to_vec(), PutCondition::Always).await.unwrap();
        heard.recv_timeout(Duration::from_secs(10)).expect("the write was heard");
    }

    #[test]
    fn only_a_folder_that_is_there_is_watched() {
        let dav =
            StorageConfig::Webdav { url: "https://dav.example.com/".into(), prefix: String::new(), username: "me".into(), password: "pw".to_owned().into() };
        assert!(watch(&dav, "x/", Box::new(|| {})).unwrap().is_none(), "WebDAV tells nothing");
        let folder = tempfile::tempdir().unwrap();
        let missing = StorageConfig::Folder { path: folder.path().join("gone") };
        assert!(matches!(watch(&missing, "devices/", Box::new(|| {})), Err(SyncError::FolderMissing)));
    }
}
