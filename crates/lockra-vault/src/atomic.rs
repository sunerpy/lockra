//! Replacing a file so that it always holds either the old or the new content in full.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// The new content is in `<file>.tmp`, not yet flushed.
    Written,
    /// The current file has been copied to `<file>.prev.tmp` and flushed, not yet renamed.
    PreviousCopied,
    /// The current file has been copied to `<file>.prev`.
    PreviousKept,
}

/// Replace `path` with `bytes`: write `<file>.tmp` and flush it to disk, keep the current file as
/// `<file>.prev`, rename the new file over the old one (atomic on the same file system, Windows
/// included), then flush the directory entry on Unix. On any failure before the rename the old
/// file is untouched and the temporary files are removed. New files are readable by the owner only.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_with(path, bytes, &mut |_| Ok(()))
}

pub(crate) fn write_with(path: &Path, bytes: &[u8], hook: &mut dyn FnMut(Step) -> io::Result<()>) -> io::Result<()> {
    let tmp = sibling(path, ".tmp");
    let prev_tmp = sibling(path, ".prev.tmp");
    let result = replace(path, &tmp, &prev_tmp, bytes, hook);
    if result.is_err() {
        // Best effort: the error that stopped the write is the one the caller gets.
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_file(&prev_tmp);
    }
    result
}

fn replace(path: &Path, tmp: &Path, prev_tmp: &Path, bytes: &[u8], hook: &mut dyn FnMut(Step) -> io::Result<()>) -> io::Result<()> {
    let mut file = create_private(tmp)?;
    file.write_all(bytes)?;
    hook(Step::Written)?;
    file.sync_all()?;
    drop(file);
    if path.exists() {
        // Written and flushed through the handle that created the copy: Windows refuses to flush
        // a handle opened for reading only (FlushFileBuffers: "Access is denied", os error 5).
        let mut copy = create_private(prev_tmp)?;
        io::copy(&mut File::open(path)?, &mut copy)?;
        copy.sync_all()?;
        drop(copy);
        hook(Step::PreviousCopied)?;
        fs::rename(prev_tmp, sibling(path, ".prev"))?;
    }
    hook(Step::PreviousKept)?;
    fs::rename(tmp, path)?;
    sync_parent(path)
}

/// `vault.lockra` → `vault.lockra.tmp`.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// A temporary file made anew: whatever is at its name goes first (a link is removed, never
/// followed: another program may share the folder, a cloud drive's peer among them), and the file
/// is created exclusively, failing rather than opening anything that appears there meanwhile.
fn create_private(path: &Path) -> io::Result<File> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> io::Result<()> {
    match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(dir) => File::open(dir)?.sync_all(),
        None => Ok(()),
    }
}

/// Windows offers no directory handle to flush through `std`; the rename itself is durable there.
#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fail_at(step: Step) -> impl FnMut(Step) -> io::Result<()> {
        move |s| if s == step { Err(io::Error::other("injected")) } else { Ok(()) }
    }

    #[test]
    fn creates_then_replaces_keeping_the_previous_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.lockra");
        write_atomic(&path, b"one").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"one");
        assert!(!dir.path().join("vault.lockra.prev").exists());
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(fs::read(dir.path().join("vault.lockra.prev")).unwrap(), b"one");
        assert!(!dir.path().join("vault.lockra.tmp").exists());
        assert!(!dir.path().join("vault.lockra.prev.tmp").exists());
    }

    #[test]
    fn a_failure_before_the_rename_leaves_the_old_file_intact() {
        for step in [Step::Written, Step::PreviousCopied, Step::PreviousKept] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("vault.lockra");
            write_atomic(&path, b"old").unwrap();
            let err = write_with(&path, b"new", &mut fail_at(step)).unwrap_err();
            assert_eq!(err.to_string(), "injected");
            assert_eq!(fs::read(&path).unwrap(), b"old", "{step:?}");
            assert!(!dir.path().join("vault.lockra.tmp").exists(), "{step:?}");
            assert!(!dir.path().join("vault.lockra.prev.tmp").exists(), "{step:?}");
        }
    }

    /// What a failed save left behind on Windows up to 0.7.1: `<file>.prev.tmp`, a copy of the
    /// current file. The next save replaces it and leaves nothing behind.
    #[test]
    fn a_copy_left_by_an_earlier_failure_does_not_get_in_the_way() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.lockra");
        write_atomic(&path, b"one").unwrap();
        fs::write(dir.path().join("vault.lockra.prev.tmp"), b"one, copied by a save that failed").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(fs::read(dir.path().join("vault.lockra.prev")).unwrap(), b"one");
        assert!(!dir.path().join("vault.lockra.prev.tmp").exists());
    }

    /// A link where a temporary file goes (left there by another program) is replaced, never
    /// written through.
    #[cfg(unix)]
    #[test]
    fn a_link_where_the_temporary_file_goes_is_not_written_through() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let victim = elsewhere.path().join("victim");
        fs::write(&victim, b"keep").unwrap();
        let path = dir.path().join("vault.lockra");
        std::os::unix::fs::symlink(&victim, dir.path().join("vault.lockra.tmp")).unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(!fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        let names: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["vault.lockra"]);
    }

    #[test]
    fn a_missing_directory_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_atomic(&dir.path().join("missing").join("vault.lockra"), b"x").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn new_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.lockra");
        write_atomic(&path, b"x").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
