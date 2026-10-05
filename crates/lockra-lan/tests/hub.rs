//! A hub and its devices on this computer's loopback: the sync step over the hub, what a device
//! may write, keys the hub does not know or no longer pairs with, pairing, and the limits.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use lockra_lan::{
    ClientConfig, FolderStore, HubClient, HubConfig, HubEvent, HubPeer, HubServer, Joined, Key, MAX_CONNECTIONS, PAIRING_TIMEOUT, PairOffer, Pairing, join,
};
use lockra_sync::{PutCondition, RemoteStore, Replica, Space, SpaceKeys, SyncError, SyncState, devices_dir, step};
use tokio::sync::mpsc;
use uuid::Uuid;
use zeroize::Zeroizing;

const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const HUB_ID: Uuid = Uuid::from_u128(0x4c4b_4c4e_0000_0000_0000_0000_0000_0001);

fn key(byte: u8) -> Key {
    Zeroizing::new([byte; 32])
}

/// A replica of a set of names: what a device has, merged by union.
#[derive(Default)]
struct Names(BTreeSet<String>);

impl Replica for Names {
    fn payload(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(serde_json::to_vec(&self.0).unwrap())
    }

    fn absorb(&mut self, payload: &[u8]) -> Result<bool, SyncError> {
        let theirs: BTreeSet<String> = serde_json::from_slice(payload).map_err(|_| SyncError::Corrupted)?;
        let before = self.0.len();
        self.0.extend(theirs);
        Ok(self.0.len() > before)
    }
}

/// A device of the space: its number, its state on the storage it uses, its replica.
struct Device {
    number: u64,
    name: &'static str,
    state: SyncState,
    names: Names,
}

impl Device {
    fn new(number: u64, name: &'static str, names: &[&str]) -> Self {
        Self { number, name, state: SyncState::default(), names: Names(names.iter().map(|n| (*n).to_owned()).collect()) }
    }

    async fn sync(&mut self, store: &dyn RemoteStore, keys: &SpaceKeys) -> Result<lockra_sync::Outcome, SyncError> {
        let space = Space { prefix: "", keys, device: self.number, device_name: self.name, keyring: b"keyring", seq_floor: 0 };
        step(store, &space, &mut self.state, &mut self.names, 1_000).await
    }
}

struct Hub {
    _folder: tempfile::TempDir,
    store: FolderStore,
    server: HubServer,
    events: mpsc::UnboundedReceiver<HubEvent>,
    config: HubConfig,
    keys: SpaceKeys,
}

impl Hub {
    /// A hub with devices `peers` (their keys), the hub itself device 1.
    async fn start(peers: &[(Uuid, Key)]) -> Self {
        let folder = tempfile::tempdir().unwrap();
        let store = FolderStore::new(folder.path());
        let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
        let config = HubConfig {
            hub_id: HUB_ID,
            space_id: keys.space_id(),
            own_tag: keys.device_tag(1),
            peers: peers.iter().map(|(peer_id, key)| HubPeer { peer_id: *peer_id, key: key.clone(), tag: None }).collect(),
            removed: Vec::new(),
            pairing: None,
        };
        let (sender, events) = mpsc::unbounded_channel();
        let server = HubServer::start(store.clone(), 0, config.clone(), sender).await.unwrap();
        Self { _folder: folder, store, server, events, config, keys }
    }

    fn client(&self, key: &Key) -> HubClient {
        HubClient::new(ClientConfig { hub_id: HUB_ID, key: key.clone(), port: self.server.port(), addrs: vec![LOOPBACK], broadcast: false })
    }

    fn events(&mut self) -> Vec<HubEvent> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            out.push(event);
        }
        out
    }

    fn dir(&self) -> String {
        devices_dir("", self.keys.space_id())
    }
}

#[tokio::test]
async fn a_paired_device_syncs_through_the_hub_as_through_any_storage() {
    let phone_id = Uuid::new_v4();
    let mut hub = Hub::start(&[(phone_id, key(2))]).await;
    let phone_store = hub.client(&key(2));
    let mut laptop = Device::new(1, "Laptop", &["GitHub"]);
    let mut phone = Device::new(2, "Phone", &["Mail"]);

    // The phone writes its snapshot into the hub's folder, under the tag it registered.
    let first = phone.sync(&phone_store, &hub.keys).await.unwrap();
    assert!(first.wrote);
    let tag = hub.keys.device_tag(2);
    assert_eq!(hub.store.list(&hub.dir()).await.unwrap().len(), 1);
    assert_eq!(hub.events(), [HubEvent::PeerTag { peer_id: phone_id, tag: tag.clone() }, HubEvent::PeerWrote { peer_id: phone_id }]);
    // The hub's own run reads it from its folder and writes its own; the phone takes that in.
    let taken = laptop.sync(&hub.store, &hub.keys).await.unwrap();
    assert_eq!(taken.brought, ["Phone"]);
    let back = phone.sync(&phone_store, &hub.keys).await.unwrap();
    assert_eq!(back.brought, ["Laptop"]);
    assert_eq!(phone.names.0, laptop.names.0);
    assert_eq!(phone.names.0.len(), 2);
    // Nothing new: the phone writes nothing, on the same connection.
    let quiet = phone.sync(&phone_store, &hub.keys).await.unwrap();
    assert!(!quiet.wrote && !quiet.changed);
    assert_eq!(phone_store.found_at(), Some(LOOPBACK));

    // The conditions hold through the hub.
    let path = format!("{}{tag}.lks", hub.dir());
    let current = phone_store.get(&path).await.unwrap().unwrap().1.unwrap();
    assert_eq!(phone_store.put(&path, b"x".to_vec(), PutCondition::IfAbsent).await.err(), Some(SyncError::Conflict));
    assert_eq!(phone_store.put(&path, b"x".to_vec(), PutCondition::IfMatch("stale".into())).await.err(), Some(SyncError::Conflict));
    assert!(phone_store.put(&path, b"x".to_vec(), PutCondition::IfMatch(current)).await.unwrap().is_some());
    phone_store.delete(&path).await.unwrap();
    assert_eq!(phone_store.get(&path).await.unwrap(), None);
}

#[tokio::test]
async fn a_device_writes_its_own_object_only_and_reads_nothing_but_the_space() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let mut hub = Hub::start(&[(a, key(2)), (b, key(3))]).await;
    let (store_a, store_b) = (hub.client(&key(2)), hub.client(&key(3)));
    let dir = hub.dir();
    let tag_a = hub.keys.device_tag(2);
    store_a.put(&format!("{dir}{tag_a}.lks"), b"a".to_vec(), PutCondition::Always).await.unwrap();
    hub.events();

    // Not under the hub's tag, nor under another device's, nor anywhere but the space.
    let own = format!("{dir}{}.lks", hub.config.own_tag);
    assert_eq!(store_b.put(&own, b"b".to_vec(), PutCondition::Always).await.err(), Some(SyncError::Denied));
    assert_eq!(store_b.put(&format!("{dir}{tag_a}.lks"), b"b".to_vec(), PutCondition::Always).await.err(), Some(SyncError::Denied));
    assert_eq!(store_b.delete(&format!("{dir}{tag_a}.lks")).await.err(), Some(SyncError::Denied));
    let elsewhere = devices_dir("", Uuid::new_v4());
    assert_eq!(store_b.list(&elsewhere).await.err(), Some(SyncError::Denied));
    assert_eq!(store_b.get(&format!("{elsewhere}{tag_a}.lks")).await.err(), Some(SyncError::Denied));
    assert_eq!(store_b.get("../../etc/passwd").await.err(), Some(SyncError::Denied));
    assert_eq!(store_b.put(&format!("{dir}NOT-HEX.lks"), b"b".to_vec(), PutCondition::Always).await.err(), Some(SyncError::Denied));
    assert_eq!(hub.store.get(&format!("{dir}{tag_a}.lks")).await.unwrap().unwrap().0, b"a");
    assert!(hub.events().is_empty(), "nothing written");
    // Reading the others' objects is what a run does.
    assert_eq!(store_b.get(&format!("{dir}{tag_a}.lks")).await.unwrap().unwrap().0, b"a");
    assert_eq!(store_b.list(&dir).await.unwrap().len(), 1);

    // A device that became a new one writes under its new tag, which is then its alone.
    let renumbered = hub.keys.device_tag(22);
    store_a.put(&format!("{dir}{renumbered}.lks"), b"a2".to_vec(), PutCondition::Always).await.unwrap();
    assert_eq!(hub.events(), [HubEvent::PeerTag { peer_id: a, tag: renumbered.clone() }, HubEvent::PeerWrote { peer_id: a }]);
    assert_eq!(store_b.put(&format!("{dir}{renumbered}.lks"), b"b".to_vec(), PutCondition::Always).await.err(), Some(SyncError::Denied));
}

#[tokio::test]
async fn a_key_the_hub_does_not_know_reaches_nothing_and_a_removed_device_is_told() {
    let peer = Uuid::new_v4();
    let mut hub = Hub::start(&[(peer, key(2))]).await;
    let stranger = hub.client(&key(9));
    assert!(matches!(stranger.list(&hub.dir()).await, Err(SyncError::Network(_))));
    // Removed: its key kept to tell it so, on a connection it had open too.
    let device = hub.client(&key(2));
    device.list(&hub.dir()).await.unwrap();
    hub.config.peers.clear();
    hub.config.removed.push(key(2));
    hub.server.update(hub.config.clone());
    assert_eq!(device.list(&hub.dir()).await.err(), Some(SyncError::WrongCredentials));
    assert_eq!(hub.client(&key(2)).list(&hub.dir()).await.err(), Some(SyncError::WrongCredentials));
}

#[tokio::test]
async fn a_pairing_waits_for_the_user_and_the_offer_opens_one_handshake() {
    let mut hub = Hub::start(&[]).await;
    // An offer past its time opens nothing, whether or not it was withdrawn.
    hub.config.pairing = Some(Pairing { key: key(7), until: tokio::time::Instant::now() });
    hub.server.update(hub.config.clone());
    let lapsed = PairOffer {
        hub_id: HUB_ID,
        hub_name: "Desktop".into(),
        space_id: hub.keys.space_id(),
        addrs: vec![LOOPBACK],
        port: hub.server.port(),
        key: key(7),
        expires_at_ms: u64::MAX,
    };
    assert!(join(&lapsed, "Pixel 8", "android").await.is_err());
    hub.config.pairing = Some(Pairing { key: key(7), until: tokio::time::Instant::now() + PAIRING_TIMEOUT });
    hub.server.update(hub.config.clone());
    let offer = PairOffer {
        hub_id: HUB_ID,
        hub_name: "Desktop".into(),
        space_id: hub.keys.space_id(),
        addrs: vec![LOOPBACK],
        port: hub.server.port(),
        key: key(7),
        expires_at_ms: u64::MAX,
    };
    let joining = join(&offer, "Pixel 8", "android").await.unwrap();
    assert_eq!(joining.code.len(), 6);
    let request = hub.events.recv().await.unwrap();
    assert_eq!(request, HubEvent::PairRequest { name: "Pixel 8".into(), platform: "android".into(), code: joining.code.clone() });
    // The offer is spent: nobody else gets in with it meanwhile.
    assert!(join(&offer, "Other", "linux").await.is_err());
    let welcome = Zeroizing::new(b"the space's keys, from the core".to_vec());
    hub.server.answer(Some(welcome.clone()));
    match joining.answer().await.unwrap() {
        Joined::Welcome(received) => assert_eq!(received.as_slice(), welcome.as_slice()),
        Joined::Refused => panic!("refused"),
    }
    assert_eq!(hub.events.recv().await.unwrap(), HubEvent::PairWelcomed);

    // Refused.
    hub.config.pairing = Some(Pairing { key: key(8), until: tokio::time::Instant::now() + PAIRING_TIMEOUT });
    hub.server.update(hub.config.clone());
    let offer = PairOffer { key: key(8), ..offer };
    let joining = join(&offer, "Laptop\u{7}", "linux; rm").await.unwrap();
    assert!(matches!(hub.events.recv().await.unwrap(), HubEvent::PairRequest { name, platform, .. } if name == "Laptop" && platform == "linuxrm"));
    hub.server.answer(None);
    assert!(matches!(joining.answer().await.unwrap(), Joined::Refused));
    assert_eq!(hub.events.recv().await.unwrap(), HubEvent::PairEnded);
}

#[tokio::test]
async fn a_hub_restarted_meanwhile_is_reached_again_and_takes_the_tag_again() {
    let peer = Uuid::new_v4();
    let hub = Hub::start(&[(peer, key(2))]).await;
    let Hub { _folder, store, server, config, keys, .. } = hub;
    let port = server.port();
    let device = HubClient::new(ClientConfig { hub_id: HUB_ID, key: key(2), port, addrs: vec![LOOPBACK], broadcast: false });
    let path = format!("{}{}.lks", devices_dir("", keys.space_id()), keys.device_tag(2));
    device.put(&path, b"1".to_vec(), PutCondition::Always).await.unwrap();
    // The hub forgot the binding: the write on the same connection registers again.
    server.update(config.clone());
    device.put(&path, b"2".to_vec(), PutCondition::Always).await.unwrap();
    // Stopped, and started again on its port: the kept connection is gone, a new one opens.
    server.stop();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let restarted = loop {
        let (sender, _events) = mpsc::unbounded_channel();
        match HubServer::start(store.clone(), port, config.clone(), sender).await {
            Ok(server) => break server,
            Err(error) => assert!(std::time::Instant::now() < deadline, "the port came free: {error}"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    assert_eq!(device.get(&path).await.unwrap().unwrap().0, b"2");
    device.put(&path, b"3".to_vec(), PutCondition::Always).await.unwrap();
    assert_eq!(store.get(&path).await.unwrap().unwrap().0, b"3");
    drop(restarted);
}

#[tokio::test]
async fn a_hub_serves_so_many_connections_at_once_and_tells_the_next_it_is_busy() {
    let peers: Vec<(Uuid, Key)> = (0..=MAX_CONNECTIONS).map(|i| (Uuid::new_v4(), key(10 + u8::try_from(i).unwrap()))).collect();
    let hub = Hub::start(&peers).await;
    let clients: Vec<Arc<HubClient>> = peers.iter().map(|(_, key)| Arc::new(hub.client(key))).collect();
    for client in &clients[..MAX_CONNECTIONS] {
        client.list(&hub.dir()).await.unwrap();
    }
    match clients[MAX_CONNECTIONS].list(&hub.dir()).await {
        Err(SyncError::Network(why)) => assert!(why.contains("busy"), "{why}"),
        other => panic!("{other:?}"),
    }
    // One leaves: the next gets in.
    drop(clients);
    let late = hub.client(&peers[MAX_CONNECTIONS].1);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while late.list(&hub.dir()).await.is_err() {
        assert!(std::time::Instant::now() < deadline, "a connection slot came free");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
