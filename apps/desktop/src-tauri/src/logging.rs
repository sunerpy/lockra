//! Where the desktop app's log goes: stderr, and on macOS a file as well,
//! `~/Library/Logs/dev.lockra.desktop/lockra.log` (where Tauri's `app_log_dir` points), because an
//! app started from the Finder or the Dock has no stderr anyone reads. The staged build an update
//! starts logs there too, so a keychain hand-over that failed says why (docs/security.md,
//! "Keychain items across updates"). Nothing secret is ever logged; the file is the user's alone.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, fmt};

/// The log file's name in [`log_dir`].
pub const LOG_FILE: &str = "lockra.log";
/// Past this size the log starts over, the previous one kept as `lockra.log.1`.
pub const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// The log's folder: `~/Library/Logs/<bundle identifier>` on macOS; elsewhere none (stderr only).
pub fn log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Logs").join(crate::KEYCHAIN_SERVICE))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// The log file in `dir`, for appending, readable by its owner only; one grown past
/// [`MAX_LOG_BYTES`] becomes `lockra.log.1` first.
pub fn open_log(dir: &Path) -> io::Result<File> {
    fs::create_dir_all(dir)?;
    let path = dir.join(LOG_FILE);
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
        fs::rename(&path, dir.join(format!("{LOG_FILE}.1")))?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path)
}

/// Log to stderr and, where there is one, the log file (`RUST_LOG`, default `lockra=info`).
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "lockra=info".into());
    let file = log_dir().and_then(|dir| open_log(&dir).ok()).map(|file| fmt::layer().with_ansi(false).with_writer(Mutex::new(file)));
    let _ = tracing_subscriber::registry().with(filter).with(fmt::layer().with_writer(io::stderr)).with(file).try_init();
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    #[test]
    fn the_log_is_appended_to_and_private() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("Logs/dev.lockra.desktop");
        writeln!(open_log(&logs).unwrap(), "first").unwrap();
        writeln!(open_log(&logs).unwrap(), "second").unwrap();
        assert_eq!(fs::read_to_string(logs.join(LOG_FILE)).unwrap(), "first\nsecond\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(fs::metadata(logs.join(LOG_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn a_full_log_starts_over_and_keeps_the_previous_one() {
        let dir = tempfile::tempdir().unwrap();
        let big = vec![b'x'; usize::try_from(MAX_LOG_BYTES).unwrap() + 1];
        fs::write(dir.path().join(LOG_FILE), &big).unwrap();
        writeln!(open_log(dir.path()).unwrap(), "fresh").unwrap();
        assert_eq!(fs::read_to_string(dir.path().join(LOG_FILE)).unwrap(), "fresh\n");
        assert_eq!(fs::read(dir.path().join(format!("{LOG_FILE}.1"))).unwrap().len(), big.len());
    }

    #[test]
    fn only_macos_writes_a_log_file() {
        assert_eq!(log_dir().is_some(), cfg!(target_os = "macos"));
    }
}
