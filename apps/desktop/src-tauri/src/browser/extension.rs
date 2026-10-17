//! The extension's folder: Chrome, Edge and Brave never update an extension loaded unpacked, so
//! Lockra carries the Chromium build of its own release (build.rs) and keeps it in a folder of its
//! own, the one Settings › 浏览器 tells the user to load. While the integration is on, every start
//! (before the channel listens) and every turning on makes the folder hold this build's copy. The
//! extension reads [`MARKER`] from its own package, sees a version other than the one it runs and
//! reloads itself (apps/extension/src/background/update.ts): the browser reads an unpacked
//! extension's folder again when it reloads.
//!
//! A new copy is written beside the folder, the marker last, and takes the folder's place in two
//! renames: a marker read from the folder is that of a complete copy. A folder whose marker names
//! this build's digest is left as it is.
//!
//! | System  | The folder                                              |
//! |---------|---------------------------------------------------------|
//! | Windows | `%LOCALAPPDATA%\Lockra\browser\extension` (beside the manifests) |
//! | macOS   | `~/Library/Application Support/Lockra/browser/extension` |
//! | Linux   | `~/.local/share/Lockra/browser/extension`               |

use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::manifest::Places;

/// The file written last into the folder: the version it holds and the digest of its files.
pub const MARKER: &str = "lockra-folder.json";

/// The releases' page, where each release has the extension's self-install package.
const RELEASES_URL: &str = "https://github.com/sunerpy/lockra/releases";

/// How often a rename is tried, and the pause before the next try (Windows refuses to move a folder
/// while a file in it is open, as a scanner may hold one it has just seen written).
const RENAME_TRIES: u32 = 5;
const RENAME_PAUSE: Duration = Duration::from_millis(100);

mod carried {
    include!(concat!(env!("OUT_DIR"), "/extension.rs"));
}

/// An extension to keep in the folder: its files by their path in the package (`/` between the
/// parts), the version its manifest names and the digest of them all.
#[derive(Debug, Clone)]
pub struct Bundle<'a> {
    files: &'a [(&'a str, &'a [u8])],
    version: String,
    digest: String,
}

impl<'a> Bundle<'a> {
    /// `files`; none without a `manifest.json` that names a version.
    pub fn new(files: &'a [(&'a str, &'a [u8])]) -> Option<Self> {
        let (_, manifest) = files.iter().find(|(name, _)| *name == "manifest.json")?;
        let manifest: serde_json::Value = serde_json::from_slice(manifest).ok()?;
        let version = manifest.get("version")?.as_str()?.to_owned();
        let mut sorted: Vec<_> = files.iter().collect();
        sorted.sort_by_key(|(name, _)| *name);
        let mut hash = Sha256::new();
        for (name, bytes) in sorted {
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        Some(Self { files, version, digest: format!("sha256:{}", HEXLOWER.encode(&hash.finalize())) })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// The extension this build carries; none in a build made before the extension was.
pub fn carried() -> Option<&'static Bundle<'static>> {
    static BUNDLE: OnceLock<Option<Bundle<'static>>> = OnceLock::new();
    BUNDLE.get_or_init(|| Bundle::new(carried::FILES)).as_ref()
}

/// What the folder's [`MARKER`] says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub version: String,
    pub digest: String,
}

/// The folder, in the local data folder.
pub fn folder(places: &Places) -> PathBuf {
    places.local_data.join("Lockra").join("browser").join("extension")
}

/// The folder's marker, when it has a readable one.
pub fn marker(folder: &Path) -> Option<Marker> {
    serde_json::from_slice(&std::fs::read(folder.join(MARKER)).ok()?).ok()
}

/// What Settings › 浏览器 shows of the folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtensionFolder {
    /// Where it is, to show: the user loads the extension from there.
    pub path: String,
    /// The version this build carries.
    pub version: String,
    /// The folder holds this build's copy.
    pub ready: bool,
}

pub fn status(folder: &Path, bundle: &Bundle<'_>) -> ExtensionFolder {
    ExtensionFolder { path: folder.display().to_string(), version: bundle.version.clone(), ready: marker(folder).is_some_and(|m| m.digest == bundle.digest) }
}

/// What [`sync`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Synced {
    /// The folder held the bundle already.
    Unchanged,
    /// The bundle took the folder's place.
    Written,
}

/// Make `folder` hold `bundle`. What an interrupted run left beside it goes first; when a rename
/// fails the folder keeps what it had (the next start tries again).
pub fn sync(folder: &Path, bundle: &Bundle<'_>) -> io::Result<Synced> {
    if marker(folder).is_some_and(|m| m.digest == bundle.digest) {
        return Ok(Synced::Unchanged);
    }
    let staging = beside(folder, "staging")?;
    let old = beside(folder, "old")?;
    remove_dir(&staging)?;
    remove_dir(&old)?;
    write_copy(&staging, bundle)?;
    let had = folder.exists();
    if had {
        retried(|| std::fs::rename(folder, &old))?;
    }
    if let Err(error) = retried(|| std::fs::rename(&staging, folder)) {
        if had && let Err(back) = retried(|| std::fs::rename(&old, folder)) {
            tracing::warn!(error = %back, "the extension's old folder could not be put back");
        }
        return Err(error);
    }
    if let Err(error) = remove_dir(&old) {
        tracing::warn!(%error, "the extension's old folder stays beside the new one until the next start");
    }
    Ok(Synced::Written)
}

/// `folder.<what>`, in the same parent (a rename stays on one file system).
fn beside(folder: &Path, what: &str) -> io::Result<PathBuf> {
    let name = folder.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the extension's folder has no name"))?;
    Ok(folder.with_file_name(format!("{}.{what}", name.to_string_lossy())))
}

/// Every file of `bundle` into the new folder `dir`, then the marker.
fn write_copy(dir: &Path, bundle: &Bundle<'_>) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, bytes) in bundle.files {
        let path = dir.join(inside(name)?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
    }
    let marker = Marker { version: bundle.version.clone(), digest: bundle.digest.clone() };
    std::fs::write(dir.join(MARKER), serde_json::to_vec_pretty(&marker).map_err(io::Error::other)?)
}

/// A file's name as a path under the folder: plain parts only.
fn inside(name: &str) -> io::Result<PathBuf> {
    let path: PathBuf = name.split('/').collect();
    let plain = name != MARKER && name.split('/').all(|part| !part.is_empty()) && path.components().all(|part| matches!(part, Component::Normal(_)));
    if !plain {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("the extension's file {name:?} is not a plain name")));
    }
    Ok(path)
}

fn retried(mut rename: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    let mut tries = 0;
    loop {
        match rename() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(error),
            Err(error) => {
                tries += 1;
                if tries >= RENAME_TRIES {
                    return Err(error);
                }
                std::thread::sleep(RENAME_PAUSE * tries);
            }
        }
    }
}

fn remove_dir(path: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The self-install package of the extension of `version` on its release: the folder's files, for
/// loading by hand where the folder will not do (it does not update with Lockra).
pub fn download_url(version: &str) -> String {
    format!("{RELEASES_URL}/download/v{version}/lockra-extension_{version}_chromium-unpacked.zip")
}

/// The file manager showing the folder: Explorer and Finder with it selected, on Linux its parent
/// (`xdg-open` selects nothing). Whether one started.
pub fn reveal(folder: &Path) -> bool {
    started(reveal_command(folder), "no file manager could show the extension's folder")
}

/// `url` in the system's browser. Whether one started.
pub fn open_url(url: &str) -> bool {
    started(url_command(url), "no browser could open the address")
}

fn started(mut command: std::process::Command, failed: &str) -> bool {
    match command.spawn() {
        Ok(mut child) => {
            // Reaped on a thread of its own: Explorer answers 1 even when it did what it was asked.
            std::thread::spawn(move || child.wait());
            true
        }
        Err(error) => {
            tracing::warn!(%error, "{failed}");
            false
        }
    }
}

#[cfg(windows)]
fn url_command(url: &str) -> std::process::Command {
    let mut command = std::process::Command::new("explorer.exe");
    command.arg(url);
    command
}

#[cfg(target_os = "macos")]
fn url_command(url: &str) -> std::process::Command {
    let mut command = std::process::Command::new("open");
    command.arg(url);
    command
}

#[cfg(not(any(windows, target_os = "macos")))]
fn url_command(url: &str) -> std::process::Command {
    let mut command = std::process::Command::new("xdg-open");
    command.arg(url);
    command
}

#[cfg(windows)]
fn reveal_command(folder: &Path) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new("explorer.exe");
    // `/select,` and the quoted path as one argument, as Explorer parses it.
    command.raw_arg(format!("/select,\"{}\"", folder.display()));
    command
}

#[cfg(target_os = "macos")]
fn reveal_command(folder: &Path) -> std::process::Command {
    let mut command = std::process::Command::new("open");
    command.arg("-R").arg(folder);
    command
}

#[cfg(not(any(windows, target_os = "macos")))]
fn reveal_command(folder: &Path) -> std::process::Command {
    let mut command = std::process::Command::new("xdg-open");
    command.arg(folder.parent().unwrap_or(folder));
    command
}

#[cfg(test)]
mod tests;
