//! The app's service loop. The shell owns the listener (a Unix socket, a named pipe) and runs
//! [`serve`] once the core has started; each connection from a host process runs its own state
//! machine: the plain `hello`, the pairing handshake and its wait for the user, a session's
//! handshake, and the session's requests. Every decision is the core's (`Core::browser_*`); this
//! side only carries messages, and keeps the Noise state in this process's memory.
//!
//! While the vault is locked only `hello` and `unlock_request` are answered. As it locks, every
//! session and pairing ends and its keys are dropped; the connections go back to before a
//! handshake and are told so.

use std::future::Future;
use std::io;
use std::time::Duration;

use lockra_core::ui::{Phase, UiEvent};
use lockra_core::{ClientInfo, Core, PAIR_TTL, Refusal, SessionId};
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};
use uuid::Uuid;

use crate::frame::{MAX_FRAME, Order, read_frame, write_frame};
use crate::noise::{Channel, Responder};
use crate::protocol::{Answer, AppPlain, AppState, ClientPlain, ENVELOPE_VERSION, Envelope, Kind, Op, Outgoing, PROTO_MAX, PROTO_MIN, Push, Request, Response};

/// Where the host processes connect.
pub trait Listener: Send + 'static {
    /// A connection.
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// The next connection. An error of kind [`io::ErrorKind::NotConnected`] means the listener is
    /// gone and ends [`serve`]; any other is retried after a moment.
    fn accept(&mut self) -> impl Future<Output = io::Result<Self::Stream>> + Send;
}

/// Serve the extensions on `listener` until `shutdown` completes: then no connection is accepted,
/// every connection ends (its session with it), and this returns once they have. The shell
/// removes the socket afterwards.
pub async fn serve<L: Listener>(mut listener: L, core: Core, shutdown: impl Future<Output = ()> + Send) {
    let (stop, stopped) = watch::channel(false);
    let mut connections = JoinSet::new();
    let mut shutdown = std::pin::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok(stream) => {
                    connections.spawn(connection(stream, core.clone(), stopped.clone()));
                }
                Err(error) if error.kind() == io::ErrorKind::NotConnected => break,
                Err(error) => {
                    tracing::warn!(?error, "the browser channel could not accept a connection");
                    // A pause before trying again, so that a persistent failure does not spin.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    let _ = stop.send(true);
    while connections.join_next().await.is_some() {}
}

/// Connections made in memory: for tests, the shell's included.
pub fn memory() -> (MemoryListener, MemoryConnector) {
    let (sender, incoming) = mpsc::channel(8);
    (MemoryListener { incoming }, MemoryConnector { sender })
}

/// The listening side of [`memory`].
#[derive(Debug)]
pub struct MemoryListener {
    incoming: mpsc::Receiver<DuplexStream>,
}

impl Listener for MemoryListener {
    type Stream = DuplexStream;

    async fn accept(&mut self) -> io::Result<DuplexStream> {
        self.incoming.recv().await.ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "the connector is gone"))
    }
}

/// The connecting side of [`memory`].
#[derive(Debug, Clone)]
pub struct MemoryConnector {
    sender: mpsc::Sender<DuplexStream>,
}

impl MemoryConnector {
    /// A new connection, the host's end of it.
    pub async fn connect(&self) -> io::Result<DuplexStream> {
        let (host, app) = tokio::io::duplex(2 * MAX_FRAME);
        self.sender.send(app).await.map_err(|_| io::Error::new(io::ErrorKind::NotConnected, "the listener is gone"))?;
        Ok(host)
    }
}

/// What a connection's own tasks hand back to it, for the session (or pairing) they began under.
enum Internal {
    /// A request's answer.
    Answer { generation: u64, response: Response },
    /// How the pairing request ended.
    Paired { generation: u64, outcome: Result<Uuid, Refusal> },
}

/// Where a connection stands.
enum Stage {
    /// Before any handshake.
    Fresh,
    /// The pairing handshake, its second message sent.
    Pairing(Box<Responder>),
    /// The pairing handshake done, the request waiting for the user.
    Waiting(Channel),
    /// A session.
    Session { channel: Channel, session: SessionId, pairing: Uuid },
}

/// The connection ends: the stream broke or the other side broke the protocol.
struct End;

struct Conn {
    core: Core,
    internal: mpsc::UnboundedSender<Internal>,
    client: Option<ClientInfo>,
    proto: Option<u32>,
    announced: Option<AppState>,
    stage: Stage,
    /// Goes up as a session or a pairing ends: what its tasks hand back after that is dropped.
    generation: u64,
    tasks: Vec<JoinHandle<()>>,
}

async fn connection<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(stream: S, core: Core, mut stop: watch::Receiver<bool>) {
    let (mut reader, mut writer) = tokio::io::split(stream);
    // Frames are read whole by a task of their own: a read cut off half-way would lose the stream.
    let (frames_in, mut frames) = mpsc::channel::<Option<Vec<u8>>>(8);
    let reading = tokio::spawn(async move {
        loop {
            let frame = read_frame(&mut reader, Order::Little).await.ok().flatten();
            let end = frame.is_none();
            if frames_in.send(frame).await.is_err() || end {
                break;
            }
        }
    });
    let mut events = core.subscribe();
    let (internal, mut answers) = mpsc::unbounded_channel();
    let mut conn = Conn { core: core.clone(), internal, client: None, proto: None, announced: None, stage: Stage::Fresh, generation: 0, tasks: Vec::new() };
    loop {
        let out = tokio::select! {
            _ = stop.changed() => break,
            frame = frames.recv() => match frame.flatten() {
                Some(bytes) => match conn.incoming(&bytes) {
                    Ok(out) => out,
                    Err(End) => break,
                },
                None => break,
            },
            event = events.recv() => match event {
                Ok(UiEvent::State { state }) => conn.state(state.phase),
                Ok(UiEvent::Notice { .. }) => Vec::new(),
                Err(broadcast::error::RecvError::Lagged(_)) => conn.state(core.state().phase),
                Err(broadcast::error::RecvError::Closed) => break,
            },
            Some(message) = answers.recv() => match conn.internal(message) {
                Ok(out) => out,
                Err(End) => break,
            },
        };
        let mut broken = false;
        for envelope in out {
            if write_frame(&mut writer, Order::Little, &envelope.frame()).await.is_err() {
                broken = true;
                break;
            }
        }
        if broken {
            break;
        }
    }
    conn.end_stage();
    reading.abort();
}

impl Conn {
    /// A frame from the host.
    fn incoming(&mut self, bytes: &[u8]) -> Result<Vec<Envelope>, End> {
        let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| End)?;
        if envelope.v != ENVELOPE_VERSION {
            return Err(End);
        }
        let data = envelope.bytes().ok_or(End)?;
        match envelope.t {
            Kind::Hello => Ok(self.plain(&data)),
            Kind::Hs => self.handshake(&data, envelope.p),
            Kind::Msg => self.message(&data),
        }
    }

    fn plain(&mut self, data: &[u8]) -> Vec<Envelope> {
        let Ok(message) = serde_json::from_slice::<ClientPlain>(data) else { return vec![error("bad_message")] };
        match message {
            ClientPlain::Hello { proto_min, proto_max, client } => {
                // A new hello starts the connection over.
                self.end_stage();
                self.client = Some(client.cleaned());
                self.proto = (proto_min.max(PROTO_MIN) <= proto_max.min(PROTO_MAX)).then(|| proto_max.min(PROTO_MAX));
                let ui = self.core.state();
                let state = app_state(ui.phase);
                self.announced = Some(state);
                vec![plain(&AppPlain::Hello { proto_min: PROTO_MIN, proto_max: PROTO_MAX, proto: self.proto, app_version: ui.app_version, state })]
            }
            ClientPlain::UnlockRequest => {
                let client = self.client.clone().unwrap_or_default();
                vec![plain(&AppPlain::Unlock { accepted: self.core.browser_unlock_request(&client) })]
            }
            // The host's alone: it never reaches the app from one.
            ClientPlain::StartApp { .. } => vec![error("bad_message")],
        }
    }

    fn handshake(&mut self, data: &[u8], pairing: Option<Uuid>) -> Result<Vec<Envelope>, End> {
        let Some(client) = self.client.clone() else { return Ok(vec![error("hello_first")]) };
        if self.proto.is_none() {
            return Ok(vec![error("version")]);
        }
        match std::mem::replace(&mut self.stage, Stage::Fresh) {
            Stage::Fresh => match pairing {
                None => self.pairing_start(data),
                Some(pairing) => self.session_start(data, pairing, client),
            },
            Stage::Pairing(responder) => self.pairing_finish(data, responder, client),
            // A handshake inside a session or a pairing request is not this protocol.
            stage @ (Stage::Waiting(_) | Stage::Session { .. }) => {
                self.stage = stage;
                Err(End)
            }
        }
    }

    /// The pairing handshake's first message: the second goes back with the app's static key.
    fn pairing_start(&mut self, data: &[u8]) -> Result<Vec<Envelope>, End> {
        let Some(key) = self.core.browser_static_secret() else { return Ok(vec![self.refusal()]) };
        let mut responder = Responder::pairing(&key).map_err(|_| End)?;
        responder.read(data).map_err(|_| End)?;
        let reply = responder.write(&[]).map_err(|_| End)?;
        self.stage = Stage::Pairing(Box::new(responder));
        Ok(vec![Envelope::new(Kind::Hs, &reply)])
    }

    /// The pairing handshake's last message: the request goes to the user, the code on both
    /// screens.
    fn pairing_finish(&mut self, data: &[u8], mut responder: Box<Responder>, client: ClientInfo) -> Result<Vec<Envelope>, End> {
        responder.read(data).map_err(|_| End)?;
        let remote = responder.remote_static().ok_or(End)?;
        let code = responder.pairing_code();
        let mut channel = responder.into_channel().map_err(|_| End)?;
        let (core, internal, generation) = (self.core.clone(), self.internal.clone(), self.generation);
        self.tasks.push(tokio::spawn(async move {
            let outcome = core.browser_pair(client, remote, code).await;
            let _ = internal.send(Internal::Paired { generation, outcome });
        }));
        let pending = Push::PairPending { expires_in_ms: u64::try_from(PAIR_TTL.as_millis()).unwrap_or(u64::MAX) };
        let out = seal(&mut channel, &Outgoing::Push(pending))?;
        self.stage = Stage::Waiting(channel);
        Ok(out)
    }

    /// A session's handshake with the keys of `pairing`: the reply, then `ready` over the new
    /// channel.
    fn session_start(&mut self, data: &[u8], pairing: Uuid, client: ClientInfo) -> Result<Vec<Envelope>, End> {
        let Some(key) = self.core.browser_static_secret() else { return Ok(vec![self.refusal()]) };
        let Some(remote) = self.core.browser_peer(pairing) else { return Ok(vec![error("not_paired")]) };
        let mut responder = Responder::session(&key, &remote).map_err(|_| End)?;
        // Keys that do not match the pairing's: the extension holds a pairing Lockra no longer has.
        if responder.read(data).is_err() {
            return Ok(vec![error("not_paired")]);
        }
        let reply = responder.write(&[]).map_err(|_| End)?;
        let mut channel = responder.into_channel().map_err(|_| End)?;
        let session = match self.core.browser_session_open(pairing, client) {
            Ok(session) => session,
            Err(refusal) => return Ok(vec![error(refusal_name(refusal))]),
        };
        let mut out = vec![Envelope::new(Kind::Hs, &reply)];
        out.extend(seal(&mut channel, &Outgoing::Push(Push::Ready { pairing }))?);
        self.stage = Stage::Session { channel, session, pairing };
        Ok(out)
    }

    /// An encrypted fragment: a session's request once it is whole.
    fn message(&mut self, data: &[u8]) -> Result<Vec<Envelope>, End> {
        let Stage::Session { channel, session, .. } = &mut self.stage else { return Err(End) };
        let session = *session;
        let Some(plain) = channel.open(data).map_err(|_| End)? else { return Ok(Vec::new()) };
        let request: Request = match serde_json::from_slice(&plain) {
            Ok(request) => request,
            // A request this version cannot read is answered, when its number can be read.
            Err(_) => {
                let req = serde_json::from_slice::<serde_json::Value>(&plain).ok().and_then(|v| v.get("req").and_then(serde_json::Value::as_u64));
                return match req {
                    Some(req) => self.send(&Outgoing::Response(Response::of(req, Err(Refusal::Invalid)))),
                    None => Ok(Vec::new()),
                };
            }
        };
        let core = &self.core;
        let req = request.req;
        let outcome = match request.op {
            Op::Status => core.browser_status(session).map(Answer::Status),
            Op::Query(query) => core.browser_query(session, query).map(Answer::Query),
            Op::Otp(otp) => core.browser_otp(session, otp).map(Answer::Otp),
            Op::Save(save) => core.browser_save(session, save).map(|item_id| Answer::Saved { item_id }),
            Op::Update(update) => core.browser_update(session, update).map(|()| Answer::Done {}),
            Op::Generate(generate) => core.browser_generate(session, generate).map(Answer::Generated),
            Op::CopyGenerated(copy) => core.browser_copy_generated(session, copy).map(|()| Answer::Done {}),
            Op::SiteRule(rule) => core.browser_site_rule(session, rule).map(|()| Answer::Done {}),
            Op::Search(search) => core.browser_search(session, search).map(|items| Answer::Found { items }),
            // These may wait for the user in Lockra: they answer when done.
            Op::Fill(fill) => {
                let (core, internal, generation) = (self.core.clone(), self.internal.clone(), self.generation);
                self.tasks.push(tokio::spawn(async move {
                    let response = Response::of(req, core.browser_fill(session, fill).await.map(Answer::Fill));
                    let _ = internal.send(Internal::Answer { generation, response });
                }));
                return Ok(Vec::new());
            }
            Op::Copy(copy) => {
                let (core, internal, generation) = (self.core.clone(), self.internal.clone(), self.generation);
                self.tasks.push(tokio::spawn(async move {
                    let response = Response::of(req, core.browser_copy(session, copy).await.map(|()| Answer::Done {}));
                    let _ = internal.send(Internal::Answer { generation, response });
                }));
                return Ok(Vec::new());
            }
            Op::Delete(delete) => {
                let (core, internal, generation) = (self.core.clone(), self.internal.clone(), self.generation);
                self.tasks.push(tokio::spawn(async move {
                    let response = Response::of(req, core.browser_delete(session, delete).await.map(|()| Answer::Done {}));
                    let _ = internal.send(Internal::Answer { generation, response });
                }));
                return Ok(Vec::new());
            }
        };
        self.send(&Outgoing::Response(Response::of(req, outcome)))
    }

    /// What a task of this connection handed back.
    fn internal(&mut self, message: Internal) -> Result<Vec<Envelope>, End> {
        match message {
            Internal::Answer { generation, response } if generation == self.generation => self.send(&Outgoing::Response(response)),
            Internal::Paired { generation, outcome } if generation == self.generation => {
                let Stage::Waiting(mut channel) = std::mem::replace(&mut self.stage, Stage::Fresh) else { return Ok(Vec::new()) };
                let push = match outcome {
                    Ok(pairing) => Push::Paired { pairing },
                    Err(reason) => Push::PairFailed { reason },
                };
                self.generation += 1;
                seal(&mut channel, &Outgoing::Push(push))
            }
            // From a session or a pairing that has ended.
            Internal::Answer { .. } | Internal::Paired { .. } => Ok(Vec::new()),
        }
    }

    /// The app's state changed: a lock ends the session or the pairing; a pairing removed in
    /// Lockra ends its session with `revoked`; the extension hears of a lock or unlock once.
    fn state(&mut self, phase: Phase) -> Vec<Envelope> {
        let state = app_state(phase);
        let mut out = Vec::new();
        if state == AppState::Unlocked {
            let ended = match &self.stage {
                Stage::Session { session, pairing, .. } if !self.core.browser_session_alive(*session) => Some(*pairing),
                _ => None,
            };
            if let Some(pairing) = ended {
                let still_paired = self.core.browser_peer(pairing).is_some();
                if !still_paired && let Stage::Session { channel, .. } = &mut self.stage {
                    out.extend(seal(channel, &Outgoing::Push(Push::Revoked)).unwrap_or_default());
                }
                self.end_stage();
                // Locked and unlocked again meanwhile: the extension hears it and opens a new session.
                if still_paired {
                    self.announced = None;
                }
            }
        } else if !matches!(self.stage, Stage::Fresh) {
            self.end_stage();
        }
        if self.client.is_some() && self.announced != Some(state) {
            self.announced = Some(state);
            out.push(plain(&AppPlain::State { state }));
        }
        out
    }

    /// `message` over the session's channel.
    fn send(&mut self, message: &Outgoing) -> Result<Vec<Envelope>, End> {
        match &mut self.stage {
            Stage::Session { channel, .. } => seal(channel, message),
            _ => Ok(Vec::new()),
        }
    }

    /// End the session or the pairing in progress: its tasks stop (a waiting request goes from
    /// Lockra's screen), its keys are dropped, and what it still hands back is ignored.
    fn end_stage(&mut self) {
        if let Stage::Session { session, .. } = &self.stage {
            self.core.browser_session_close(*session);
        }
        self.stage = Stage::Fresh;
        self.generation += 1;
        for task in self.tasks.drain(..) {
            task.abort();
        }
    }

    /// Why a handshake gets no static key: locked, or no extension offered here.
    fn refusal(&self) -> Envelope {
        match self.core.state().phase {
            Phase::Unlocked => error("unavailable"),
            Phase::Locked | Phase::NoVault => error("locked"),
        }
    }
}

fn app_state(phase: Phase) -> AppState {
    match phase {
        Phase::NoVault => AppState::NoVault,
        Phase::Locked => AppState::Locked,
        Phase::Unlocked => AppState::Unlocked,
    }
}

fn plain(message: &AppPlain) -> Envelope {
    // Plain data: serializing it cannot fail.
    #[allow(clippy::expect_used)]
    Envelope::new(Kind::Hello, &serde_json::to_vec(message).expect("a message serializes"))
}

fn error(error: &str) -> Envelope {
    plain(&AppPlain::Error { error: error.to_owned() })
}

fn refusal_name(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Locked => "locked",
        Refusal::NotPaired => "not_paired",
        Refusal::Busy => "busy",
        Refusal::Unavailable => "unavailable",
        _ => "bad_message",
    }
}

fn seal(channel: &mut Channel, message: &Outgoing) -> Result<Vec<Envelope>, End> {
    let bytes = zeroize::Zeroizing::new(serde_json::to_vec(message).map_err(|_| End)?);
    Ok(channel.seal(&bytes).map_err(|_| End)?.iter().map(|sealed| Envelope::new(Kind::Msg, sealed)).collect())
}

#[cfg(test)]
mod tests;
