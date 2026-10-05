//! End-to-end encrypted multi-device sync over storage the user configures (S3-compatible or
//! WebDAV), docs/formats.md "Sync" and docs/security.md "Sync".
//!
//! A sync space lives under one prefix of the user's storage: one encrypted snapshot per device,
//! each carrying a keyring that wraps the space's data key under that device's master password
//! *and* a random sync key. Every device writes only its own snapshot and folds in the others'
//! with a last-writer-wins merge over hybrid logical clock stamps: no object has two writers, so
//! no lock and no conditional write is needed, and S3 and WebDAV behave alike. The storage sees
//! opaque objects whose names say nothing but the number of devices.
//!
//! This crate does no I/O: the storage is the [`RemoteStore`] trait (lockra-remote implements it
//! over HTTP for a [`StorageConfig`]), the merged data is the caller's [`Replica`].

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod clock;
mod error;
mod frame;
mod invite;
mod keys;
mod lww;
mod object;
mod pair;
mod remote;
mod space;
mod step;
mod storage;

pub use clock::{Clock, Hlc};
pub use error::SyncError;
pub use invite::Invite;
pub use keys::{SYNC_KEY_TEXT_LEN, SpaceKeys, SyncKey, open_keyring, seal_keyring};
pub use lww::{Record, Tombstone, merge};
pub use object::{PAD_TO, Snapshot, open_snapshot, seal_snapshot, snapshot_keyring};
pub use pair::{LanKey, LanWelcome, PAIR_PREFIX, PairOffer, PairTextError};
pub use remote::{MAX_OBJECT_BYTES, MemoryRemote, ObjectMeta, PutCondition, RemoteFuture, RemoteStore};
pub use space::{MAX_SNAPSHOTS_TRIED, find_space, open_space};
pub use step::{DeviceView, Outcome, PendingWrite, Persist, Replica, Seen, Space, SyncState, device_path, devices_dir, remove_device, step, step_with};
pub use storage::{ConfigError, StorageConfig};
