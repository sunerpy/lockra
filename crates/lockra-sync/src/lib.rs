//! End-to-end encrypted multi-device sync over storage the user configures (S3-compatible or
//! WebDAV), docs/formats.md "Sync" and docs/security.md "Sync".
//!
//! A sync space lives under one prefix of the user's storage: a keyring object that wraps the
//! space's data key under the master password *and* a random sync key, and one encrypted
//! snapshot per device. Every device writes only its own snapshot and folds in the others' with
//! a last-writer-wins merge over hybrid logical clock stamps, so no conditional write is needed
//! and S3 and WebDAV behave alike. The storage sees opaque objects whose names say nothing but
//! the number of devices.
//!
//! This crate does no I/O: the storage is the [`RemoteStore`] trait (lockra-remote implements it
//! over HTTP), the merged data is the caller's [`Replica`].

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod clock;
mod error;
mod frame;
mod keys;
mod lww;
mod object;
mod remote;
mod step;

pub use clock::{Clock, Hlc};
pub use error::SyncError;
pub use keys::{SYNC_KEY_TEXT_LEN, SpaceKeys, SyncKey, open_keyring, seal_keyring};
pub use lww::{Record, Tombstone, merge};
pub use object::{PAD_TO, Snapshot, open_snapshot, seal_snapshot};
pub use remote::{MemoryRemote, ObjectMeta, PutCondition, RemoteFuture, RemoteStore};
pub use step::{DeviceView, Outcome, Replica, Seen, Space, SyncState, device_path, devices_dir, keyring_path, remove_device, step};
