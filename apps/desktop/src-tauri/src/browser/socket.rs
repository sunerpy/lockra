//! Where the app listens for the host (plan §四, "本地 socket"), and how the host reaches it:
//!
//! - Linux: `$XDG_RUNTIME_DIR/lockra/browser.sock`, else `<data>/browser/browser.sock`.
//! - macOS: `<data>/browser/browser.sock` while that is at most 104 bytes (`sun_path`), else
//!   `$TMPDIR/lockra/browser.sock`.
//! - The folder is the user's alone (0700, theirs): the app makes it so, the host checks it, and
//!   the app takes a connection only from a process of the same user (`peer_cred`).
//! - Windows: `\\.\pipe\lockra-browser-<sha256 of the config folder>`, created as the first
//!   instance and refusing remote clients. Its default DACL lets everyone in; Noise is why that
//!   does not matter (docs/security.md): a squatter can only make the connection fail.

use std::io;
use std::path::{Path, PathBuf};

use data_encoding::HEXLOWER;
use sha2::{Digest, Sha256};

/// The longest path a Unix socket address holds on macOS (`sun_path`, 104 bytes).
pub const MACOS_SUN_PATH: usize = 104;

/// The socket's file name.
pub const SOCKET_FILE: &str = "browser.sock";

/// Which system the paths are for (the tests check every one on any machine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Linux,
    MacOs,
}

/// The Unix socket's path on `system`.
pub fn unix_socket_path(system: System, runtime_dir: Option<&Path>, data_dir: &Path, temp_dir: &Path) -> PathBuf {
    match system {
        System::Linux => match runtime_dir.filter(|d| d.is_absolute()) {
            Some(runtime) => runtime.join("lockra").join(SOCKET_FILE),
            None => data_dir.join("browser").join(SOCKET_FILE),
        },
        System::MacOs => {
            let in_data = data_dir.join("browser").join(SOCKET_FILE);
            if in_data.as_os_str().len() <= MACOS_SUN_PATH { in_data } else { temp_dir.join("lockra").join(SOCKET_FILE) }
        }
    }
}

/// The named pipe's name on Windows, for the app whose config folder is `config_dir`.
pub fn pipe_name(config_dir: &Path) -> String {
    let digest = Sha256::digest(config_dir.to_string_lossy().as_bytes());
    format!(r"\\.\pipe\lockra-browser-{}", HEXLOWER.encode(&digest))
}

/// Where this machine's app listens, given its data and config folders.
#[cfg(unix)]
pub fn place(data_dir: &Path, _config_dir: &Path) -> PathBuf {
    let system = if cfg!(target_os = "macos") { System::MacOs } else { System::Linux };
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    unix_socket_path(system, runtime.as_deref(), data_dir, &std::env::temp_dir())
}

/// Where this machine's app listens, given its data and config folders.
#[cfg(windows)]
pub fn place(_data_dir: &Path, config_dir: &Path) -> PathBuf {
    PathBuf::from(pipe_name(config_dir))
}

#[cfg(unix)]
pub use unix::{Socket, connect};
#[cfg(windows)]
pub use windows::{Socket, connect};

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::{DirBuilderExt as _, FileTypeExt as _, MetadataExt as _, PermissionsExt as _};

    use lockra_browser::Listener;
    use tokio::net::{UnixListener, UnixStream};

    use super::*;

    /// This process's user.
    fn own_uid() -> u32 {
        rustix::process::getuid().as_raw()
    }

    /// The socket's folder, made the user's alone: created 0700, or, when it is there, checked to
    /// be a folder of theirs (not a link) and set to 0700.
    pub fn prepare_dir(dir: &Path) -> io::Result<()> {
        match std::fs::symlink_metadata(dir) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?,
            Err(error) => return Err(error),
            Ok(meta) => {
                if !meta.file_type().is_dir() || meta.uid() != own_uid() {
                    return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the socket's folder is not this user's"));
                }
            }
        }
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
    }

    /// The folder is this user's alone: theirs, a folder, no one else may enter it.
    pub fn trusted_dir(dir: &Path) -> io::Result<()> {
        let meta = std::fs::symlink_metadata(dir)?;
        if meta.file_type().is_dir() && meta.uid() == own_uid() && meta.mode() & 0o077 == 0 {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "the socket's folder is not this user's alone"))
        }
    }

    /// The app's listening socket.
    #[derive(Debug)]
    pub struct Socket {
        listener: UnixListener,
        path: PathBuf,
        uid: u32,
    }

    impl Socket {
        /// Listen at `path`, its folder made the user's alone. A socket left there by a Lockra
        /// that did not end goes first (the single-instance check means no other one runs);
        /// anything else there is left alone, and nothing listens.
        pub fn bind(path: &Path) -> io::Result<Self> {
            let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the socket has no folder"))?;
            prepare_dir(dir)?;
            match std::fs::symlink_metadata(path) {
                Ok(meta) if meta.file_type().is_socket() => std::fs::remove_file(path)?,
                Ok(_) => return Err(io::Error::new(io::ErrorKind::AlreadyExists, "something else is where the socket goes")),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            let listener = UnixListener::bind(path)?;
            Ok(Self { listener, path: path.to_owned(), uid: own_uid() })
        }

        /// Where it listens.
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Listener for Socket {
        type Stream = UnixStream;

        async fn accept(&mut self) -> io::Result<UnixStream> {
            loop {
                let (stream, _) = self.listener.accept().await?;
                match stream.peer_cred() {
                    Ok(peer) if peer.uid() == self.uid => return Ok(stream),
                    Ok(peer) => tracing::warn!(uid = peer.uid(), "the browser channel refused another user's process"),
                    Err(error) => tracing::warn!(%error, "the browser channel could not tell who connected"),
                }
            }
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            // Only the socket this listener made: a newer one in its place stays.
            if std::fs::symlink_metadata(&self.path).is_ok_and(|meta| meta.file_type().is_socket()) {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }

    /// The host's connection to the app at `path`, once its folder checks out.
    pub async fn connect(path: &Path) -> io::Result<UnixStream> {
        let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the socket has no folder"))?;
        trusted_dir(dir)?;
        UnixStream::connect(path).await
    }
}

#[cfg(windows)]
mod windows {
    use std::time::Duration;

    use lockra_browser::Listener;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions};

    use super::*;

    /// `ERROR_PIPE_BUSY`: every instance is taken for the moment.
    const PIPE_BUSY: i32 = 231;

    /// The app's named pipe: the instance waiting for the next host.
    #[derive(Debug)]
    pub struct Socket {
        name: String,
        next: NamedPipeServer,
    }

    impl Socket {
        /// Create the pipe `name` as its first instance: if another process holds the name, this
        /// fails rather than share it.
        pub fn bind(name: &Path) -> io::Result<Self> {
            let name = name.to_string_lossy().into_owned();
            let next = ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create(&name)?;
            Ok(Self { name, next })
        }

        /// Its name.
        pub fn path(&self) -> &Path {
            Path::new(&self.name)
        }
    }

    impl Listener for Socket {
        type Stream = NamedPipeServer;

        async fn accept(&mut self) -> io::Result<NamedPipeServer> {
            self.next.connect().await?;
            // The next instance first, so that a host is never told there is no pipe.
            let next = ServerOptions::new().reject_remote_clients(true).create(&self.name)?;
            Ok(std::mem::replace(&mut self.next, next))
        }
    }

    /// The host's connection to the app's pipe; a busy pipe is tried again for a moment.
    pub async fn connect(name: &Path) -> io::Result<NamedPipeClient> {
        let name = name.to_string_lossy().into_owned();
        for _ in 0..20 {
            match ClientOptions::new().open(&name) {
                Err(error) if error.raw_os_error() == Some(PIPE_BUSY) => tokio::time::sleep(Duration::from_millis(50)).await,
                other => return other,
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "the app's pipe stayed busy"))
    }
}

#[cfg(test)]
mod tests;
