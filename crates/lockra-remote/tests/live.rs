//! The storage against real servers: the Versity S3 gateway and rclone's WebDAV server, as
//! `make sync-it` (scripts/sync-it.sh) and CI start them. Each test runs only when its server is
//! named in the environment; with `LOCKRA_IT_REQUIRED=1` a missing one fails instead of skipping.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lockra_remote::{Storage, StorageConfig};
use lockra_sync::{
    Hlc, MAX_OBJECT_BYTES, MemoryRemote, PutCondition, Record, RemoteStore, Replica, Space, SpaceKeys, SyncError, SyncKey, SyncState, Tombstone, merge,
    open_keyring, open_space, seal_keyring, step,
};
use lockra_vault::KdfCost;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// The server's configuration, under a fresh prefix so runs never meet; `None` to skip.
fn configured(server: &str, config: impl FnOnce(String) -> Option<StorageConfig>) -> Option<StorageConfig> {
    let prefix = format!("it-{}/", Uuid::new_v4());
    let found = config(prefix);
    if found.is_none() {
        assert!(env("LOCKRA_IT_REQUIRED").is_none(), "LOCKRA_IT_REQUIRED is set but the {server} server is not configured");
        eprintln!("live: no {server} server configured, skipped (make sync-it)");
    }
    found
}

fn s3() -> Option<StorageConfig> {
    configured("S3", |prefix| {
        Some(StorageConfig::S3 {
            endpoint: env("LOCKRA_IT_S3_ENDPOINT")?,
            region: env("LOCKRA_IT_S3_REGION").unwrap_or_else(|| "us-east-1".into()),
            bucket: env("LOCKRA_IT_S3_BUCKET")?,
            prefix,
            access_key_id: env("LOCKRA_IT_S3_ACCESS_KEY")?,
            secret_access_key: Zeroizing::new(env("LOCKRA_IT_S3_SECRET_KEY")?),
            path_style: true,
        })
    })
}

fn webdav() -> Option<StorageConfig> {
    configured("WebDAV", |prefix| {
        Some(StorageConfig::Webdav {
            url: env("LOCKRA_IT_WEBDAV_URL")?,
            prefix,
            username: env("LOCKRA_IT_WEBDAV_USER")?,
            password: Zeroizing::new(env("LOCKRA_IT_WEBDAV_PASSWORD")?),
        })
    })
}

/// What lockra-sync relies on, request by request.
async fn the_store_contract(storage: &dyn RemoteStore, prefix: &str) {
    let dir = format!("{prefix}contract/");
    let path = format!("{dir}a.lks");
    assert!(storage.list(&dir).await.unwrap().is_empty(), "an absent directory lists empty");
    assert_eq!(storage.get(&path).await.unwrap(), None);
    let first = storage.put(&path, b"one".to_vec(), PutCondition::IfAbsent).await.unwrap();
    assert_eq!(storage.get(&path).await.unwrap().map(|(bytes, _)| bytes), Some(b"one".to_vec()));
    let listing = storage.list(&dir).await.unwrap();
    assert_eq!(listing.iter().map(|m| (m.name.as_str(), m.size)).collect::<Vec<_>>(), [("a.lks", 3)]);
    if storage.conditional_puts() {
        assert_eq!(storage.put(&path, b"two".to_vec(), PutCondition::IfAbsent).await.err(), Some(SyncError::Conflict), "an existing object is not replaced");
        assert_eq!(storage.put(&path, b"two".to_vec(), PutCondition::IfMatch("\"not-the-etag\"".into())).await.err(), Some(SyncError::Conflict));
        let etag = first.or_else(|| listing[0].etag.clone()).expect("S3 gives etags");
        storage.put(&path, b"two".to_vec(), PutCondition::IfMatch(etag)).await.unwrap();
        assert_eq!(storage.get(&path).await.unwrap().map(|(bytes, _)| bytes), Some(b"two".to_vec()));
    } else {
        storage.put(&path, b"two".to_vec(), PutCondition::IfAbsent).await.unwrap();
        assert_eq!(storage.get(&path).await.unwrap().map(|(bytes, _)| bytes), Some(b"two".to_vec()), "WebDAV writes whatever the condition");
    }
    // A listing does not descend into folders.
    storage.put(&format!("{dir}sub/b.lks"), b"x".to_vec(), PutCondition::Always).await.unwrap();
    assert_eq!(storage.list(&dir).await.unwrap().len(), 1);
    storage.delete(&path).await.unwrap();
    storage.delete(&path).await.unwrap();
    assert_eq!(storage.get(&path).await.unwrap(), None);

    // An empty object reads as empty; one larger than any snapshot is refused, not read whole.
    let limits = format!("{prefix}limits/");
    let empty = format!("{limits}empty.lks");
    storage.put(&empty, Vec::new(), PutCondition::Always).await.unwrap();
    assert_eq!(storage.get(&empty).await.unwrap().map(|(bytes, _)| bytes), Some(Vec::new()));
    let huge = format!("{limits}huge.lks");
    storage.put(&huge, vec![7; usize::try_from(MAX_OBJECT_BYTES).unwrap() + 1], PutCondition::Always).await.unwrap();
    assert!(storage.list(&limits).await.unwrap().iter().any(|m| m.name == "huge.lks" && m.size > MAX_OBJECT_BYTES));
    assert_eq!(storage.get(&huge).await.err(), Some(SyncError::Corrupted));
    storage.delete(&empty).await.unwrap();
    storage.delete(&huge).await.unwrap();
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Account {
    id: Uuid,
    stamp: Hlc,
    issuer: String,
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
struct Vault {
    accounts: Vec<Account>,
    tombstones: Vec<Tombstone>,
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

fn account(issuer: &str, wall_ms: u64, device: u64) -> Account {
    Account { id: Uuid::new_v4(), stamp: Hlc { wall_ms, counter: 0, device }, issuer: issuer.into() }
}

fn issuers(vault: &Vault) -> Vec<String> {
    let mut issuers: Vec<String> = vault.accounts.iter().map(|a| a.issuer.clone()).collect();
    issuers.sort();
    issuers
}

/// The space's keys from its storage, the sync key and a device's master password.
async fn join(storage: &dyn RemoteStore, prefix: &str, sync_key: &SyncKey, password: &'static [u8]) -> Result<SpaceKeys, SyncError> {
    let space_id = sync_key.space_id();
    open_space(storage, prefix, space_id, |keyring| {
        let sync_key = sync_key.clone();
        async move { open_keyring(&keyring, space_id, &sync_key, password) }
    })
    .await
}

/// A space created on one device and joined on another, through the real server; then both
/// change something at the same moment.
async fn two_devices_sync(storage: &dyn RemoteStore, prefix: &str) {
    let sync_key = SyncKey::generate().unwrap();
    let keys = SpaceKeys::generate(sync_key.space_id()).unwrap();
    let laptop_keyring = seal_keyring(&keys, &sync_key, b"laptop password", KdfCost::FAST_INSECURE).unwrap();
    assert_eq!(join(storage, prefix, &sync_key, b"laptop password").await.err(), Some(SyncError::NoSpace));

    let mut laptop = (SyncState::default(), Vault::default());
    laptop.1.accounts.push(account("GitHub", 10, 1));
    let space = Space { prefix, keys: &keys, device: 1, device_name: "Laptop", keyring: &laptop_keyring, seq_floor: 0 };
    assert!(step(storage, &space, &mut laptop.0, &mut laptop.1, 100).await.unwrap().wrote);

    // The phone has the storage, the sync key and the laptop's master password.
    let joined = join(storage, prefix, &sync_key, b"laptop password").await.unwrap();
    assert_eq!(join(storage, prefix, &sync_key, b"a guess").await.err(), Some(SyncError::WrongCredentials));
    let phone_keyring = seal_keyring(&joined, &sync_key, b"phone password", KdfCost::FAST_INSECURE).unwrap();
    let mut phone = (SyncState::default(), Vault::default());
    let phone_space = Space { prefix, keys: &joined, device: 2, device_name: "Phone", keyring: &phone_keyring, seq_floor: 0 };
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

#[tokio::test]
async fn s3_holds_a_sync_space() {
    let Some(config) = s3() else { return };
    let storage = Storage::open(&config).unwrap();
    assert!(storage.conditional_puts());
    the_store_contract(&storage, config.prefix()).await;
    two_devices_sync(&storage, config.prefix()).await;
}

#[tokio::test]
async fn webdav_holds_a_sync_space() {
    let Some(config) = webdav() else { return };
    let storage = Storage::open(&config).unwrap();
    assert!(!storage.conditional_puts());
    the_store_contract(&storage, config.prefix()).await;
    two_devices_sync(&storage, config.prefix()).await;
}

#[tokio::test]
async fn wrong_credentials_are_denied() {
    let Some(StorageConfig::S3 { endpoint, region, bucket, prefix, access_key_id, path_style, .. }) = s3() else { return };
    let wrong = StorageConfig::S3 {
        endpoint,
        region,
        bucket,
        prefix: prefix.clone(),
        access_key_id,
        secret_access_key: Zeroizing::new("wrong secret".into()),
        path_style,
    };
    let storage = Storage::open(&wrong).unwrap();
    assert_eq!(storage.list(&prefix).await.err(), Some(SyncError::Denied));
}

/// WebDAV refuses with 401, which OpenDAL leaves unclassified.
#[tokio::test]
async fn a_wrong_webdav_password_is_denied() {
    let Some(StorageConfig::Webdav { url, prefix, username, .. }) = webdav() else { return };
    let wrong = StorageConfig::Webdav { url, prefix: prefix.clone(), username, password: Zeroizing::new("wrong password".into()) };
    let storage = Storage::open(&wrong).unwrap();
    assert_eq!(storage.list(&prefix).await.err(), Some(SyncError::Denied));
}

/// The same contract on the in-memory stand-in, so the test code itself is exercised everywhere.
#[tokio::test]
async fn the_contract_holds_for_the_memory_store_too() {
    for conditional in [true, false] {
        let store = MemoryRemote::new(conditional);
        the_store_contract(&store, "mem/").await;
        two_devices_sync(&store, "mem/").await;
    }
}
