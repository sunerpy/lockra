use super::*;

#[test]
fn linux_puts_the_socket_in_the_runtime_folder_else_the_data_folder() {
    let data = Path::new("/home/me/.local/share/dev.lockra.desktop");
    let temp = Path::new("/tmp");
    assert_eq!(unix_socket_path(System::Linux, Some(Path::new("/run/user/1000")), data, temp), PathBuf::from("/run/user/1000/lockra/browser.sock"));
    assert_eq!(unix_socket_path(System::Linux, None, data, temp), data.join("browser/browser.sock"));
    // A relative runtime folder is no runtime folder.
    assert_eq!(unix_socket_path(System::Linux, Some(Path::new("run")), data, temp), data.join("browser/browser.sock"));
}

#[test]
fn macos_keeps_the_socket_in_the_data_folder_while_its_path_fits() {
    let temp = Path::new("/var/folders/xy/T");
    let data = Path::new("/Users/me/Library/Application Support/dev.lockra.desktop");
    assert_eq!(unix_socket_path(System::MacOs, None, data, temp), data.join("browser/browser.sock"));
    let long = PathBuf::from(format!("/Users/{}/Library/Application Support/dev.lockra.desktop", "m".repeat(60)));
    assert_eq!(unix_socket_path(System::MacOs, None, &long, temp), temp.join("lockra/browser.sock"));
}

#[test]
fn the_pipe_is_named_after_the_config_folder() {
    let name = pipe_name(Path::new(r"C:\Users\me\AppData\Roaming\dev.lockra.desktop"));
    assert!(name.starts_with(r"\\.\pipe\lockra-browser-"));
    assert_eq!(name.len(), r"\\.\pipe\lockra-browser-".len() + 64);
    assert_ne!(name, pipe_name(Path::new(r"C:\Users\you\AppData\Roaming\dev.lockra.desktop")));
}

#[cfg(unix)]
mod unix_socket {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    use lockra_browser::Listener as _;
    use lockra_browser::frame::{Order, read_frame, write_frame};

    use super::super::unix::{prepare_dir, trusted_dir};
    use super::*;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().mode() & 0o777
    }

    #[test]
    fn the_folder_is_made_the_users_alone() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("lockra");
        prepare_dir(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        // One opened up is closed again.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(trusted_dir(&dir).is_err());
        prepare_dir(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        trusted_dir(&dir).unwrap();
        // A link or a file in its place is refused.
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        assert!(prepare_dir(&link).is_err());
        assert!(trusted_dir(&link).is_err());
        let file = root.path().join("file");
        std::fs::write(&file, b"").unwrap();
        assert!(prepare_dir(&file).is_err());
    }

    #[tokio::test]
    async fn the_app_listens_and_the_host_reaches_it() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("lockra").join(SOCKET_FILE);
        let mut socket = Socket::bind(&path).unwrap();
        assert_eq!(socket.path(), path);
        let host = tokio::spawn({
            let path = path.clone();
            async move {
                let mut stream = connect(&path).await.unwrap();
                write_frame(&mut stream, Order::Little, b"{}").await.unwrap();
            }
        });
        let mut stream = socket.accept().await.unwrap();
        assert_eq!(read_frame(&mut stream, Order::Little).await.unwrap().unwrap(), b"{}");
        host.await.unwrap();
        // Gone with the listener.
        drop(socket);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn a_socket_left_behind_is_replaced_and_anything_else_is_left() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("lockra").join(SOCKET_FILE);
        // A Lockra that did not end left its socket.
        let left = Socket::bind(&path).unwrap();
        std::mem::forget(left);
        let socket = Socket::bind(&path).unwrap();
        drop(socket);
        // A file is not a socket: nothing listens, and the file stays.
        std::fs::write(&path, b"mine").unwrap();
        assert_eq!(Socket::bind(&path).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).unwrap(), b"mine");
    }

    #[tokio::test]
    async fn the_host_trusts_no_folder_others_may_enter() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("lockra").join(SOCKET_FILE);
        let _socket = Socket::bind(&path).unwrap();
        std::fs::set_permissions(path.parent().unwrap(), std::fs::Permissions::from_mode(0o770)).unwrap();
        assert_eq!(connect(&path).await.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        // Nothing listening: the host hears it as Lockra not running.
        let none = root.path().join("none").join(SOCKET_FILE);
        std::fs::create_dir(none.parent().unwrap()).unwrap();
        std::fs::set_permissions(none.parent().unwrap(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(connect(&none).await.is_err());
    }
}

#[cfg(windows)]
mod pipe {
    use lockra_browser::Listener as _;
    use lockra_browser::frame::{Order, read_frame, write_frame};

    use super::*;

    #[tokio::test]
    async fn the_pipe_is_the_first_instance_alone_and_the_host_reaches_it() {
        let name = PathBuf::from(pipe_name(&std::env::temp_dir().join(uuid::Uuid::new_v4().to_string())));
        let mut socket = Socket::bind(&name).unwrap();
        // A second "first instance" under the same name is refused.
        assert!(Socket::bind(&name).is_err());
        let host = tokio::spawn({
            let name = name.clone();
            async move {
                let mut stream = connect(&name).await.unwrap();
                write_frame(&mut stream, Order::Little, b"{}").await.unwrap();
            }
        });
        let mut stream = socket.accept().await.unwrap();
        assert_eq!(read_frame(&mut stream, Order::Little).await.unwrap().unwrap(), b"{}");
        host.await.unwrap();
    }
}
