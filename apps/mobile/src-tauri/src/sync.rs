//! The sync's network side on the phone: lockra-remote over HTTPS as on the desktop (on Android,
//! with the certificate authorities the system keeps), lockra-lan's client for a computer's LAN hub
//! (the phone pairs with a hub; it is never one), and joining a space or pairing with a hub from
//! what the camera reads: an invitation, which holds the storage's credentials and the sync key,
//! or a hub's pairing code. Either goes from the camera to the core and never to the webview.

use std::net::IpAddr;
use std::sync::Arc;

use lockra_core::ports::{
    HubServe, LanClientConfig, LanEvents, LanFuture, LanJoining, LanService, PairOffer, PortError, RemoteStore, SyncError, SyncTransport,
};
use lockra_core::ui::JoinSource;
use lockra_core::{Core, CoreError, StorageConfig};
use lockra_lan::{ClientConfig, HubClient, Joined, Joining};
use zeroize::Zeroizing;

use crate::scanner::{self, Scan};

/// lockra-remote's storage, and a LAN hub's client.
#[derive(Debug, Default, Clone, Copy)]
pub struct PhoneSync;

impl SyncTransport for PhoneSync {
    fn open(&self, config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        Ok(Arc::new(lockra_remote::Storage::open(config)?))
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

/// The LAN as a client only: the phone pairs with a computer and syncs through it, and serves no
/// device itself.
impl LanService for PhoneSync {
    fn serve(&self, _config: HubServe, _events: LanEvents) -> LanFuture<'_, u16> {
        Box::pin(async { Err(SyncError::Storage("the phone is never a LAN hub".into())) })
    }

    fn update(&self, _config: HubServe) {}

    fn answer(&self, _welcome: Option<Zeroizing<Vec<u8>>>) {}

    fn stop(&self) {}

    fn addresses(&self) -> Vec<IpAddr> {
        lockra_lan::local_addresses()
    }

    fn join<'a>(&'a self, offer: &'a PairOffer, name: &'a str, platform: &'a str) -> LanFuture<'a, Box<dyn LanJoining>> {
        Box::pin(async move { Ok(Box::new(PhoneJoining(lockra_lan::join(offer, name, platform).await?)) as Box<dyn LanJoining>) })
    }
}

/// A pairing request to a hub, as the core takes it.
struct PhoneJoining(Joining);

impl LanJoining for PhoneJoining {
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

/// What a scan means for joining: an invitation read joins its space as `sync_join` does, with
/// `password`, `device` and `space_password` (`true`); otherwise as [`scanner::read`] (`false`).
pub async fn join(
    core: &Core,
    scan: Result<Scan, PortError>,
    password: Zeroizing<String>,
    device: String,
    space_password: Option<Zeroizing<String>>,
) -> Result<bool, CoreError> {
    let Some(text) = scanner::read(core, scan)? else { return Ok(false) };
    core.sync_join(JoinSource::Invite { text, code: None }, password, device, space_password).await.map(|()| true)
}

/// What a scan means for pairing: a hub's pairing code read pairs with it as `sync_lan_join` does,
/// with `password` and `device` (`true`, once the hub's user allowed it); otherwise as
/// [`scanner::read`] (`false`).
pub async fn pair(core: &Core, scan: Result<Scan, PortError>, password: Zeroizing<String>, device: String) -> Result<bool, CoreError> {
    let Some(text) = scanner::read(core, scan)? else { return Ok(false) };
    core.sync_lan_join(text, password, device).await.map(|()| true)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::time::Duration;

    use lockra_lan::{FolderStore, HubConfig, HubEvent, HubServer, Pairing};
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn a_hub_s_client_opens_with_the_key_a_hub_gives_and_the_phone_serves_no_device() {
        let sync = PhoneSync;
        let client = |psk: Vec<u8>| LanClientConfig {
            hub_id: Uuid::nil(),
            peer_id: Uuid::nil(),
            psk: Zeroizing::new(psk),
            port: 47_100,
            addrs: vec!["192.168.1.20".into(), "not an address".into()],
        };
        assert!(sync.open_lan_client(&client(vec![7; 32])).is_ok());
        assert!(matches!(sync.open_lan_client(&client(vec![7; 16])), Err(SyncError::WrongCredentials)));
        assert!(sync.addresses().iter().all(|ip| lockra_lan::local_address(*ip)));
        let config =
            HubServe { hub_id: Uuid::nil(), space_id: Uuid::nil(), own_tag: String::new(), port: 0, peers: Vec::new(), removed: Vec::new(), pairing: None };
        let served = tauri::async_runtime::block_on(sync.serve(config.clone(), Arc::new(|_| {})));
        assert!(matches!(served, Err(SyncError::Storage(_))));
        // Nothing to update, answer or stop.
        sync.update(config);
        sync.answer(None);
        sync.stop();
        assert!(sync.open_hub_store(Uuid::nil()).is_err());
    }

    #[tokio::test]
    async fn the_phone_asks_a_hub_to_pair_and_takes_its_welcome() {
        let folder = tempfile::tempdir().unwrap();
        let (sender, mut events) = mpsc::unbounded_channel();
        let pairing = Pairing { key: Zeroizing::new([9; 32]), until: tokio::time::Instant::now() + Duration::from_secs(120) };
        let config = HubConfig {
            hub_id: Uuid::from_u128(1),
            space_id: Uuid::from_u128(2),
            own_tag: "00".repeat(16),
            peers: Vec::new(),
            removed: Vec::new(),
            pairing: Some(pairing),
        };
        let hub = HubServer::start(FolderStore::new(folder.path().to_path_buf()), 0, config, sender).await.unwrap();
        let offer = PairOffer {
            hub_id: Uuid::from_u128(1),
            hub_name: "Desktop".into(),
            space_id: Uuid::from_u128(2),
            addrs: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            port: hub.port(),
            key: Zeroizing::new([9; 32]),
            expires_at_ms: u64::MAX,
        };
        let joining = PhoneSync.join(&offer, "Pixel 8", "android").await.unwrap();
        let code = joining.code();
        assert_eq!(events.recv().await, Some(HubEvent::PairRequest { name: "Pixel 8".into(), platform: "android".into(), code }));
        hub.answer(Some(Zeroizing::new(b"welcome".to_vec())));
        assert_eq!(joining.answer().await.unwrap().map(|welcome| welcome.to_vec()), Some(b"welcome".to_vec()));
        hub.stop();
    }

    #[tokio::test]
    async fn a_refused_request_ends_without_a_welcome() {
        let folder = tempfile::tempdir().unwrap();
        let (sender, mut events) = mpsc::unbounded_channel();
        let pairing = Pairing { key: Zeroizing::new([4; 32]), until: tokio::time::Instant::now() + Duration::from_secs(120) };
        let config = HubConfig {
            hub_id: Uuid::from_u128(1),
            space_id: Uuid::from_u128(2),
            own_tag: "00".repeat(16),
            peers: Vec::new(),
            removed: Vec::new(),
            pairing: Some(pairing),
        };
        let hub = HubServer::start(FolderStore::new(folder.path().to_path_buf()), 0, config, sender).await.unwrap();
        let offer = PairOffer {
            hub_id: Uuid::from_u128(1),
            hub_name: "Desktop".into(),
            space_id: Uuid::from_u128(2),
            addrs: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            port: hub.port(),
            key: Zeroizing::new([4; 32]),
            expires_at_ms: u64::MAX,
        };
        let joining = PhoneSync.join(&offer, "Pixel 8", "android").await.unwrap();
        assert!(matches!(events.recv().await, Some(HubEvent::PairRequest { .. })));
        hub.answer(None);
        assert_eq!(joining.answer().await.unwrap().map(|welcome| welcome.to_vec()), None);
        hub.stop();
    }
}
