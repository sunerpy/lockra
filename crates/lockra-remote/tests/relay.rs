//! A sync space on a real relay (lockra-relay, started in the test on this computer): what the
//! sync engine relies on, two devices syncing through it, the access token, the watch. With
//! `LOCKRA_IT_RELAY_URL` (the built-in relay, or one of one's own), the last test does the same
//! through that relay, over the network, and removes what it wrote.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::two_devices_sync_with;
use lockra_relay::{Config, Limits, Relay};
use lockra_remote::{SpaceAccess, StorageConfig, Watch, open, watch};
use lockra_sync::{PutCondition, SyncError, SyncKey, device_path, devices_dir};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

const TAG: &str = "0123456789abcdef0123456789abcdef";
const OTHER: &str = "fedcba9876543210fedcba9876543210";

async fn relay(dir: &std::path::Path, limits: Limits) -> Relay {
    Relay::start(Config { bind: "127.0.0.1:0".parse().unwrap(), data_dir: dir.to_owned(), trusted_proxies: Vec::new(), limits }).await.unwrap()
}

fn config(relay: &Relay) -> StorageConfig {
    StorageConfig::Relay { url: format!("http://{}", relay.addr()) }
}

#[tokio::test]
async fn a_relay_keeps_what_the_engine_relies_on() {
    let dir = tempfile::tempdir().unwrap();
    let relay = relay(dir.path(), Limits { max_object_bytes: 1024, ..Limits::default() }).await;
    let key = SyncKey::generate().unwrap();
    let store = open(&config(&relay), &SpaceAccess::of(&key)).unwrap();
    assert!(store.conditional_puts(), "a relay holds a write's condition");
    let space = key.space_id();
    let (devices, path) = (devices_dir("", space), device_path("", space, TAG));
    assert!(store.list(&devices).await.unwrap().is_empty(), "a space the relay does not keep lists empty");
    assert_eq!(store.get(&path).await.unwrap(), None);
    store.delete(&path).await.unwrap();

    let first = store.put(&path, b"one".to_vec(), PutCondition::IfAbsent).await.unwrap().expect("a relay gives etags");
    let (bytes, etag) = store.get(&path).await.unwrap().unwrap();
    assert_eq!((bytes.as_slice(), etag.as_deref()), (b"one".as_slice(), Some(first.as_str())));
    let listing = store.list(&devices).await.unwrap();
    assert_eq!(
        listing.iter().map(|m| (m.name.as_str(), m.size, m.etag.as_deref())).collect::<Vec<_>>(),
        [(format!("{TAG}.lks").as_str(), 3, Some(first.as_str()))]
    );
    assert_eq!(store.put(&path, b"two".to_vec(), PutCondition::IfAbsent).await.err(), Some(SyncError::Conflict));
    assert_eq!(store.put(&path, b"two".to_vec(), PutCondition::IfMatch("\"not-the-etag\"".into())).await.err(), Some(SyncError::Conflict));
    store.put(&path, b"two".to_vec(), PutCondition::IfMatch(first)).await.unwrap();
    assert_eq!(store.get(&path).await.unwrap().map(|(bytes, _)| bytes), Some(b"two".to_vec()));
    // An empty snapshot is kept as empty.
    let empty = device_path("", space, OTHER);
    store.put(&empty, Vec::new(), PutCondition::Always).await.unwrap();
    assert_eq!(store.get(&empty).await.unwrap().map(|(bytes, _)| bytes), Some(Vec::new()));
    store.delete(&path).await.unwrap();
    store.delete(&path).await.unwrap();
    assert_eq!(store.get(&path).await.unwrap(), None);

    // Past the relay's own limit, and outside a space: refused as storage failures, not retried.
    assert!(matches!(store.put(&path, vec![0; 1025], PutCondition::Always).await, Err(SyncError::Storage(_))));
    for bad in ["contract/a.lks".to_owned(), format!("lockra-sync-v1/{space}/devices/sub/x.lks")] {
        assert!(matches!(store.put(&bad, b"x".to_vec(), PutCondition::Always).await, Err(SyncError::Storage(_))), "{bad}");
    }
    relay.stop().await;
}

#[tokio::test]
async fn two_devices_sync_through_a_relay() {
    let dir = tempfile::tempdir().unwrap();
    let relay = relay(dir.path(), Limits::default()).await;
    let key = SyncKey::generate().unwrap();
    let store = open(&config(&relay), &SpaceAccess::of(&key)).unwrap();
    two_devices_sync_with(&*store, "", &key).await;
    // On the relay's disk, the snapshots as the devices sealed them: no account in clear.
    let space = dir.path().join("lockra-relay-v1/spaces").join(key.space_id().to_string()).join("devices");
    let files: Vec<_> = std::fs::read_dir(space).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(files.len(), 2, "one snapshot per device");
    for file in files {
        let bytes = std::fs::read(file).unwrap();
        assert!(bytes.starts_with(b"LKSDEVS1"));
        assert!(!bytes.windows(4).any(|w| w == b"Bank" || w == b"Mail"));
    }
    relay.stop().await;
}

#[tokio::test]
async fn only_the_spaces_devices_reach_it() {
    let dir = tempfile::tempdir().unwrap();
    let relay = relay(dir.path(), Limits::default()).await;
    let key = SyncKey::generate().unwrap();
    let path = device_path("", key.space_id(), TAG);
    open(&config(&relay), &SpaceAccess::of(&key)).unwrap().put(&path, b"sealed".to_vec(), PutCondition::IfAbsent).await.unwrap();
    // The same space id with another key's token: refused, whatever the request.
    let stranger = open(&config(&relay), &SpaceAccess::of(&SyncKey::generate().unwrap())).unwrap();
    assert_eq!(stranger.list(&devices_dir("", key.space_id())).await.err(), Some(SyncError::Denied));
    assert_eq!(stranger.get(&path).await.err(), Some(SyncError::Denied));
    assert_eq!(stranger.put(&path, b"x".to_vec(), PutCondition::Always).await.err(), Some(SyncError::Denied));
    assert_eq!(stranger.delete(&path).await.err(), Some(SyncError::Denied));
    relay.stop().await;
}

#[tokio::test]
async fn an_address_without_a_relay_or_without_anything_fails_plainly() {
    let dir = tempfile::tempdir().unwrap();
    let relay = relay(dir.path(), Limits::default()).await;
    let key = SyncKey::generate().unwrap();
    let access = SpaceAccess::of(&key);
    // A server, but no relay under this path.
    let elsewhere = StorageConfig::Relay { url: format!("http://{}/not-here/", relay.addr()) };
    assert!(matches!(open(&elsewhere, &access).unwrap().list(&devices_dir("", key.space_id())).await, Err(SyncError::Storage(_))));
    relay.stop().await;
    // Nothing listening any more.
    let gone = StorageConfig::Relay { url: format!("http://{}", relay_addr_closed().await) };
    assert!(matches!(open(&gone, &access).unwrap().list(&devices_dir("", key.space_id())).await, Err(SyncError::Network(_))));
    // An address that is not one is refused before any request.
    assert!(open(&StorageConfig::Relay { url: "http://relay.example.com".into() }, &access).is_err());
}

/// A port on this computer that nothing listens on.
async fn relay_addr_closed() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap()
}

/// A server that answers every request with `head` and `body`, then closes.
async fn fake(head: &'static str, body: Vec<u8>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let body = body.clone();
            tokio::spawn(async move {
                let mut request = vec![0u8; 8192];
                let _ = socket.read(&mut request).await;
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn a_relay_sending_more_than_any_snapshot_is_not_read() {
    let key = SyncKey::generate().unwrap();
    let access = SpaceAccess::of(&key);
    // It declares more than a device reads: refused before the body.
    let url = fake("HTTP/1.1 200 OK\r\nContent-Length: 17000000\r\nConnection: close\r\n\r\n", Vec::new()).await;
    let store = open(&StorageConfig::Relay { url }, &access).unwrap();
    assert_eq!(store.get(&device_path("", key.space_id(), TAG)).await.err(), Some(SyncError::Corrupted));
    // A listing without a length that goes on past any listing's size.
    let chunked = {
        let mut body = Vec::new();
        let chunk = vec![b' '; 64 * 1024];
        for _ in 0..20 {
            body.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
            body.extend_from_slice(&chunk);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b"0\r\n\r\n");
        body
    };
    let url = fake("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n", chunked).await;
    let store = open(&StorageConfig::Relay { url }, &access).unwrap();
    assert_eq!(store.list(&devices_dir("", key.space_id())).await.err(), Some(SyncError::Corrupted));
    // Not a listing at all.
    let url = fake("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n", b"hello".to_vec()).await;
    let store = open(&StorageConfig::Relay { url }, &access).unwrap();
    assert!(matches!(store.list(&devices_dir("", key.space_id())).await, Err(SyncError::Storage(_))));
}

#[tokio::test]
async fn a_watch_hears_another_device_and_stops_when_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let relay = relay(dir.path(), Limits::default()).await;
    let key = SyncKey::generate().unwrap();
    let access = SpaceAccess::of(&key);
    let config = config(&relay);
    let store = open(&config, &access).unwrap();
    let space = key.space_id();
    store.put(&device_path("", space, TAG), b"laptop".to_vec(), PutCondition::Always).await.unwrap();
    let (sender, mut heard) = tokio::sync::mpsc::unbounded_channel();
    let watching = watch(
        &config,
        &access,
        &devices_dir("", space),
        Box::new(move || {
            let _ = sender.send(());
        }),
    )
    .unwrap()
    .expect("a relay is watched");
    assert!(matches!(watching, Watch::Relay(_)));
    // Another device writes (again, until the watch has asked once and hears it).
    let mut caught = false;
    for n in 0..10u8 {
        store.put(&device_path("", space, OTHER), vec![n; 10], PutCondition::Always).await.unwrap();
        if tokio::time::timeout(Duration::from_secs(2), heard.recv()).await.is_ok() {
            caught = true;
            break;
        }
    }
    assert!(caught, "the watch heard the other device's write");
    drop(watching);
    while heard.try_recv().is_ok() {}
    store.put(&device_path("", space, OTHER), b"after".to_vec(), PutCondition::Always).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(heard.try_recv().is_err(), "a dropped watch hears nothing more");
    // Nothing to watch on S3 or WebDAV; a relay watch needs a runtime, which a test has.
    let dav = StorageConfig::Webdav { url: "https://dav.example.com/".into(), prefix: String::new(), username: "me".into(), password: "pw".to_owned().into() };
    assert!(watch(&dav, &access, "x/", Box::new(|| {})).unwrap().is_none());
    relay.stop().await;
}

#[tokio::test]
async fn a_live_relay_syncs_two_devices_and_tells_of_a_change() {
    let Some(url) = std::env::var("LOCKRA_IT_RELAY_URL").ok().filter(|u| !u.is_empty()) else {
        eprintln!("relay: LOCKRA_IT_RELAY_URL is not set, the live relay is skipped");
        return;
    };
    let config = StorageConfig::Relay { url };
    config.validate().unwrap();
    let key = SyncKey::generate().unwrap();
    let access = SpaceAccess::of(&key);
    let store = open(&config, &access).unwrap();
    let space = key.space_id();
    two_devices_sync_with(&*store, "", &key).await;
    let devices = devices_dir("", space);
    assert_eq!(store.list(&devices).await.unwrap().len(), 2, "one snapshot per device");
    // Another sync key's token is refused at this space, through whatever is in front of the relay.
    let stranger = open(&config, &SpaceAccess::of(&SyncKey::generate().unwrap())).unwrap();
    assert_eq!(stranger.list(&devices).await.err(), Some(SyncError::Denied));

    // A device waiting on the relay hears another one write.
    let (sender, mut heard) = tokio::sync::mpsc::unbounded_channel();
    let watching = watch(
        &config,
        &access,
        &devices,
        Box::new(move || {
            let _ = sender.send(());
        }),
    )
    .unwrap()
    .expect("a relay is watched");
    let mut caught = false;
    for n in 0..10u8 {
        store.put(&device_path("", space, OTHER), vec![n; 10], PutCondition::Always).await.unwrap();
        if tokio::time::timeout(Duration::from_secs(5), heard.recv()).await.is_ok() {
            caught = true;
            break;
        }
    }
    drop(watching);
    assert!(caught, "the watch heard the other device's write");

    // The relay keeps nothing of the test.
    for object in store.list(&devices).await.unwrap() {
        store.delete(&format!("{devices}{}", object.name)).await.unwrap();
    }
    assert!(store.list(&devices).await.unwrap().is_empty());
}
