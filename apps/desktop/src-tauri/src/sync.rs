//! The sync's network side on the desktop: lockra-remote over HTTPS for the storage of the user's
//! own (S3-compatible object storage or WebDAV, through OpenDAL), and lockra-lan for the LAN: the
//! hub's copy of the space in `<app data>/lan-store`, its server, a client's way to its hub, and
//! pairing. The core opens any of them only for a space the user set up.

use std::io;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use lockra_core::StorageConfig;
use lockra_core::ports::{HubServe, LanClientConfig, LanEvent, LanEvents, LanFuture, LanJoining, LanService, PairOffer, RemoteStore, SyncError, SyncTransport};
use lockra_lan::{ClientConfig, FolderStore, HubClient, HubConfig, HubEvent, HubPeer, HubServer, Joined, Joining, Pairing};
use parking_lot::Mutex;
use tokio::sync::mpsc;
use zeroize::Zeroizing;

/// The folder of the hub's copy, under the app's data folder.
pub const LAN_STORE_DIR: &str = "lan-store";

/// lockra-remote's storage, and the LAN.
pub struct DesktopSync {
    /// The hub's copy: one store for the hub's own runs and its server, so their reads and writes
    /// take turns.
    folder: FolderStore,
    server: Mutex<Option<HubServer>>,
    /// Settings the core gave while the server was starting: it serves with them once started.
    pending: Mutex<Option<HubServe>>,
}

impl DesktopSync {
    pub fn new(data_dir: &Path) -> Self {
        Self { folder: FolderStore::new(data_dir.join(LAN_STORE_DIR)), server: Mutex::new(None), pending: Mutex::new(None) }
    }
}

/// The core's settings as lockra-lan's server takes them.
fn hub_config(config: &HubServe) -> HubConfig {
    HubConfig {
        hub_id: config.hub_id,
        space_id: config.space_id,
        own_tag: config.own_tag.clone(),
        peers: config.peers.iter().map(|peer| HubPeer { peer_id: peer.peer_id, key: peer.key.clone(), tag: peer.tag.clone() }).collect(),
        removed: config.removed.clone(),
        pairing: config.pairing.as_ref().map(|(key, until)| Pairing { key: key.clone(), until: *until }),
    }
}

/// lockra-lan's events as the core takes them.
fn lan_event(event: HubEvent) -> LanEvent {
    match event {
        HubEvent::PeerWrote { peer_id } => LanEvent::PeerWrote { peer_id },
        HubEvent::PeerTag { peer_id, tag } => LanEvent::PeerTag { peer_id, tag },
        HubEvent::PairRequest { name, platform, code } => LanEvent::PairRequest { name, platform, code },
        HubEvent::PairWelcomed => LanEvent::PairWelcomed,
        HubEvent::PairEnded => LanEvent::PairEnded,
    }
}

impl SyncTransport for DesktopSync {
    fn open(&self, config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        Ok(Arc::new(lockra_remote::Storage::open(config)?))
    }

    fn open_hub_store(&self, _space_id: uuid::Uuid) -> Result<Arc<dyn RemoteStore>, SyncError> {
        Ok(Arc::new(self.folder.clone()))
    }

    fn open_lan_client(&self, config: &LanClientConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        let mut key = Zeroizing::new([0u8; 32]);
        if config.psk.len() != key.len() {
            return Err(SyncError::WrongCredentials);
        }
        key.copy_from_slice(&config.psk);
        let addrs = config.addrs.iter().filter_map(|addr| addr.parse::<IpAddr>().ok()).collect();
        Ok(Arc::new(HubClient::new(ClientConfig { hub_id: config.hub_id, key, port: config.port, addrs, broadcast: true })))
    }

    fn lan(&self) -> Option<&dyn LanService> {
        Some(self)
    }
}

impl LanService for DesktopSync {
    fn serve(&self, config: HubServe, events: LanEvents) -> LanFuture<'_, u16> {
        Box::pin(async move {
            if let Some(server) = self.server.lock().as_ref() {
                server.update(hub_config(&config));
                return Ok(server.port());
            }
            let (sender, mut received) = mpsc::unbounded_channel();
            // The port the vault says, or another when it is taken.
            let server = match HubServer::start(self.folder.clone(), config.port, hub_config(&config), sender.clone()).await {
                Ok(server) => server,
                Err(error) if error.kind() == io::ErrorKind::AddrInUse && config.port != 0 => {
                    HubServer::start(self.folder.clone(), 0, hub_config(&config), sender).await.map_err(|e| SyncError::Network(e.to_string()))?
                }
                Err(error) => return Err(SyncError::Network(error.to_string())),
            };
            let port = server.port();
            if let Some(newer) = self.pending.lock().take() {
                server.update(hub_config(&newer));
            }
            *self.server.lock() = Some(server);
            tokio::spawn(async move {
                while let Some(event) = received.recv().await {
                    events(lan_event(event));
                }
            });
            Ok(port)
        })
    }

    fn update(&self, config: HubServe) {
        match self.server.lock().as_ref() {
            Some(server) => server.update(hub_config(&config)),
            None => *self.pending.lock() = Some(config),
        }
    }

    fn answer(&self, welcome: Option<Zeroizing<Vec<u8>>>) {
        if let Some(server) = self.server.lock().as_ref() {
            server.answer(welcome);
        }
    }

    fn stop(&self) {
        self.pending.lock().take();
        if let Some(server) = self.server.lock().take() {
            server.stop();
        }
    }

    fn addresses(&self) -> Vec<IpAddr> {
        lockra_lan::local_addresses()
    }

    fn join<'a>(&'a self, offer: &'a PairOffer, name: &'a str, platform: &'a str) -> LanFuture<'a, Box<dyn LanJoining>> {
        Box::pin(async move { Ok(Box::new(DesktopJoining(lockra_lan::join(offer, name, platform).await?)) as Box<dyn LanJoining>) })
    }
}

/// A pairing under way, as the core takes it.
struct DesktopJoining(Joining);

impl LanJoining for DesktopJoining {
    fn code(&self) -> String {
        self.0.code.clone()
    }

    fn answer(self: Box<Self>) -> LanFuture<'static, Option<Zeroizing<Vec<u8>>>> {
        Box::pin(async move {
            match self.0.answer().await? {
                Joined::Welcome(welcome) => Ok(Some(welcome)),
                Joined::Refused => Ok(None),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use lockra_core::ports::HubServePeer;
    use uuid::Uuid;

    use super::*;

    fn sync() -> (tempfile::TempDir, DesktopSync) {
        let folder = tempfile::tempdir().unwrap();
        let sync = DesktopSync::new(folder.path());
        (folder, sync)
    }

    fn serve_config(port: u16, pairing: Option<[u8; 32]>) -> HubServe {
        HubServe {
            hub_id: Uuid::from_u128(1),
            space_id: Uuid::from_u128(2),
            own_tag: "00".repeat(16),
            port,
            peers: vec![HubServePeer { peer_id: Uuid::from_u128(3), key: Zeroizing::new([5; 32]), tag: None }],
            removed: Vec::new(),
            pairing: pairing.map(|key| (Zeroizing::new(key), tokio::time::Instant::now() + Duration::from_secs(120))),
        }
    }

    fn quiet() -> LanEvents {
        Arc::new(|_| {})
    }

    #[test]
    fn a_storage_opens_without_contacting_it_and_plain_http_elsewhere_is_refused() {
        let (_folder, sync) = sync();
        let dav = |url: &str| StorageConfig::Webdav { url: url.into(), prefix: String::new(), username: "me".into(), password: Zeroizing::new("pw".into()) };
        assert!(
            matches!(sync.open(&dav("https://dav.example.com/dav/")), Ok(storage) if !storage.conditional_puts()),
            "WebDAV opens, and writes without conditions"
        );
        assert!(sync.open(&dav("http://192.168.1.2/dav/")).is_err());
        // The hub's copy is a folder store; a client's key must be one a hub gives.
        assert!(sync.open_hub_store(Uuid::nil()).unwrap().conditional_puts());
        let client = |psk: Vec<u8>| LanClientConfig { hub_id: Uuid::nil(), peer_id: Uuid::nil(), psk: Zeroizing::new(psk), port: 47_100, addrs: vec!["192.168.1.20".into(), "not an address".into()] };
        assert!(sync.open_lan_client(&client(vec![7; 32])).is_ok());
        assert!(matches!(sync.open_lan_client(&client(vec![7; 16])), Err(SyncError::WrongCredentials)));
        assert!(sync.addresses().iter().all(|ip| lockra_lan::local_address(*ip)));
    }

    #[tokio::test]
    async fn the_hub_serves_on_another_port_when_its_own_is_taken_and_stops() {
        let (_folder, sync) = sync();
        let holder = std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let taken = holder.local_addr().unwrap().port();
        let port = sync.serve(serve_config(taken, None), quiet()).await.unwrap();
        assert_ne!(port, taken);
        // Serving again goes on with the same server.
        assert_eq!(sync.serve(serve_config(taken, None), quiet()).await.unwrap(), port);
        sync.update(serve_config(port, None));
        sync.stop();
        assert!(sync.server.lock().is_none());
        // Settings given while no server runs are the ones it starts with.
        sync.update(serve_config(0, None));
        assert!(sync.pending.lock().is_some());
        sync.stop();
        assert!(sync.pending.lock().is_none());
    }

    #[tokio::test]
    async fn a_device_pairs_with_the_hub_through_the_shells_own_lan() {
        let (_folder, hub) = sync();
        let (requests, mut asked) = mpsc::unbounded_channel();
        let events: LanEvents = Arc::new(move |event| {
            let _ = requests.send(event);
        });
        let port = hub.serve(serve_config(0, Some([9; 32])), events).await.unwrap();
        let offer = PairOffer {
            hub_id: Uuid::from_u128(1),
            hub_name: "Desktop".into(),
            space_id: Uuid::from_u128(2),
            addrs: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            port,
            key: Zeroizing::new([9; 32]),
            expires_at_ms: u64::MAX,
        };
        let (_other, phone) = sync();
        let joining = phone.join(&offer, "Pixel 8", "android").await.unwrap();
        let code = joining.code();
        assert_eq!(asked.recv().await, Some(LanEvent::PairRequest { name: "Pixel 8".into(), platform: "android".into(), code }));
        hub.answer(Some(Zeroizing::new(b"welcome".to_vec())));
        assert_eq!(joining.answer().await.unwrap().map(|welcome| welcome.to_vec()), Some(b"welcome".to_vec()));
        assert_eq!(asked.recv().await, Some(LanEvent::PairWelcomed));
        hub.stop();
    }
}
