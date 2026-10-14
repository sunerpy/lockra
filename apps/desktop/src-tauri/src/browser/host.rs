//! Host mode (plan §四, "宿主模式"): the browser started this binary for the extension. Chrome and
//! Edge pass the extension's origin first (`chrome-extension://<id>/`, then on Windows a
//! `--parent-window=` handle); Firefox passes the manifest's path and the add-on's id. Nothing of
//! the app starts: no Tauri context, no single-instance check, no window. The host relays frames
//! between its stdio and the app's socket ([`lockra_browser::host::relay`]); stdout carries the
//! protocol alone, the log goes to stderr (`logging.rs`).

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use lockra_browser::host::{HostPorts, relay};
use lockra_browser::ids;

use super::{FROM_BROWSER, app_dirs, program, socket};

/// Whether `args` (the program's own first) are a browser starting its native-messaging host.
pub fn is_host_mode(args: &[OsString]) -> bool {
    let arg = |n: usize| args.get(n).and_then(|a| a.to_str());
    match (arg(1), arg(2)) {
        (Some(origin), _) if origin.starts_with("chrome-extension://") => true,
        (Some(manifest), Some(id)) => manifest.to_ascii_lowercase().ends_with(".json") && id == ids::FIREFOX,
        _ => false,
    }
}

/// What the host reaches on this machine: the app's socket, and the program to start.
struct Machine {
    socket: PathBuf,
}

impl HostPorts for Machine {
    #[cfg(unix)]
    type Stream = tokio::net::UnixStream;
    #[cfg(windows)]
    type Stream = tokio::net::windows::named_pipe::NamedPipeClient;

    async fn connect(&self) -> io::Result<Self::Stream> {
        socket::connect(&self.socket).await
    }

    fn start_app(&self, show: bool) -> io::Result<()> {
        start_lockra(show)
    }
}

/// Start Lockra by itself, detached from the host: with no stdio of the browser's (stdout is the
/// protocol's), in a process group of its own on Unix so the browser's ending the host does not
/// end it. Without `show` it starts as the browser's ([`FROM_BROWSER`]).
pub fn start_lockra(show: bool) -> io::Result<()> {
    let mut command = Command::new(program()?);
    if !show {
        command.arg(FROM_BROWSER);
    }
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command.spawn().map(drop)
}

/// Run the host until the browser closes its end: the process's exit status.
pub fn run_host() -> i32 {
    let Some((data, config)) = app_dirs() else {
        tracing::error!("host mode: no data folder");
        return 1;
    };
    let machine = Machine { socket: socket::place(&data, &config) };
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "host mode: no runtime");
            return 1;
        }
    };
    match runtime.block_on(relay(tokio::io::stdin(), tokio::io::stdout(), machine)) {
        Ok(()) => 0,
        Err(error) => {
            tracing::warn!(%error, "host mode: the browser sent what is not a frame");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn chromium_and_firefox_starts_are_host_mode_and_nothing_else_is() {
        assert!(is_host_mode(&args(&["lockra-desktop", "chrome-extension://egbpficliljjnpkjbigonldakfllebco/"])));
        assert!(is_host_mode(&args(&["lockra-desktop.exe", "chrome-extension://egbpficliljjnpkjbigonldakfllebco/", "--parent-window=0"])));
        assert!(is_host_mode(&args(&["lockra-desktop", "/home/me/.mozilla/native-messaging-hosts/dev.lockra.browser.json", "lockra@firlab.app"])));
        assert!(is_host_mode(&args(&["lockra-desktop.exe", r"C:\Users\me\AppData\Local\Lockra\browser\dev.lockra.browser.firefox.JSON", "lockra@firlab.app"])));
        // Another add-on's id, a plain start, the app's own flags: not the host.
        assert!(!is_host_mode(&args(&["lockra-desktop", "/x/dev.lockra.browser.json", "other@example.com"])));
        assert!(!is_host_mode(&args(&["lockra-desktop"])));
        assert!(!is_host_mode(&args(&["lockra-desktop", FROM_BROWSER])));
        assert!(!is_host_mode(&args(&["lockra-desktop", "--palette"])));
    }
}
