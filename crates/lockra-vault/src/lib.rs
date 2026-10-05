//! The encrypted container shared by the vault and its backups (docs/formats.md).
//!
//! A file is `magic | header length | JSON header | payload ciphertext`. The payload is
//! XChaCha20-Poly1305 under a random data key, authenticated together with everything before it;
//! the data key is wrapped once per slot: under Argon2id of the master password, and optionally
//! under a random device key that the OS keychain holds ("remember on this device").

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod atomic;
mod b64;
mod container;
mod kdf;

pub use atomic::write_atomic;
pub use container::{
    BACKUP_MAGIC, DeviceCheck, DeviceKey, DeviceSlot, FileKind, HeaderInfo, MAX_HEADER_LEN, Opened, Sealed, VAULT_MAGIC, VaultError, read_header,
};
pub use kdf::{KdfCost, KdfParams};
