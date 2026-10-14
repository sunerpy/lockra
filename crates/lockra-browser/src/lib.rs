//! The browser extension's channel (docs/formats.md §10, docs/security.md "Browser extension").
//!
//! The extension is a thin client: it keeps no vault and caches no secret. It reaches the running
//! app through the browser's native messaging, a host process (`lockra-desktop` in host mode) that
//! only passes frames between its stdio and the app's local socket; that pipe and the socket are
//! treated as untrusted, since Noise encrypts end to end between the extension and the app:
//!
//! - [`frame`]: the length-prefixed frames of both legs, at most 1 MiB.
//! - [`protocol`]: the envelopes, the plain messages before a handshake, the requests and answers.
//! - [`noise`]: pairing (`Noise_XX`, a six-digit code the user compares) and sessions
//!   (`Noise_KK` between the paired keys), fragmentation of the application messages.
//! - [`server`]: the app's service loop over a [`server::Listener`] the shell provides.
//!
//! Every decision on a request (the match for the frame, frames of other sites, the HTTP
//! downgrade, the protection level, the rate limits) is the core's: this crate calls the
//! `Core::browser_*` methods and the core calls nothing back.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod frame;
pub mod noise;
pub mod protocol;
pub mod server;

pub use server::{Listener, MemoryConnector, MemoryListener, memory, serve};

/// The native-messaging host's name, in every browser's manifest.
pub const HOST_NAME: &str = "dev.lockra.browser";
