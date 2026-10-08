//! lockra-relay: a server that keeps Lockra's sync spaces for devices that have no storage of their
//! own (docs/relay.md; the API in docs/formats.md "Relay"; what it can and cannot learn in
//! docs/security.md "Sync").
//!
//! It is storage, not a participant. It holds the same end-to-end encrypted snapshots an S3 bucket
//! or a WebDAV folder would, one per device, each written only by its device, and opens none of
//! them: the data key that seals them never leaves the devices, and neither does the sync key. A
//! space's devices show the relay an access token derived from their sync key, which it keeps
//! only as a hash; the space is bound to it by its first write. What the relay sees is what any
//! storage sees: the space's id, its devices' opaque tags, snapshot sizes in 4 KiB steps, when
//! they change, and the addresses that ask.
//!
//! One process, the snapshots in a directory, a limit on every resource a client can spend, and no
//! log of who asked for what. Serve it behind HTTPS (a load balancer or a reverse proxy): Lockra
//! talks to relays over HTTPS only.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod api;
mod config;
mod limit;
mod server;
mod store;

pub use api::VERSION;
pub use config::{Command, Config, HELP, IpNet, Limits};
pub use server::{Relay, run};

/// Log to standard error, at `RUST_LOG`'s levels (`info` by default); in colour on a terminal only
/// (the journal and `docker logs` take the plain text).
pub fn init_logging() {
    use std::io::IsTerminal as _;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ =
        tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).with_ansi(std::io::stderr().is_terminal()).with_target(false).try_init();
}

/// Completes when the process is asked to stop (Ctrl-C, or SIGTERM where there is one).
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut signal) => {
                    signal.recv().await;
                }
                Err(_) => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            () = terminate => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
