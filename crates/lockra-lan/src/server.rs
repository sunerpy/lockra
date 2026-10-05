//! The hub's server: TCP for the connections and UDP, on the same port, for the probes of devices
//! looking for it, answering this computer and the local network only. A paired device lists and
//! reads the space's devices directory and writes its own object only, under the tag it registered
//! (one tag per device, none another device or the hub has). A device the hub removed is told so
//! with the key it had. A pairing request waits for the user at the hub, who compares the check
//! code; the offer's key opens one handshake only.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use lockra_sync::{RemoteStore, devices_dir};
use parking_lot::Mutex;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::timeout;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::channel::Channel;
use crate::proto::{Answer, Failure, Object, Request};
use crate::wire::{Kind, PREAMBLE_LEN, Preamble};
use crate::{FolderStore, HANDSHAKE_TIMEOUT, IDLE_TIMEOUT, Key, MAX_CONNECTIONS, MAX_PEERS, PAIRING_TIMEOUT, local_address};

/// A device paired with the hub.
#[derive(Clone)]
pub struct HubPeer {
    pub peer_id: Uuid,
    pub key: Key,
    /// The tag it writes under, once it said.
    pub tag: Option<String>,
}

/// What the hub serves, and to whom.
#[derive(Clone)]
pub struct HubConfig {
    pub hub_id: Uuid,
    pub space_id: Uuid,
    /// The hub's own tag: no device writes under it.
    pub own_tag: String,
    /// At most [`MAX_PEERS`]; any beyond are not served.
    pub peers: Vec<HubPeer>,
    /// The keys of devices the hub removed: they are told so.
    pub removed: Vec<Key>,
    /// The key of the pairing offer that stands, if one does.
    pub pairing: Option<Key>,
}

/// What happened at the hub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubEvent {
    /// A device wrote or removed its object: the hub's own run can take it in.
    PeerWrote { peer_id: Uuid },
    /// A device writes under `tag` from now on (its first run, or once it became a new device).
    PeerTag { peer_id: Uuid, tag: String },
    /// A device asks to pair; the user compares `code` with the one it shows, then answers
    /// ([`HubServer::answer`]).
    PairRequest { name: String, platform: String, code: String },
    /// The welcome reached the device.
    PairWelcomed,
    /// The pairing request ended without a welcome: refused, unanswered in time, or the device
    /// left.
    PairEnded,
}

/// The user's answer to a pairing request: the welcome, or none.
type Welcome = Option<Zeroizing<Vec<u8>>>;

/// Who a connection's key belongs to.
enum Who {
    Peer(Uuid),
    Removed,
    Pairing,
}

struct Shared {
    store: FolderStore,
    config: Mutex<HubConfig>,
    events: mpsc::UnboundedSender<HubEvent>,
    connections: Arc<Semaphore>,
    handshakes: Arc<Semaphore>,
    /// Where the user's answer to the pairing request in progress goes.
    answer: Mutex<Option<oneshot::Sender<Welcome>>>,
}

/// A hub serving its folder.
pub struct HubServer {
    shared: Arc<Shared>,
    port: u16,
    tasks: Vec<JoinHandle<()>>,
}

impl HubServer {
    /// Serve `store` on `port` (0: a free one), on every IPv4 interface. Discovery answers on the
    /// same port when it is free for UDP too.
    pub async fn start(store: FolderStore, port: u16, config: HubConfig, events: mpsc::UnboundedSender<HubEvent>) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared {
            store,
            config: Mutex::new(config),
            events,
            connections: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
            handshakes: Arc::new(Semaphore::new(MAX_CONNECTIONS * 4)),
            answer: Mutex::new(None),
        });
        let mut tasks = vec![tokio::spawn(accept(listener, Arc::clone(&shared)))];
        match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).await {
            Ok(socket) => tasks.push(tokio::spawn(answer_probes(socket, Arc::clone(&shared)))),
            Err(error) => tracing::warn!(%error, port, "no discovery: devices reach the hub at the addresses they know"),
        }
        Ok(Self { shared, port, tasks })
    }

    /// The port it listens on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// New devices, keys or offer; connections already open keep their device.
    pub fn update(&self, config: HubConfig) {
        *self.shared.config.lock() = config;
    }

    /// The user's answer to the pairing request: the welcome to send (from the core: the space's
    /// keys, the device's own key), or none to refuse.
    pub fn answer(&self, welcome: Welcome) {
        if let Some(waiting) = self.shared.answer.lock().take() {
            let _ = waiting.send(welcome);
        }
    }

    /// Stop serving; the connections in progress end.
    pub fn stop(self) {}
}

impl Drop for HubServer {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

async fn accept(listener: TcpListener, shared: Arc<Shared>) {
    // Dropped with this task when the hub stops: the connections in progress end with it.
    let mut connections = JoinSet::new();
    loop {
        while connections.try_join_next().is_some() {}
        let (stream, from) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::debug!(%error, "a connection was not accepted");
                continue;
            }
        };
        if !local_address(from.ip()) {
            continue;
        }
        let Ok(handshaking) = Arc::clone(&shared.handshakes).try_acquire_owned() else { continue };
        let shared = Arc::clone(&shared);
        connections.spawn(async move {
            let opened = timeout(HANDSHAKE_TIMEOUT, open(stream, &shared)).await;
            drop(handshaking);
            if let Ok(Ok((channel, kind, who))) = opened {
                shared.serve(channel, kind, who).await;
            }
        });
    }
}

async fn open(stream: TcpStream, shared: &Shared) -> Result<(Channel<TcpStream>, Kind, Who), crate::channel::ChannelError> {
    let hub_id = shared.config.lock().hub_id;
    Channel::accept(stream, hub_id, |preamble| shared.choose(preamble)).await
}

impl Shared {
    /// The key a preamble's hint was made with, and whose it is. The offer's key is taken: one
    /// handshake only.
    fn choose(&self, preamble: &Preamble) -> Option<(Key, Who)> {
        let mut config = self.config.lock();
        match preamble.kind {
            Kind::Session => config
                .peers
                .iter()
                .take(MAX_PEERS)
                .find(|peer| preamble.made_with(&peer.key))
                .map(|peer| (peer.key.clone(), Who::Peer(peer.peer_id)))
                .or_else(|| config.removed.iter().find(|key| preamble.made_with(key)).map(|key| (key.clone(), Who::Removed))),
            Kind::Pairing => {
                if !config.pairing.as_ref().is_some_and(|key| preamble.made_with(key)) {
                    return None;
                }
                config.pairing.take().map(|key| (key, Who::Pairing))
            }
        }
    }

    async fn serve(&self, mut channel: Channel<TcpStream>, kind: Kind, who: Who) {
        match (kind, who) {
            (Kind::Session, Who::Peer(peer_id)) => match Arc::clone(&self.connections).try_acquire_owned() {
                Ok(_serving) => self.session(&mut channel, peer_id).await,
                Err(_) => refuse(&mut channel, Failure::Busy).await,
            },
            (Kind::Session, Who::Removed) => refuse(&mut channel, Failure::Removed).await,
            (Kind::Pairing, Who::Pairing) => self.pairing(&mut channel).await,
            _ => {}
        }
    }

    async fn session(&self, channel: &mut Channel<TcpStream>, peer_id: Uuid) {
        loop {
            let Ok(Ok((header, body))) = timeout(IDLE_TIMEOUT, channel.recv()).await else { return };
            // Removed since the connection opened: the request is not done, the device is told.
            if !self.still_paired(peer_id) {
                let _ = send(channel, &Answer::Failed { error: Failure::Removed }, &[]).await;
                return;
            }
            let (answer, out) = match serde_json::from_slice::<Request>(&header) {
                Ok(request) => self.handle(peer_id, request, body).await,
                Err(_) => (Answer::Failed { error: Failure::Unsupported }, Zeroizing::new(Vec::new())),
            };
            if send(channel, &answer, &out).await.is_err() {
                return;
            }
        }
    }

    fn still_paired(&self, peer_id: Uuid) -> bool {
        self.config.lock().peers.iter().take(MAX_PEERS).any(|peer| peer.peer_id == peer_id)
    }

    /// One request of a paired device.
    async fn handle(&self, peer_id: Uuid, request: Request, body: Zeroizing<Vec<u8>>) -> (Answer, Zeroizing<Vec<u8>>) {
        let none = || Zeroizing::new(Vec::new());
        let failed = |error| (Answer::Failed { error }, Zeroizing::new(Vec::new()));
        let (dir, own_tag, bound) = {
            let config = self.config.lock();
            let bound = config.peers.iter().find(|peer| peer.peer_id == peer_id).and_then(|peer| peer.tag.clone());
            (devices_dir("", config.space_id), config.own_tag.clone(), bound)
        };
        match request {
            Request::List { dir: asked } if asked == dir => match self.store.list(&dir).await {
                Ok(listing) => (Answer::Listed { objects: listing.into_iter().map(Object::from).collect() }, none()),
                Err(error) => failed(Failure::of(&error)),
            },
            Request::Get { path } if tag_in(&dir, &path).is_some() => match self.store.get(&path).await {
                Ok(Some((bytes, etag))) => (Answer::Got { found: true, etag }, Zeroizing::new(bytes)),
                Ok(None) => (Answer::Got { found: false, etag: None }, none()),
                Err(error) => failed(Failure::of(&error)),
            },
            Request::Put { path, condition } if bound.is_some() && tag_in(&dir, &path) == bound.as_deref() => {
                match self.store.put(&path, body.to_vec(), condition.into()).await {
                    Ok(etag) => {
                        let _ = self.events.send(HubEvent::PeerWrote { peer_id });
                        (Answer::Put { etag }, none())
                    }
                    Err(error) => failed(Failure::of(&error)),
                }
            }
            Request::Delete { path } if bound.is_some() && tag_in(&dir, &path) == bound.as_deref() => match self.store.delete(&path).await {
                Ok(()) => {
                    let _ = self.events.send(HubEvent::PeerWrote { peer_id });
                    (Answer::Done, none())
                }
                Err(error) => failed(Failure::of(&error)),
            },
            Request::Register { tag } if is_tag(&tag) && tag != own_tag => {
                let mut config = self.config.lock();
                if config.peers.iter().any(|peer| peer.peer_id != peer_id && peer.tag.as_deref() == Some(tag.as_str())) {
                    return failed(Failure::Denied);
                }
                let Some(peer) = config.peers.iter_mut().find(|peer| peer.peer_id == peer_id) else { return failed(Failure::Removed) };
                if peer.tag.as_deref() != Some(tag.as_str()) {
                    peer.tag = Some(tag.clone());
                    let _ = self.events.send(HubEvent::PeerTag { peer_id, tag });
                }
                (Answer::Done, none())
            }
            _ => failed(Failure::Denied),
        }
    }

    async fn pairing(&self, channel: &mut Channel<TcpStream>) {
        let request = timeout(IDLE_TIMEOUT, channel.recv()).await;
        let Ok(Ok((header, _))) = request else {
            let _ = self.events.send(HubEvent::PairEnded);
            return;
        };
        let Ok(Request::Join { name, platform }) = serde_json::from_slice::<Request>(&header) else {
            let _ = self.events.send(HubEvent::PairEnded);
            return;
        };
        let (answered, answer) = oneshot::channel();
        *self.answer.lock() = Some(answered);
        let name: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
        let platform: String = platform.chars().filter(char::is_ascii_alphanumeric).take(16).collect();
        let _ = self.events.send(HubEvent::PairRequest { name, platform, code: channel.check_code() });
        match timeout(PAIRING_TIMEOUT, answer).await {
            Ok(Ok(Some(welcome))) => {
                let sent = send(channel, &Answer::Welcome, &welcome).await;
                let _ = self.events.send(if sent.is_ok() { HubEvent::PairWelcomed } else { HubEvent::PairEnded });
            }
            _ => {
                self.answer.lock().take();
                let _ = send(channel, &Answer::Refused, &[]).await;
                let _ = self.events.send(HubEvent::PairEnded);
            }
        }
    }
}

/// The tag of an object directly in `dir`.
fn tag_in<'a>(dir: &str, path: &'a str) -> Option<&'a str> {
    path.strip_prefix(dir)?.strip_suffix(".lks").filter(|tag| is_tag(tag))
}

/// A device tag: lowercase hex, as the space makes them.
fn is_tag(tag: &str) -> bool {
    (8..=64).contains(&tag.len()) && tag.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

async fn send(channel: &mut Channel<TcpStream>, answer: &Answer, body: &[u8]) -> Result<(), crate::channel::ChannelError> {
    // Serializing plain data to JSON cannot fail.
    #[allow(clippy::expect_used)]
    let header = serde_json::to_vec(answer).expect("an answer serializes");
    channel.send(&header, body).await
}

/// Answer the device's next request with `failure`, and end.
async fn refuse(channel: &mut Channel<TcpStream>, failure: Failure) {
    if let Ok(Ok(_)) = timeout(IDLE_TIMEOUT, channel.recv()).await {
        let _ = send(channel, &Answer::Failed { error: failure }, &[]).await;
    }
}

/// Answer the probes made with a paired device's key, or the offer's; ignore the others.
async fn answer_probes(socket: UdpSocket, shared: Arc<Shared>) {
    let mut buffer = [0u8; 64];
    loop {
        let Ok((length, from)) = socket.recv_from(&mut buffer).await else { continue };
        let Some(answer) = probe_answer(&shared, &buffer[..length], from) else { continue };
        let _ = socket.send_to(&answer, from).await;
    }
}

fn probe_answer(shared: &Shared, bytes: &[u8], from: SocketAddr) -> Option<[u8; crate::wire::ANSWER_LEN]> {
    if !local_address(from.ip()) {
        return None;
    }
    let opening: &[u8; PREAMBLE_LEN] = bytes.try_into().ok()?;
    let probe = Preamble::parse(opening)?;
    let config = shared.config.lock();
    let key = match probe.kind {
        Kind::Session => config.peers.iter().take(MAX_PEERS).map(|peer| &peer.key).find(|key| probe.made_with(key)),
        Kind::Pairing => config.pairing.as_ref().filter(|key| probe.made_with(key)),
    }?;
    Some(probe.answer(key))
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::*;
    use crate::discover::discover;

    #[test]
    fn a_tag_is_lowercase_hex_and_an_object_lies_directly_in_the_spaces_directory() {
        let dir = "lockra-sync-v1/x/devices/";
        assert_eq!(tag_in(dir, "lockra-sync-v1/x/devices/0123abcd.lks"), Some("0123abcd"));
        for path in [
            "lockra-sync-v1/x/devices/0123ABCD.lks",
            "lockra-sync-v1/x/devices/sub/0123abcd.lks",
            "lockra-sync-v1/y/devices/0123abcd.lks",
            "lockra-sync-v1/x/devices/0123abcd.txt",
            "lockra-sync-v1/x/devices/.lks",
            "lockra-sync-v1/x/devices/abc.lks",
        ] {
            assert_eq!(tag_in(dir, path), None, "{path}");
        }
        assert!(is_tag(&"a".repeat(64)) && !is_tag(&"a".repeat(65)) && !is_tag("abcdefg1"));
    }

    #[tokio::test]
    async fn a_probe_is_answered_for_a_paired_device_and_the_offer_only() {
        let folder = tempfile::tempdir().unwrap();
        let config = HubConfig {
            hub_id: Uuid::from_u128(1),
            space_id: Uuid::from_u128(2),
            own_tag: "00".repeat(8),
            peers: vec![HubPeer { peer_id: Uuid::from_u128(3), key: Zeroizing::new([5; 32]), tag: None }],
            removed: vec![Zeroizing::new([6; 32])],
            pairing: Some(Zeroizing::new([7; 32])),
        };
        let (sender, _events) = mpsc::unbounded_channel();
        let hub = HubServer::start(FolderStore::new(folder.path()), 0, config, sender).await.unwrap();
        let (port, here) = (hub.port(), [IpAddr::V4(Ipv4Addr::LOCALHOST)]);
        let probe = |kind, byte| async move { discover(kind, &Zeroizing::new([byte; 32]), port, &here, false).await };
        let (paired, offer, removed, stranger, offer_as_session, peer_as_pairing) = tokio::join!(
            probe(Kind::Session, 5),
            probe(Kind::Pairing, 7),
            probe(Kind::Session, 6),
            probe(Kind::Session, 9),
            probe(Kind::Session, 7),
            probe(Kind::Pairing, 5)
        );
        assert_eq!((paired, offer), (Some(here[0]), Some(here[0])));
        assert_eq!((removed, stranger, offer_as_session, peer_as_pairing), (None, None, None, None));
    }
}
