//! Sync over the local network, in the core (docs/formats.md §10, docs/security.md "Sync"): this
//! computer as the hub (its server runs through the `LanService` port, lockra-lan in the shells),
//! pairing a device with a hub, and the storage of the user's own added to a space or taken away.
//! The runs themselves are the other storages' (`run_sync`).

use std::net::IpAddr;

use lockra_sync::{Invite, LanWelcome, PairOffer, PairTextError, find_space};

use super::*;
use crate::ports::{HubServe, HubServePeer, LanEvent, LanEvents, LanKey, LanService};
use crate::sync::{CloudLocal, LanPeer, MAX_DEVICE_NAME_CHARS, MAX_REMOVED_KEYS, TransportLocal, lan_key};
use crate::ui::{LanJoiningView, LanOffer, PairRequestView, StorageSource};

/// How long a pairing offer stands.
pub const LAN_OFFER_TIME: Duration = Duration::from_secs(120);
/// The port a hub listens on first; the server takes another when it is taken.
pub const LAN_PORT: u16 = 47_100;
/// Devices a hub pairs with (lockra-lan serves no more).
pub const MAX_LAN_PEERS: usize = 32;

/// The LAN hub's server and pairing, as this run of the app knows them.
#[derive(Default)]
pub(super) struct LanRuntime {
    /// The server runs. It goes on serving while the vault is locked, with what it had (the devices
    /// write to the hub's copy; the hub takes their snapshots in when unlocked).
    pub serving: bool,
    /// The settings it serves with.
    pub served: Option<HubServe>,
    /// The offer that stands: its key, until when, and when that is in Unix milliseconds.
    pub offer: Option<(LanKey, Instant, u64)>,
    /// A device asking to pair, waiting for the user's answer.
    pub request: Option<PairRequestView>,
    /// This device asking a hub to pair.
    pub joining: Option<LanJoiningView>,
    /// Tags the devices registered while the vault was locked: kept when it unlocks.
    pub tags: Vec<(Uuid, String)>,
}

/// A new random 32-byte key, and its Base64 for the vault.
fn new_lan_key() -> CoreResult<(LanKey, Zeroizing<String>)> {
    let mut key = Zeroizing::new([0u8; 32]);
    getrandom::fill(key.as_mut()).map_err(|_| ErrorCode::Internal)?;
    let text = Zeroizing::new(BASE64.encode(key.as_ref()));
    Ok((key, text))
}

/// The name a platform goes by on the wire.
fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "windows",
        Platform::Macos => "macos",
        Platform::Linux => "linux",
        Platform::Android => "android",
    }
}

/// What the hub's server serves: the devices and keys from the vault, the offer from the runtime.
fn hub_serve(sync: &SyncLocal, device: u64, offer: Option<&(LanKey, Instant, u64)>) -> Option<HubServe> {
    let Some(LanLocal::Hub { hub_id, port, peers, removed, .. }) = &sync.lan else { return None };
    Some(HubServe {
        hub_id: *hub_id,
        space_id: sync.space_id,
        own_tag: sync.keys().ok()?.device_tag(device),
        port: *port,
        peers: peers.iter().filter_map(|peer| Some(HubServePeer { peer_id: peer.peer_id, key: lan_key(&peer.psk)?, tag: peer.tag.clone() })).collect(),
        removed: removed.iter().filter_map(|key| lan_key(key)).collect(),
        pairing: offer.map(|(key, until, _)| (key.clone(), *until)),
    })
}

fn pairing_error(error: PairTextError) -> CoreError {
    match error {
        PairTextError::Expired => ErrorCode::SyncPairingExpired.into(),
        PairTextError::NotOne => ErrorCode::SyncPairingInvalid.into(),
    }
}

impl Core {
    fn lan_service(&self) -> CoreResult<&dyn LanService> {
        self.shared.ports.sync.lan().ok_or_else(|| ErrorCode::SyncLanUnavailable.into())
    }

    /// Where the server's events reach this core (without keeping it alive).
    fn lan_events(&self) -> LanEvents {
        let shared = Arc::downgrade(&self.shared);
        Arc::new(move |event| {
            if let Some(shared) = shared.upgrade() {
                Core { shared }.on_lan_event(event);
            }
        })
    }

    /// Serve the hub's copy when this device is the hub, with the vault's devices and the offer;
    /// stop when it no longer is. While locked the server goes on as it was.
    pub(super) fn serve_lan(&self) {
        let Some(service) = self.shared.ports.sync.lan() else { return };
        let (config, start, stop) = {
            let mut st = self.lock();
            let PhaseState::Unlocked(session) = &st.phase else { return };
            let config = session.data.sync().and_then(|sync| hub_serve(sync, session.data.device(), st.lan.offer.as_ref()));
            let was_serving = std::mem::replace(&mut st.lan.serving, config.is_some());
            if config.is_none() {
                st.lan = LanRuntime { joining: st.lan.joining.take(), ..LanRuntime::default() };
            } else {
                st.lan.served.clone_from(&config);
            }
            (config.clone(), config.is_some() && !was_serving, config.is_none() && was_serving)
        };
        let Some(config) = config else {
            if stop {
                service.stop();
            }
            return;
        };
        if !start {
            service.update(config);
            return;
        }
        let core = self.clone();
        let events = self.lan_events();
        tokio::spawn(async move {
            let Some(service) = core.shared.ports.sync.lan() else { return };
            let wanted = config.port;
            match service.serve(config, events).await {
                Ok(port) if port != wanted => core.keep_lan_port(port),
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(?error, "the LAN hub did not start");
                    core.lock().lan.serving = false;
                    core.changed();
                }
            }
        });
    }

    /// The server listens elsewhere than the vault says (its port was taken): the vault says so
    /// from now on, so the devices' next pairing offers name it.
    fn keep_lan_port(&self, port: u16) {
        {
            let mut st = self.lock();
            let Ok(session) = unlocked_mut(&mut st) else { return };
            let Some(LanLocal::Hub { port: kept, .. }) = session.data.sync_mut().and_then(|sync| sync.lan.as_mut()) else { return };
            let previous = std::mem::replace(kept, port);
            if let Err(error) = self.save(&mut st, false, move |s| {
                if let Some(LanLocal::Hub { port, .. }) = s.data.sync_mut().and_then(|sync| sync.lan.as_mut()) {
                    *port = previous;
                }
            }) {
                tracing::warn!(?error, "the hub's port was not kept");
            }
            if let Some(served) = st.lan.served.as_mut() {
                served.port = port;
            }
        }
        self.changed();
    }

    /// What the hub's server tells.
    fn on_lan_event(&self, event: LanEvent) {
        match event {
            LanEvent::PeerWrote { .. } => {
                // Unlocked, the hub takes the device's snapshot in now; locked, at the unlock.
                let mut st = self.lock();
                if configured_stores(&st).contains(&Store::Lan) {
                    let runtime = st.sync.store_mut(Store::Lan);
                    runtime.held = false;
                    runtime.at = Some(Instant::now());
                }
                drop(st);
                self.shared.wake.notify_one();
            }
            LanEvent::PeerTag { peer_id, tag } => {
                let unlocked = matches!(self.lock().phase, PhaseState::Unlocked(_));
                if unlocked {
                    self.keep_peer_tags(vec![(peer_id, tag)]);
                } else {
                    self.lock().lan.tags.push((peer_id, tag));
                }
            }
            LanEvent::PairRequest { name, platform, code } => {
                self.lock().lan.request = Some(PairRequestView { name, platform, code });
                self.changed();
            }
            LanEvent::PairWelcomed | LanEvent::PairEnded => {
                self.lock().lan.request = None;
                self.changed();
            }
        }
    }

    /// The tags the devices said they write under, into the vault (their rows in the list).
    fn keep_peer_tags(&self, tags: Vec<(Uuid, String)>) {
        {
            let mut st = self.lock();
            let Ok(session) = unlocked_mut(&mut st) else { return };
            let Some(LanLocal::Hub { peers, .. }) = session.data.sync_mut().and_then(|sync| sync.lan.as_mut()) else { return };
            let before = peers.clone();
            let mut changed = false;
            for (peer_id, tag) in tags {
                if let Some(peer) = peers.iter_mut().find(|peer| peer.peer_id == peer_id)
                    && peer.tag.as_deref() != Some(tag.as_str())
                {
                    peer.tag = Some(tag);
                    changed = true;
                }
            }
            if !changed {
                return;
            }
            if let Err(error) = self.save(&mut st, false, move |s| {
                if let Some(LanLocal::Hub { peers, .. }) = s.data.sync_mut().and_then(|sync| sync.lan.as_mut()) {
                    *peers = before;
                }
            }) {
                tracing::warn!(?error, "the devices' tags were not kept");
            }
        }
        self.changed();
        self.serve_lan();
    }

    /// The vault was unlocked: the tags kept meanwhile go in, and the hub serves.
    pub(super) fn lan_unlocked(&self) {
        let tags = std::mem::take(&mut self.lock().lan.tags);
        if !tags.is_empty() {
            self.keep_peer_tags(tags);
        }
        self.serve_lan();
    }

    /// The vault locks: no offer stands and no request waits; the server goes on with the devices
    /// it has.
    pub(super) fn lan_locking(st: &mut State, service: Option<&dyn LanService>) {
        st.lan.offer = None;
        st.lan.request = None;
        if let (Some(service), Some(served)) = (service, st.lan.served.as_mut()) {
            served.pairing = None;
            service.update(served.clone());
        }
    }

    /// The offer lapsed: the server takes no more handshakes under it.
    pub(super) fn lan_offer_lapsed(&self, now: Instant) {
        let lapsed = {
            let mut st = self.lock();
            let lapsed = st.lan.offer.as_ref().is_some_and(|(_, until, _)| now >= *until);
            if lapsed {
                st.lan.offer = None;
            }
            lapsed
        };
        if lapsed {
            self.changed();
            self.serve_lan();
        }
    }

    /// Make this computer the space's LAN hub. Without a space yet, a space on the LAN alone is
    /// made under this vault's master password (`password`, required then; the sync key is shown
    /// in Settings › Sync until saved). With one, the user proves to be here (the password or the
    /// biometric check).
    pub async fn sync_lan_enable(&self, password: Option<Zeroizing<String>>, reason: Option<String>, device: String) -> CoreResult<()> {
        self.lan_service()?;
        let has_space = {
            let st = self.lock();
            match unlocked(&st)?.data.sync() {
                Some(sync) if sync.lan.is_some() => return Err(ErrorCode::SyncAlreadyOn.into()),
                found => found.is_some(),
            }
        };
        let hub = LanLocal::Hub {
            install_id: self.shared.install_id,
            hub_id: Uuid::new_v4(),
            port: LAN_PORT,
            peers: Vec::new(),
            removed: Vec::new(),
            sync: TransportLocal::default(),
        };
        if has_space {
            self.confirm_presence(password, reason).await?;
            let mut st = self.lock();
            let sync = unlocked_mut(&mut st)?.data.sync_mut().ok_or(ErrorCode::SyncOff)?;
            if sync.lan.is_some() {
                return Err(ErrorCode::SyncAlreadyOn.into());
            }
            sync.lan = Some(hub);
            self.save(&mut st, false, |s| {
                if let Some(sync) = s.data.sync_mut() {
                    sync.lan = None;
                }
            })?;
            sync_due(&mut st, Instant::now(), true);
        } else {
            let password = password.ok_or(ErrorCode::WrongPassword)?;
            let (sealed, vault_id) = {
                let st = self.lock();
                let session = unlocked(&st)?;
                (session.sealed.clone(), session.sealed.vault_id())
            };
            let cost = self.shared.config.kdf;
            let (keys, sync_key, keyring) = blocking(move || {
                sealed.verify_password(password.as_bytes())?;
                let sync_key = SyncKey::generate().map_err(|e| sync_error(&e))?;
                let keys = SpaceKeys::generate(sync_key.space_id()).map_err(|e| sync_error(&e))?;
                let keyring = seal_keyring(&keys, &sync_key, password.as_bytes(), cost).map_err(|e| sync_error(&e))?;
                Ok((keys, sync_key, keyring))
            })
            .await?;
            let mut space = SyncLocal::new(None, &keys, &sync_key, device_name(&device, self.shared.config.platform), &keyring);
            space.key_saved = false;
            space.lan = Some(hub);
            self.join_space(vault_id, space, None)?;
        }
        self.changed();
        self.serve_lan();
        Ok(())
    }

    /// An offer for a device to pair with this hub, once the user proved to be here: its text, as
    /// a QR code too, good for [`LAN_OFFER_TIME`] and one handshake. A secret while it stands.
    pub async fn sync_lan_offer(&self, password: Option<Zeroizing<String>>, reason: Option<String>) -> CoreResult<LanOffer> {
        let service = self.lan_service()?;
        {
            let st = self.lock();
            let sync = unlocked(&st)?.data.sync().ok_or(ErrorCode::SyncOff)?;
            if !matches!(sync.lan, Some(LanLocal::Hub { .. })) {
                return Err(ErrorCode::SyncOff.into());
            }
            if !st.lan.serving {
                return Err(ErrorCode::SyncLanUnavailable.into());
            }
        }
        self.confirm_presence(password, reason).await?;
        let (key, _) = new_lan_key()?;
        let until = Instant::now() + LAN_OFFER_TIME;
        let expires_at_ms = self.now_ms() + u64::try_from(LAN_OFFER_TIME.as_millis()).unwrap_or(u64::MAX);
        let offer = {
            let mut st = self.lock();
            let session = unlocked(&st)?;
            let sync = session.data.sync().ok_or(ErrorCode::SyncOff)?;
            let Some(LanLocal::Hub { hub_id, port, .. }) = &sync.lan else { return Err(ErrorCode::SyncOff.into()) };
            let port = st.lan.served.as_ref().map_or(*port, |served| served.port);
            let offer = PairOffer {
                hub_id: *hub_id,
                hub_name: sync.device_name.clone(),
                space_id: sync.space_id,
                addrs: service.addresses(),
                port,
                key: key.clone(),
                expires_at_ms,
            };
            st.lan.offer = Some((key, until, expires_at_ms));
            offer
        };
        self.changed();
        self.serve_lan();
        let text = offer.to_text();
        let svg = qr::svg(&text).map_err(|_| ErrorCode::Internal)?;
        Ok(LanOffer { text: text.to_string(), svg: svg.to_string(), expires_at_ms })
    }

    /// The user's answer to the device asking to pair. Yes: the device gets its own key with the
    /// hub and what it needs of the space (the welcome); the offer is spent.
    pub fn sync_lan_answer(&self, approve: bool) -> CoreResult<()> {
        let service = self.lan_service()?;
        let request = self.lock().lan.request.take().ok_or(ErrorCode::SyncPairingExpired)?;
        if !approve {
            service.answer(None);
            self.changed();
            return Ok(());
        }
        let (key, psk) = new_lan_key()?;
        let peer_id = Uuid::new_v4();
        let addrs = service.addresses();
        let answered = {
            let mut st = self.lock();
            let offer_port = st.lan.served.as_ref().map(|served| served.port);
            let session = unlocked_mut(&mut st)?;
            let sync = session.data.sync_mut().ok_or(ErrorCode::SyncOff)?;
            let welcome = {
                let Some(LanLocal::Hub { hub_id, port, peers, .. }) = &sync.lan else { return Err(ErrorCode::SyncOff.into()) };
                if peers.len() >= MAX_LAN_PEERS {
                    None
                } else {
                    Some(LanWelcome {
                        peer_id,
                        key: psk.clone(),
                        hub_id: *hub_id,
                        hub_name: sync.device_name.clone(),
                        port: offer_port.unwrap_or(*port),
                        addrs,
                        space_id: sync.space_id,
                        data_key: sync.data_key.clone(),
                        sync_key: sync.sync_key.clone(),
                        cloud: sync.cloud.as_ref().map(|cloud| cloud.storage.clone()),
                    })
                }
            };
            match welcome {
                None => None,
                Some(welcome) => {
                    if let Some(LanLocal::Hub { peers, .. }) = sync.lan.as_mut() {
                        let name: String = request.name.chars().take(MAX_DEVICE_NAME_CHARS).collect();
                        peers.push(LanPeer { peer_id, psk, tag: None, name, platform: request.platform.clone() });
                    }
                    self.save(&mut st, false, move |s| {
                        if let Some(LanLocal::Hub { peers, .. }) = s.data.sync_mut().and_then(|sync| sync.lan.as_mut()) {
                            peers.retain(|peer| peer.peer_id != peer_id);
                        }
                    })?;
                    st.lan.offer = None;
                    Some(welcome.to_bytes())
                }
            }
        };
        drop(key);
        let Some(welcome) = answered else {
            service.answer(None);
            self.changed();
            return Err(ErrorCode::SyncLanFull.into());
        };
        // The new device's key first: it connects with it as soon as the welcome arrives.
        self.serve_lan();
        service.answer(Some(welcome));
        self.changed();
        Ok(())
    }

    /// Take a device off this hub: its key no longer opens the hub (kept a while, to tell it so),
    /// and its snapshot leaves the hub's copy. As on any storage, what it holds of the space is
    /// not taken back.
    pub async fn sync_lan_remove_peer(&self, peer_id: Uuid) -> CoreResult<()> {
        let (tag, space_id, keys, device) = {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let device = session.data.device();
            let sync = session.data.sync_mut().ok_or(ErrorCode::SyncOff)?;
            let previous = sync.clone();
            let (space_id, keys) = (sync.space_id, sync.keys().map_err(|e| sync_error(&e))?);
            let Some(LanLocal::Hub { peers, removed, .. }) = sync.lan.as_mut() else { return Err(ErrorCode::SyncOff.into()) };
            let at = peers.iter().position(|peer| peer.peer_id == peer_id).ok_or(ErrorCode::Internal)?;
            let peer = peers.remove(at);
            removed.push(peer.psk);
            let excess = removed.len().saturating_sub(MAX_REMOVED_KEYS);
            removed.drain(..excess);
            if let Some(tag) = &peer.tag
                && let Some(transport) = sync.transport_mut(Store::Lan)
            {
                transport.state.seen.remove(tag);
            }
            self.save(&mut st, false, move |s| s.data.local_mut().sync = Some(previous))?;
            (peer.tag, space_id, keys, device)
        };
        self.serve_lan();
        if let Some(tag) = tag
            && let Some(key) = self.lock_key(Store::Lan)
        {
            let remote = self.space_storage(Store::Lan, space_id, &key).map_err(|e| sync_error(&e))?;
            let space = Space { prefix: key.prefix(), keys: &keys, device, device_name: "", keyring: &[], seq_floor: 0 };
            match remove_device(&*remote, &space, &mut SyncState::default(), &tag).await {
                Ok(()) | Err(SyncError::Misplaced) => {}
                Err(error) => return Err(sync_error(&error)),
            }
        }
        self.changed();
        Ok(())
    }

    /// The unlocked space's key for `store`.
    fn lock_key(&self, store: Store) -> Option<StoreKey> {
        let st = self.lock();
        unlocked(&st).ok()?.data.sync()?.store_key(store)
    }

    /// Stop syncing over the LAN on this device (the hub stops serving). A space with no storage
    /// of the user's own goes with it, as when sync is turned off.
    pub fn sync_lan_disable(&self) -> CoreResult<()> {
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let sync = session.data.sync().ok_or(ErrorCode::SyncOff)?;
            if sync.lan.is_none() {
                return Err(ErrorCode::SyncOff.into());
            }
            let previous = sync.clone();
            if sync.cloud.is_none() {
                session.data.local_mut().sync = None;
                self.save(&mut st, false, move |s| s.data.local_mut().sync = Some(previous))?;
                st.sync.reset();
            } else {
                if let Some(sync) = session.data.sync_mut() {
                    sync.lan = None;
                }
                self.save(&mut st, false, move |s| s.data.local_mut().sync = Some(previous))?;
                st.sync.lan = StoreRuntime::default();
            }
        }
        self.serve_lan();
        self.changed();
        Ok(())
    }

    /// Pair this device with the hub whose offer is `text`. `password` is this device's master
    /// password: unlocked, it must be the vault's (and the vault's accounts join the space); with
    /// no vault yet, a new vault is made under it. The space opens from the welcome, so no other
    /// device's password is asked; this device's keyring goes in under `password`. A device of the
    /// space already gets the LAN beside its storage. While the user at the hub decides, the
    /// state shows the code to compare (`sync.joining`).
    pub async fn sync_lan_join(&self, text: Zeroizing<String>, password: Zeroizing<String>, device: String) -> CoreResult<()> {
        let service = self.lan_service()?;
        let offer = PairOffer::from_text(&text, self.now_ms()).map_err(pairing_error)?;
        let (existing, in_space) = {
            let st = self.lock();
            match &st.phase {
                PhaseState::Locked => return Err(ErrorCode::Locked.into()),
                PhaseState::NoVault => {
                    check_password(&password)?;
                    (None, false)
                }
                PhaseState::Unlocked(session) => match session.data.sync() {
                    Some(sync) if sync.space_id != offer.space_id => return Err(ErrorCode::SyncOtherSpace.into()),
                    Some(sync) if sync.lan.is_some() => return Err(ErrorCode::SyncAlreadyOn.into()),
                    found => (Some(session.sealed.clone()), found.is_some()),
                },
            }
        };
        if let Some(sealed) = existing.clone() {
            let checked = password.clone();
            blocking(move || Ok(sealed.verify_password(checked.as_bytes())?)).await?;
        }
        let name = device_name(&device, self.shared.config.platform);
        let joining = service.join(&offer, &name, platform_name(self.shared.config.platform)).await.map_err(|e| sync_error(&e))?;
        self.lock().lan.joining = Some(LanJoiningView { hub_name: offer.hub_name.clone(), code: joining.code() });
        self.changed();
        let answered = joining.answer().await;
        self.lock().lan.joining = None;
        self.changed();
        let bytes = answered.map_err(|e| sync_error(&e))?.ok_or(ErrorCode::SyncPairingRefused)?;
        let welcome = LanWelcome::from_bytes(&bytes).map_err(|_| ErrorCode::SyncPairingInvalid)?;
        if welcome.space_id != offer.space_id {
            return Err(ErrorCode::SyncOtherSpace.into());
        }
        let role = LanLocal::Client {
            install_id: self.shared.install_id,
            hub_id: welcome.hub_id,
            hub_name: welcome.hub_name.clone(),
            peer_id: welcome.peer_id,
            psk: welcome.key.clone(),
            port: welcome.port,
            addrs: welcome.addrs.iter().map(IpAddr::to_string).collect(),
            sync: TransportLocal::default(),
        };
        // A storage the welcome names but this version cannot use stays out: the LAN is enough.
        let cloud = welcome.cloud.clone().filter(|storage| storage.validate().is_ok());
        if in_space {
            let mut st = self.lock();
            let sync = unlocked_mut(&mut st)?.data.sync_mut().filter(|sync| sync.space_id == welcome.space_id).ok_or(ErrorCode::SyncOff)?;
            let previous = sync.clone();
            sync.lan = Some(role);
            if sync.cloud.is_none() {
                sync.cloud = cloud.map(|storage| CloudLocal { storage, sync: TransportLocal::default() });
            }
            self.save(&mut st, false, move |s| s.data.local_mut().sync = Some(previous))?;
            sync_due(&mut st, Instant::now(), true);
            drop(st);
            self.changed();
            return Ok(());
        }
        let (keys, sync_key) = (welcome.keys().map_err(|_| ErrorCode::SyncPairingInvalid)?, welcome.sync_key().map_err(|_| ErrorCode::SyncPairingInvalid)?);
        let (cost, now, data_dir) = (self.shared.config.kdf, self.now_ms(), self.shared.config.data_dir.clone());
        let create = existing.is_none();
        let (keys, sealed, sync_key, keyring) = blocking(move || {
            let keyring = seal_keyring(&keys, &sync_key, password.as_bytes(), cost).map_err(|e| sync_error(&e))?;
            let sealed = if create {
                let sealed = Sealed::create(password.as_bytes(), cost, now)?;
                fs::create_dir_all(&data_dir)?;
                restrict_dir(&data_dir);
                Some(sealed)
            } else {
                None
            };
            Ok((keys, sealed, sync_key, keyring))
        })
        .await?;
        let mut space = SyncLocal::new(cloud, &keys, &sync_key, name, &keyring);
        space.lan = Some(role);
        match (existing.as_ref().map(Sealed::vault_id), sealed) {
            (Some(vault_id), _) => self.join_space(vault_id, space, None),
            (None, Some(sealed)) => self.create_joined(sealed, space, None),
            (None, None) => Err(ErrorCode::Internal.into()),
        }
    }

    /// Give the space a storage of the user's own when it has none (it syncs over the LAN alone),
    /// once the user proved to be here: settings typed in, or another device's invitation of this
    /// space. An empty place is the space's from its first write; a place that holds objects must
    /// hold this space.
    pub async fn sync_add_storage(&self, source: StorageSource, password: Option<Zeroizing<String>>, reason: Option<String>) -> CoreResult<()> {
        let (space_id, keys, device) = {
            let st = self.lock();
            let session = unlocked(&st)?;
            let sync = session.data.sync().ok_or(ErrorCode::SyncOff)?;
            if sync.cloud.is_some() {
                return Err(ErrorCode::SyncAlreadyOn.into());
            }
            (sync.space_id, sync.keys().map_err(|e| sync_error(&e))?, session.data.device())
        };
        let storage = match source {
            StorageSource::Storage { storage } => storage,
            StorageSource::Invite { text, code } => {
                let invite = blocking(move || Invite::from_any_text(&text, code.as_ref().map(|c| c.as_str())).map_err(|e| sync_error(&e))).await?;
                if invite.sync_key.space_id() != space_id {
                    return Err(ErrorCode::SyncOtherSpace.into());
                }
                invite.storage
            }
        };
        storage.validate().map_err(config_error)?;
        self.confirm_presence(password, reason).await?;
        let remote = self.open_storage(&storage)?;
        match find_space(&*remote, storage.prefix(), &keys, device).await {
            Ok(()) | Err(SyncError::NoSpace) => {}
            Err(error) => return Err(sync_error(&error)),
        }
        {
            let mut st = self.lock();
            let sync = unlocked_mut(&mut st)?.data.sync_mut().filter(|sync| sync.space_id == space_id).ok_or(ErrorCode::SyncOff)?;
            if sync.cloud.is_some() {
                return Err(ErrorCode::SyncAlreadyOn.into());
            }
            sync.cloud = Some(CloudLocal { storage: storage.clone(), sync: TransportLocal::default() });
            self.save(&mut st, false, |s| {
                if let Some(sync) = s.data.sync_mut() {
                    sync.cloud = None;
                }
            })?;
            let cloud = &mut st.sync.cloud;
            cloud.remote = Some((space_id, StoreKey::Cloud(storage), remote));
            cloud.held = false;
            cloud.at = Some(Instant::now());
        }
        self.changed();
        Ok(())
    }

    /// Take the storage of the user's own off this device's space, which goes on over the LAN; a
    /// space without the LAN is turned off here instead.
    pub fn sync_remove_storage(&self) -> CoreResult<()> {
        let lan = {
            let st = self.lock();
            let sync = unlocked(&st)?.data.sync().ok_or(ErrorCode::SyncOff)?;
            if sync.cloud.is_none() {
                return Err(ErrorCode::SyncNoStorage.into());
            }
            sync.lan.is_some()
        };
        if !lan {
            return self.sync_disable();
        }
        {
            let mut st = self.lock();
            let sync = unlocked_mut(&mut st)?.data.sync_mut().ok_or(ErrorCode::SyncOff)?;
            let previous = sync.cloud.take();
            self.save(&mut st, false, move |s| {
                if let Some(sync) = s.data.sync_mut() {
                    sync.cloud = previous;
                }
            })?;
            st.sync.cloud = StoreRuntime::default();
        }
        self.changed();
        Ok(())
    }
}
