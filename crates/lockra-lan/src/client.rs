//! A paired device's way to its hub, as a sync storage: one connection kept between runs (the hub
//! ends it after a while without requests; the next request opens another). The hub is tried at
//! the address it answered at last, then at the ones the device knows, then found by a probe.
//! Before it writes, the device registers the tag it writes under on that connection.

use std::net::IpAddr;
use std::time::Duration;

use lockra_sync::{ObjectMeta, PutCondition, RemoteFuture, RemoteStore, SyncError};
use parking_lot::Mutex;
use tokio::net::TcpStream;
use tokio::time::timeout;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::channel::{Channel, ChannelError};
use crate::discover::discover;
use crate::pair::PairOffer;
use crate::proto::{Answer, Failure, Request};
use crate::wire::Kind;
use crate::{HANDSHAKE_TIMEOUT, IDLE_TIMEOUT, Key, PAIRING_TIMEOUT};

/// How long a connection to one address may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Where a device finds its hub, and its key there.
#[derive(Clone)]
pub struct ClientConfig {
    pub hub_id: Uuid,
    pub key: Key,
    pub port: u16,
    /// Where the hub was reached before.
    pub addrs: Vec<IpAddr>,
    /// Look for the hub by a broadcast probe when none of `addrs` answers.
    pub broadcast: bool,
}

struct Connection {
    channel: Channel<TcpStream>,
    /// The tag registered on it.
    tag: Option<String>,
}

/// The hub's copy of the space, for a paired device.
pub struct HubClient {
    config: ClientConfig,
    connection: tokio::sync::Mutex<Option<Connection>>,
    found: Mutex<Option<IpAddr>>,
}

fn lost(error: &ChannelError) -> SyncError {
    SyncError::Network(format!("the hub: {error}"))
}

async fn connect(ip: IpAddr, port: u16, kind: Kind, key: &Key, hub_id: Uuid) -> Option<Channel<TcpStream>> {
    let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect((ip, port))).await.ok()?.ok()?;
    let _ = stream.set_nodelay(true);
    timeout(HANDSHAKE_TIMEOUT, Channel::connect(stream, kind, key, hub_id)).await.ok()?.ok()
}

/// A hub at the first of `ips` that answers, else the one a probe finds: where, and the channel.
async fn reach(ips: &[IpAddr], port: u16, kind: Kind, key: &Key, hub_id: Uuid, broadcast: bool) -> Option<(IpAddr, Channel<TcpStream>)> {
    for ip in ips {
        if let Some(channel) = connect(*ip, port, kind, key, hub_id).await {
            return Some((*ip, channel));
        }
    }
    let ip = discover(kind, key, port, ips, broadcast).await?;
    connect(ip, port, kind, key, hub_id).await.map(|channel| (ip, channel))
}

async fn exchange(channel: &mut Channel<TcpStream>, request: &Request, body: &[u8]) -> Result<(Answer, Zeroizing<Vec<u8>>), ChannelError> {
    // Serializing plain data to JSON cannot fail.
    #[allow(clippy::expect_used)]
    let header = serde_json::to_vec(request).expect("a request serializes");
    channel.send(&header, body).await?;
    let (header, body) = timeout(IDLE_TIMEOUT, channel.recv()).await.map_err(|_| ChannelError::Io(std::io::ErrorKind::TimedOut.into()))??;
    let answer = serde_json::from_slice(&header).map_err(|_| ChannelError::Malformed)?;
    Ok((answer, body))
}

impl HubClient {
    pub fn new(config: ClientConfig) -> Self {
        Self { config, connection: tokio::sync::Mutex::new(None), found: Mutex::new(None) }
    }

    /// Where the hub answered last, to try first next time.
    pub fn found_at(&self) -> Option<IpAddr> {
        *self.found.lock()
    }

    async fn open(&self) -> Result<Channel<TcpStream>, SyncError> {
        let mut ips: Vec<IpAddr> = self.found_at().into_iter().collect();
        for ip in &self.config.addrs {
            if !ips.contains(ip) {
                ips.push(*ip);
            }
        }
        let (ip, channel) = reach(&ips, self.config.port, Kind::Session, &self.config.key, self.config.hub_id, self.config.broadcast)
            .await
            .ok_or_else(|| SyncError::Network("the hub was not found".into()))?;
        *self.found.lock() = Some(ip);
        Ok(channel)
    }

    /// Ask the hub, on the kept connection or a new one. `tag`: register it first, for a write.
    /// A kept connection the hub ended meanwhile is opened again, and a write it refused is tried
    /// again after registering anew (the hub may have restarted without the binding): once each.
    async fn call(&self, request: Request, body: &[u8], tag: Option<&str>) -> Result<(Answer, Zeroizing<Vec<u8>>), SyncError> {
        let mut kept = self.connection.lock().await;
        let (mut reopened, mut registered_again) = (false, false);
        loop {
            let fresh = kept.is_none();
            if fresh {
                *kept = Some(Connection { channel: self.open().await?, tag: None });
            }
            let Some(connection) = kept.as_mut() else { return Err(SyncError::Interrupted) };
            let registering = tag.is_some() && connection.tag.as_deref() != tag;
            let result = async {
                if let Some(tag) = tag.filter(|_| registering) {
                    let (answer, _) = exchange(&mut connection.channel, &Request::Register { tag: tag.to_owned() }, &[]).await?;
                    if answer != Answer::Done {
                        return Ok((answer, Zeroizing::new(Vec::new())));
                    }
                    connection.tag = Some(tag.to_owned());
                }
                exchange(&mut connection.channel, &request, body).await
            }
            .await;
            match result {
                Ok((Answer::Failed { error: Failure::Denied }, _)) if tag.is_some() && !registering && !registered_again => {
                    registered_again = true;
                    connection.tag = None;
                }
                Ok((Answer::Failed { error }, _)) => {
                    if matches!(error, Failure::Removed | Failure::Busy) {
                        *kept = None;
                    }
                    return Err(error.error());
                }
                Ok(answered) => return Ok(answered),
                Err(error) => {
                    *kept = None;
                    if fresh || reopened {
                        return Err(lost(&error));
                    }
                    reopened = true;
                }
            }
        }
    }
}

/// The tag of the object at `path`, which only it writes.
fn own_tag(path: &str) -> Option<&str> {
    path.rsplit('/').next()?.strip_suffix(".lks")
}

fn unexpected() -> SyncError {
    SyncError::Network("the hub answered something else".into())
}

impl RemoteStore for HubClient {
    fn conditional_puts(&self) -> bool {
        true
    }

    fn list<'a>(&'a self, dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>> {
        Box::pin(async move {
            match self.call(Request::List { dir: dir.to_owned() }, &[], None).await? {
                (Answer::Listed { objects }, _) => Ok(objects.into_iter().map(ObjectMeta::from).collect()),
                _ => Err(unexpected()),
            }
        })
    }

    fn get<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, Option<(Vec<u8>, Option<String>)>> {
        Box::pin(async move {
            match self.call(Request::Get { path: path.to_owned() }, &[], None).await? {
                (Answer::Got { found: true, etag }, body) => Ok(Some((body.to_vec(), etag))),
                (Answer::Got { found: false, .. }, _) => Ok(None),
                _ => Err(unexpected()),
            }
        })
    }

    fn put<'a>(&'a self, path: &'a str, bytes: Vec<u8>, condition: PutCondition) -> RemoteFuture<'a, Option<String>> {
        Box::pin(async move {
            let tag = own_tag(path).ok_or(SyncError::Denied)?;
            match self.call(Request::Put { path: path.to_owned(), condition: condition.into() }, &bytes, Some(tag)).await? {
                (Answer::Put { etag }, _) => Ok(etag),
                _ => Err(unexpected()),
            }
        })
    }

    fn delete<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, ()> {
        Box::pin(async move {
            let tag = own_tag(path).ok_or(SyncError::Denied)?;
            match self.call(Request::Delete { path: path.to_owned() }, &[], Some(tag)).await? {
                (Answer::Done, _) => Ok(()),
                _ => Err(unexpected()),
            }
        })
    }
}

/// A pairing under way: the check code to show, while the user at the hub compares it with
/// theirs.
pub struct Joining {
    channel: Channel<TcpStream>,
    /// The six digits both sides show.
    pub code: String,
    /// Where the hub answered.
    pub at: IpAddr,
}

/// Ask the hub of `offer` to pair, as `name` on `platform`.
pub async fn join(offer: &PairOffer, name: &str, platform: &str) -> Result<Joining, SyncError> {
    let (at, mut channel) = reach(&offer.addrs, offer.port, Kind::Pairing, &offer.key, offer.hub_id, true)
        .await
        .ok_or_else(|| SyncError::Network("the hub was not found, or its offer was taken".into()))?;
    // Serializing plain data to JSON cannot fail.
    #[allow(clippy::expect_used)]
    let header = serde_json::to_vec(&Request::Join { name: name.to_owned(), platform: platform.to_owned() }).expect("a request serializes");
    channel.send(&header, &[]).await.map_err(|e| lost(&e))?;
    let code = channel.check_code();
    Ok(Joining { channel, code, at })
}

/// What the hub answered a pairing request.
pub enum Joined {
    /// The user at the hub agreed: the welcome (the core reads it).
    Welcome(Zeroizing<Vec<u8>>),
    /// The user at the hub said no, or did not answer in time.
    Refused,
}

impl Joining {
    /// Wait for the user at the hub.
    pub async fn answer(mut self) -> Result<Joined, SyncError> {
        let waited = timeout(PAIRING_TIMEOUT + IDLE_TIMEOUT, self.channel.recv()).await;
        let (header, body) = waited.map_err(|_| SyncError::Network("the hub did not answer".into()))?.map_err(|e| lost(&e))?;
        match serde_json::from_slice::<Answer>(&header) {
            Ok(Answer::Welcome) => Ok(Joined::Welcome(body)),
            Ok(Answer::Refused) => Ok(Joined::Refused),
            _ => Err(unexpected()),
        }
    }
}
