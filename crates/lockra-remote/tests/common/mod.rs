//! What the storage tests share: a replica of accounts, and a space two devices sync through.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use lockra_sync::{
    Hlc, Record, RemoteStore, Replica, Space, SpaceKeys, SyncError, SyncKey, SyncState, Tombstone, merge, open_keyring, open_space, seal_keyring, step,
};
use lockra_vault::KdfCost;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub stamp: Hlc,
    pub issuer: String,
}

impl Record for Account {
    fn id(&self) -> Uuid {
        self.id
    }

    fn stamp(&self) -> Hlc {
        self.stamp
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vault {
    pub accounts: Vec<Account>,
    pub tombstones: Vec<Tombstone>,
}

impl Replica for Vault {
    fn payload(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(serde_json::to_vec(self).unwrap())
    }

    fn absorb(&mut self, payload: &[u8]) -> Result<bool, SyncError> {
        let theirs: Self = serde_json::from_slice(payload).map_err(|_| SyncError::Corrupted)?;
        Ok(merge(&mut self.accounts, &mut self.tombstones, &theirs.accounts, &theirs.tombstones))
    }
}

pub fn account(issuer: &str, wall_ms: u64, device: u64) -> Account {
    Account { id: Uuid::new_v4(), stamp: Hlc { wall_ms, counter: 0, device }, issuer: issuer.into() }
}

pub fn issuers(vault: &Vault) -> Vec<String> {
    let mut issuers: Vec<String> = vault.accounts.iter().map(|a| a.issuer.clone()).collect();
    issuers.sort();
    issuers
}

/// The space's keys from its storage, the sync key and a device's master password.
pub async fn join(storage: &dyn RemoteStore, prefix: &str, sync_key: &SyncKey, password: &'static [u8]) -> Result<SpaceKeys, SyncError> {
    let space_id = sync_key.space_id();
    open_space(storage, prefix, space_id, |keyring| {
        let sync_key = sync_key.clone();
        async move { open_keyring(&keyring, space_id, &sync_key, password) }
    })
    .await
}

/// A space created on one device and joined on another, through the real server; then both
/// change something at the same moment.
pub async fn two_devices_sync(storage: &dyn RemoteStore, prefix: &str) {
    two_devices_sync_with(storage, prefix, &SyncKey::generate().unwrap()).await;
}

/// [`two_devices_sync`] in the space `sync_key` names (a relay's store is opened for it).
pub async fn two_devices_sync_with(storage: &dyn RemoteStore, prefix: &str, sync_key: &SyncKey) {
    let sync_key = sync_key.clone();
    let keys = SpaceKeys::generate(sync_key.space_id()).unwrap();
    let laptop_keyring = seal_keyring(&keys, &sync_key, b"laptop password", KdfCost::FAST_INSECURE).unwrap();
    assert_eq!(join(storage, prefix, &sync_key, b"laptop password").await.err(), Some(SyncError::NoSpace));

    let mut laptop = (SyncState::default(), Vault::default());
    laptop.1.accounts.push(account("GitHub", 10, 1));
    let space = Space { prefix, keys: &keys, device: 1, device_name: "Laptop", keyring: &laptop_keyring };
    assert!(step(storage, &space, &mut laptop.0, &mut laptop.1, 100).await.unwrap().wrote);

    // The phone has the storage, the sync key and the laptop's master password.
    let joined = join(storage, prefix, &sync_key, b"laptop password").await.unwrap();
    assert_eq!(join(storage, prefix, &sync_key, b"a guess").await.err(), Some(SyncError::WrongCredentials));
    let phone_keyring = seal_keyring(&joined, &sync_key, b"phone password", KdfCost::FAST_INSECURE).unwrap();
    let mut phone = (SyncState::default(), Vault::default());
    let phone_space = Space { prefix, keys: &joined, device: 2, device_name: "Phone", keyring: &phone_keyring };
    let outcome = step(storage, &phone_space, &mut phone.0, &mut phone.1, 200).await.unwrap();
    assert!(outcome.changed);
    assert_eq!(phone.1.accounts[0].issuer, "GitHub");
    assert_eq!(outcome.devices.len(), 2);

    let id = phone.1.accounts[0].id;
    phone.1.accounts.clear();
    phone.1.tombstones.push(Tombstone { id, stamp: Hlc { wall_ms: 300, counter: 0, device: 2 }, counter: None });
    step(storage, &phone_space, &mut phone.0, &mut phone.1, 300).await.unwrap();
    assert!(step(storage, &space, &mut laptop.0, &mut laptop.1, 400).await.unwrap().changed);
    assert!(laptop.1.accounts.is_empty(), "the deletion arrived");
    // Quiet runs read and write nothing more.
    assert!(!step(storage, &space, &mut laptop.0, &mut laptop.1, 500).await.unwrap().wrote);
    // The phone's own master password opens the space as well.
    assert_eq!(*join(storage, prefix, &sync_key, b"phone password").await.unwrap().data_key_text(), *keys.data_key_text());

    // Both change something at the same moment: each writes only its own object, so neither
    // change is lost, conditions or not.
    laptop.1.accounts.push(account("Mail", 600, 1));
    phone.1.accounts.push(account("Bank", 600, 2));
    let (laptop_run, phone_run) =
        tokio::join!(step(storage, &space, &mut laptop.0, &mut laptop.1, 600), step(storage, &phone_space, &mut phone.0, &mut phone.1, 600));
    assert!(laptop_run.unwrap().wrote && phone_run.unwrap().wrote);
    step(storage, &space, &mut laptop.0, &mut laptop.1, 700).await.unwrap();
    step(storage, &phone_space, &mut phone.0, &mut phone.1, 700).await.unwrap();
    assert_eq!(issuers(&laptop.1), ["Bank", "Mail"]);
    assert_eq!(issuers(&phone.1), ["Bank", "Mail"]);
}
