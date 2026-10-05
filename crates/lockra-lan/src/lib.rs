//! Sync over the local network (docs/formats.md, "LAN sync"; docs/security.md, "Sync"). One
//! computer of a space, the hub, keeps a copy of the space in a folder ([`FolderStore`]) and serves
//! it to the devices paired with it ([`HubServer`]); they reach it as a sync storage
//! ([`HubClient`]) and write their own snapshot only. Every connection opens with a cleartext
//! preamble giving its kind and a hint of the key it uses, then a Noise NNpsk0 handshake under a
//! 32-byte key of the device's own: nothing on the wire names the device or the space, and the
//! snapshots inside are sealed under the space's keys besides. The hub answers this computer and
//! the local network only.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod addr;
mod channel;
mod client;
mod discover;
mod folder;
mod proto;
mod server;
mod wire;

use std::time::Duration;

use zeroize::Zeroizing;

pub use addr::{local_address, local_addresses};
pub use client::{ClientConfig, HubClient, Joined, Joining, join};
pub use discover::DISCOVERY_WAIT;
pub use folder::FolderStore;
pub use lockra_sync::{PAIR_PREFIX, PairOffer, PairTextError};
pub use server::{HubConfig, HubEvent, HubPeer, HubServer, Pairing};

/// A device's key with a hub, or a pairing offer's.
pub type Key = Zeroizing<[u8; 32]>;

/// Devices a hub pairs with.
pub const MAX_PEERS: usize = 32;
/// Connections a hub serves at once.
pub const MAX_CONNECTIONS: usize = 8;
/// The time a handshake may take.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// The time a connection may stay without a request.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a pairing offer stands, and a pairing request waits for the user's answer.
pub const PAIRING_TIMEOUT: Duration = Duration::from_secs(120);
/// The largest message: an object at its largest, with its header.
pub const MAX_MESSAGE: usize = 17 * 1024 * 1024;

/// A new random key.
pub fn new_key() -> Result<Key, lockra_sync::SyncError> {
    let mut key = Zeroizing::new([0u8; 32]);
    getrandom::fill(key.as_mut()).map_err(|_| lockra_sync::SyncError::Random)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    #[test]
    fn keys_are_random() {
        let (a, b) = (super::new_key().unwrap(), super::new_key().unwrap());
        assert_ne!(*a, *b);
        assert_ne!(*a, [0u8; 32]);
    }
}
