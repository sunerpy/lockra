//! The service loop on a real core (with the fakes) and an extension played by the tests: the
//! plain messages, pairing, sessions, the lock, a removed pairing, requests and their answers, a
//! request that waits for the user, broken frames, and the shutdown.

use std::sync::Arc;
use std::time::Duration;

use lockra_core::fakes::{FakeBiometrics, FakeClipboard, FakeClock, FakeKeychain, FakeTransport, FakeUpdater};
use lockra_core::ui::{Features, Platform};
use lockra_core::{
    ClientInfo, Core, CoreConfig, FillRequest, FrameFacts, ItemDraft, KdfCost, Matching, Ports, Protection, QueryRequest, Refusal, SearchRequest, SecretChange,
    UrlDraft,
};
use tokio::io::DuplexStream;
use tokio::sync::oneshot;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::*;
use crate::noise::initiator::{Initiator, keypair};
use crate::noise::pairing_code;
use crate::protocol::{AppPlain, AppState, ClientPlain, Envelope, Kind, Op, Outgoing, Push, Request, Response};

const MASTER: &str = "correct horse battery";
const PASSWORD: &str = "hunter2-for-the-page";

struct App {
    core: Core,
    connector: MemoryConnector,
    stop: Option<oneshot::Sender<()>>,
    serving: JoinHandle<()>,
    _dir: tempfile::TempDir,
}

async fn app() -> App {
    let dir = tempfile::tempdir().unwrap();
    let config = CoreConfig {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        app_version: "0.10.0-test".into(),
        kdf: KdfCost::FAST_INSECURE,
        platform: Platform::Linux,
        features: Features { passwords: true },
    };
    let ports = Ports {
        secrets: Arc::new(FakeKeychain::default()),
        clipboard: Arc::new(FakeClipboard::default()),
        clock: Arc::new(FakeClock::new(1_790_000_000_000)),
        updater: Arc::new(FakeUpdater::default()),
        sync: Arc::new(FakeTransport::default()),
        biometrics: Arc::new(FakeBiometrics::default()),
    };
    let core = Core::start(config, ports);
    core.create_vault(Zeroizing::new(MASTER.into())).await.unwrap();
    let (listener, connector) = memory();
    let (stop, stopped) = oneshot::channel::<()>();
    let serving = tokio::spawn(serve(listener, core.clone(), async move {
        let _ = stopped.await;
    }));
    App { core, connector, stop: Some(stop), serving, _dir: dir }
}

fn pw(text: &str) -> Option<Zeroizing<String>> {
    Some(Zeroizing::new(text.into()))
}

/// The extension's side, as the tests play it.
struct Extension {
    stream: DuplexStream,
    key: [u8; 32],
    public: [u8; 32],
    app_key: Option<[u8; 32]>,
    pairing: Option<Uuid>,
    channel: Option<Channel>,
    next: u64,
}

impl Extension {
    async fn connect(app: &App) -> Self {
        let (key, public) = keypair().unwrap();
        Self { stream: app.connector.connect().await.unwrap(), key, public, app_key: None, pairing: None, channel: None, next: 0 }
    }

    async fn send(&mut self, envelope: &Envelope) {
        write_frame(&mut self.stream, Order::Little, &envelope.frame()).await.unwrap();
    }

    /// The next frame; `None` once the app ended the connection.
    async fn frame(&mut self) -> Option<Envelope> {
        let frame = tokio::time::timeout(Duration::from_secs(600), read_frame(&mut self.stream, Order::Little)).await.expect("an answer in time");
        frame.ok().flatten().map(|bytes| serde_json::from_slice(&bytes).unwrap())
    }

    async fn plain(&mut self, message: &ClientPlain) {
        self.send(&Envelope::new(Kind::Hello, &serde_json::to_vec(message).unwrap())).await;
    }

    async fn plain_reply(&mut self) -> AppPlain {
        let envelope = self.frame().await.expect("a frame");
        assert_eq!(envelope.t, Kind::Hello, "{envelope:?}");
        serde_json::from_slice(&envelope.bytes().unwrap()).unwrap()
    }

    async fn hello(&mut self) -> AppPlain {
        let client = ClientInfo { browser: "chrome".into(), label: "Work".into(), extension: "dev".into(), version: "0.10.0".into(), page_access: true };
        self.plain(&ClientPlain::Hello { proto_min: 1, proto_max: 1, client }).await;
        self.plain_reply().await
    }

    /// The pairing handshake up to the request waiting in Lockra; the code the extension shows.
    async fn pair_handshake(&mut self) -> String {
        let mut initiator = Initiator::pairing(&self.key, None).unwrap();
        self.send(&Envelope::new(Kind::Hs, &initiator.write(b"").unwrap())).await;
        let reply = self.frame().await.unwrap();
        assert_eq!(reply.t, Kind::Hs, "{reply:?}");
        initiator.read(&reply.bytes().unwrap()).unwrap();
        self.app_key = initiator.remote_static();
        self.send(&Envelope::new(Kind::Hs, &initiator.write(b"").unwrap())).await;
        let code = pairing_code(&initiator.hash());
        self.channel = Some(initiator.into_channel().unwrap());
        assert!(matches!(self.push().await, Push::PairPending { expires_in_ms: 120_000 }));
        code
    }

    /// A pairing the user approves in Lockra.
    async fn pair(&mut self, app: &App) -> Uuid {
        self.pair_handshake().await;
        let request = until_pairing(&app.core).await;
        app.core.browser_pair_approve(request, pw(MASTER), None).await.unwrap();
        let Push::Paired { pairing } = self.push().await else { panic!("not paired") };
        self.pairing = Some(pairing);
        self.channel = None;
        pairing
    }

    /// A session's handshake; the first plain or handshake reply when it is refused.
    async fn session(&mut self) -> Result<(), AppPlain> {
        let mut initiator = Initiator::session(&self.key, &self.app_key.unwrap(), None).unwrap();
        let mut envelope = Envelope::new(Kind::Hs, &initiator.write(b"").unwrap());
        envelope.p = self.pairing;
        self.send(&envelope).await;
        let reply = self.frame().await.unwrap();
        if reply.t == Kind::Hello {
            return Err(serde_json::from_slice(&reply.bytes().unwrap()).unwrap());
        }
        initiator.read(&reply.bytes().unwrap()).unwrap();
        self.channel = Some(initiator.into_channel().unwrap());
        assert!(matches!(self.push().await, Push::Ready { .. }));
        Ok(())
    }

    /// The next message over the channel.
    async fn message(&mut self) -> Outgoing {
        loop {
            let envelope = self.frame().await.expect("a frame");
            assert_eq!(envelope.t, Kind::Msg, "{envelope:?}");
            let channel = self.channel.as_mut().unwrap();
            if let Some(plain) = channel.open(&envelope.bytes().unwrap()).unwrap() {
                return serde_json::from_slice(&plain).unwrap();
            }
        }
    }

    async fn push(&mut self) -> Push {
        match self.message().await {
            Outgoing::Push(push) => push,
            Outgoing::Response(response) => panic!("a response, not a push: {response:?}"),
        }
    }

    /// Send `op`; its number.
    async fn ask(&mut self, op: Op) -> u64 {
        self.next += 1;
        let request = Request { req: self.next, op };
        self.send_raw(&serde_json::to_vec(&request).unwrap()).await;
        self.next
    }

    async fn send_raw(&mut self, bytes: &[u8]) {
        let sealed = self.channel.as_mut().unwrap().seal(bytes).unwrap();
        for fragment in sealed {
            self.send(&Envelope::new(Kind::Msg, &fragment)).await;
        }
    }

    async fn response(&mut self) -> Response {
        match self.message().await {
            Outgoing::Response(response) => response,
            Outgoing::Push(push) => panic!("a push, not a response: {push:?}"),
        }
    }

    async fn request(&mut self, op: Op) -> Response {
        let req = self.ask(op).await;
        let response = self.response().await;
        assert_eq!(response.req, req);
        response
    }
}

async fn until_pairing(core: &Core) -> Uuid {
    for _ in 0..100 {
        if let Some(request) = core.state().browser.pairing.first() {
            return request.id;
        }
        tokio::task::yield_now().await;
    }
    panic!("no pairing request");
}

fn github() -> ItemDraft {
    ItemDraft {
        title: "GitHub".into(),
        username: "octocat".into(),
        password: SecretChange::Set { value: Zeroizing::new(PASSWORD.into()) },
        urls: vec![UrlDraft { url: "https://github.com/".into(), matching: Matching::Domain }],
        ..ItemDraft::default()
    }
}

fn facts(url: &str) -> FrameFacts {
    FrameFacts { frame_url: url.into(), top_url: url.into(), frame_chain: Vec::new() }
}

async fn in_session(app: &App) -> Extension {
    let mut extension = Extension::connect(app).await;
    assert!(matches!(extension.hello().await, AppPlain::Hello { state: AppState::Unlocked, proto: Some(1), .. }));
    extension.pair(app).await;
    extension.session().await.unwrap();
    extension
}

#[tokio::test(start_paused = true)]
async fn before_a_handshake_only_hello_and_unlock_are_answered() {
    let app = app().await;
    let mut extension = Extension::connect(&app).await;
    // A handshake before hello.
    extension.send(&Envelope::new(Kind::Hs, b"x")).await;
    assert_eq!(extension.plain_reply().await, AppPlain::Error { error: "hello_first".into() });
    let AppPlain::Hello { proto_min, proto_max, proto, app_version, state } = extension.hello().await else { panic!() };
    assert_eq!((proto_min, proto_max, proto, app_version.as_str(), state), (1, 1, Some(1), "0.10.0-test", AppState::Unlocked));
    // Locked: no handshake; an unlock request goes through once every ten seconds.
    app.core.lock_vault();
    assert_eq!(extension.plain_reply().await, AppPlain::State { state: AppState::Locked });
    let mut initiator = Initiator::pairing(&extension.key, None).unwrap();
    extension.send(&Envelope::new(Kind::Hs, &initiator.write(b"").unwrap())).await;
    assert_eq!(extension.plain_reply().await, AppPlain::Error { error: "locked".into() });
    extension.plain(&ClientPlain::UnlockRequest).await;
    assert_eq!(extension.plain_reply().await, AppPlain::Unlock { accepted: true });
    extension.plain(&ClientPlain::UnlockRequest).await;
    assert_eq!(extension.plain_reply().await, AppPlain::Unlock { accepted: false });
    // An unreadable plain message is answered, not fatal.
    extension.send(&Envelope::new(Kind::Hello, b"{\"op\":\"later\"}")).await;
    assert_eq!(extension.plain_reply().await, AppPlain::Error { error: "bad_message".into() });
    // Unlocking is told; and versions that do not meet refuse every handshake.
    app.core.unlock(Zeroizing::new(MASTER.into())).await.unwrap();
    assert_eq!(extension.plain_reply().await, AppPlain::State { state: AppState::Unlocked });
    let client = ClientInfo::default();
    extension.plain(&ClientPlain::Hello { proto_min: 2, proto_max: 3, client }).await;
    assert!(matches!(extension.plain_reply().await, AppPlain::Hello { proto: None, proto_max: 1, .. }));
    extension.send(&Envelope::new(Kind::Hs, &initiator.write(b"").unwrap_or_default())).await;
    assert_eq!(extension.plain_reply().await, AppPlain::Error { error: "version".into() });
}

#[tokio::test(start_paused = true)]
async fn pairing_shows_one_code_on_both_sides_and_opens_sessions_once_approved() {
    let app = app().await;
    let mut extension = Extension::connect(&app).await;
    extension.hello().await;
    let code = extension.pair_handshake().await;
    let state = app.core.state();
    let request = &state.browser.pairing[0];
    assert_eq!((request.code.as_str(), request.browser.as_str(), request.label.as_str()), (code.as_str(), "chrome", "Work"));
    app.core.browser_pair_approve(request.id, pw(MASTER), None).await.unwrap();
    let Push::Paired { pairing } = extension.push().await else { panic!() };
    assert_eq!(app.core.browser_peer(pairing), Some(extension.public), "the extension's key is the pairing's");
    assert_eq!(extension.app_key.map(|k| k.len()), Some(32));
    extension.pairing = Some(pairing);
    extension.channel = None;
    extension.session().await.unwrap();
    assert!(app.core.state().browser.paired[0].connected);
    let status = extension.request(Op::Status).await;
    assert!(status.ok, "{status:?}");
    // A second connection of the same extension opens a session of its own.
    let mut again = Extension::connect(&app).await;
    again.hello().await;
    (again.key, again.public, again.app_key, again.pairing) = (extension.key, extension.public, extension.app_key, extension.pairing);
    again.session().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_pairing_that_is_denied_or_lapses_says_so_and_another_key_opens_nothing() {
    let app = app().await;
    let mut extension = Extension::connect(&app).await;
    extension.hello().await;
    extension.pair_handshake().await;
    let request = until_pairing(&app.core).await;
    app.core.browser_pair_deny(request).unwrap();
    assert_eq!(extension.push().await, Push::PairFailed { reason: Refusal::Denied });
    // Back before a handshake: pairing again works, and lapses after two minutes.
    extension.channel = None;
    extension.pair_handshake().await;
    assert_eq!(extension.push().await, Push::PairFailed { reason: Refusal::Expired }, "the paused clock runs to the deadline");
    assert!(app.core.state().browser.pairing.is_empty());
    // A session for a pairing that does not exist, or with keys that are not the pairing's.
    let other = in_session(&app).await;
    let mut stranger = Extension::connect(&app).await;
    stranger.hello().await;
    stranger.app_key = other.app_key;
    stranger.pairing = Some(Uuid::new_v4());
    assert_eq!(stranger.session().await, Err(AppPlain::Error { error: "not_paired".into() }));
    stranger.pairing = other.pairing;
    assert_eq!(stranger.session().await, Err(AppPlain::Error { error: "not_paired".into() }), "its own key, not the paired one");
}

#[tokio::test(start_paused = true)]
async fn locking_ends_the_session_and_unlocking_lets_a_new_one_open() {
    let app = app().await;
    let mut extension = in_session(&app).await;
    app.core.lock_vault();
    assert_eq!(extension.plain_reply().await, AppPlain::State { state: AppState::Locked });
    assert!(app.core.state().browser.paired.is_empty(), "nothing shown while locked");
    assert_eq!(extension.session().await, Err(AppPlain::Error { error: "locked".into() }));
    app.core.unlock(Zeroizing::new(MASTER.into())).await.unwrap();
    assert_eq!(extension.plain_reply().await, AppPlain::State { state: AppState::Unlocked });
    assert!(!app.core.state().browser.paired[0].connected, "the old session is gone");
    extension.session().await.unwrap();
    assert!(extension.request(Op::Status).await.ok);
    // An encrypted message after the session ended is not this protocol: the connection ends.
    app.core.lock_vault();
    assert_eq!(extension.plain_reply().await, AppPlain::State { state: AppState::Locked });
    extension.ask(Op::Status).await;
    assert!(extension.frame().await.is_none(), "closed");
}

#[tokio::test(start_paused = true)]
async fn a_pairing_removed_in_lockra_is_told_and_its_session_ends() {
    let app = app().await;
    let mut extension = in_session(&app).await;
    app.core.browser_unpair(extension.pairing.unwrap()).unwrap();
    assert_eq!(extension.push().await, Push::Revoked);
    extension.channel = None;
    assert_eq!(extension.session().await, Err(AppPlain::Error { error: "not_paired".into() }));
}

#[tokio::test(start_paused = true)]
async fn requests_reach_the_core_and_answers_carry_their_numbers() {
    let app = app().await;
    let mut extension = in_session(&app).await;
    let id = app.core.item_add(github()).unwrap();
    let query = extension.request(Op::Query(QueryRequest { facts: facts("https://github.com/login"), ..QueryRequest::default() })).await;
    let Some(Answer::Query(found)) = query.result else { panic!("{query:?}") };
    assert_eq!(found.items.iter().map(|i| i.item_id).collect::<Vec<_>>(), [id]);
    let fill = extension.request(Op::Fill(FillRequest { facts: facts("https://github.com/login"), ..FillRequest::default() })).await;
    let Some(Answer::Fill(answer)) = fill.result else { panic!("{fill:?}") };
    assert_eq!((answer.username.as_str(), answer.password.as_str()), ("octocat", PASSWORD));
    let refused = extension.request(Op::Fill(FillRequest { facts: facts("https://example.org/"), ..FillRequest::default() })).await;
    assert_eq!((refused.ok, refused.error), (false, Some(Refusal::NoMatch)));
    // A request this version cannot read is answered as invalid when it has a number, ignored
    // otherwise; the session goes on.
    extension.send_raw(br#"{"req":41,"op":"teleport"}"#).await;
    assert_eq!(extension.response().await, Response { req: 41, ok: false, result: None, error: Some(Refusal::Invalid) });
    extension.send_raw(b"[1,2,3]").await;
    assert!(extension.request(Op::Status).await.ok);
    // A request in several fragments.
    let long = extension.request(Op::Search(SearchRequest { text: "g".repeat(100_000), top_url: String::new() })).await;
    assert_eq!(long.result, Some(Answer::Found { items: Vec::new() }));
}

#[tokio::test(start_paused = true)]
async fn every_op_goes_through_to_the_core() {
    use lockra_core::{CopyPart, GenerateRequest, GenerateSave, OtpRequest, SaveRequest, SiteRuleKind, SiteRuleRequest, UpdateRequest};
    let app = app().await;
    let mut extension = in_session(&app).await;
    let entry = app.core.add_uri("otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub").unwrap();
    let id = app.core.item_add(ItemDraft { totp_entry: Some(entry), ..github() }).unwrap();
    assert!(extension.request(Op::Fill(FillRequest { facts: facts("https://github.com/"), ..FillRequest::default() })).await.ok);
    let otp = extension.request(Op::Otp(OtpRequest { item_id: id, facts: facts("https://github.com/2fa") })).await;
    assert!(matches!(otp.result, Some(Answer::Otp(ref answer)) if answer.code.len() == 6), "{otp:?}");
    let copy = extension.request(Op::Copy(lockra_core::CopyRequest { item_id: id, part: CopyPart::Username })).await;
    assert_eq!(copy.result, Some(Answer::Done {}));
    let save = SaveRequest { facts: facts("https://new.example/"), username: "me".into(), password: Zeroizing::new("pw".into()), ..SaveRequest::default() };
    let saved = extension.request(Op::Save(save)).await;
    let Some(Answer::Saved { item_id }) = saved.result else { panic!("{saved:?}") };
    let update = UpdateRequest { item_id, facts: facts("https://new.example/"), password: Zeroizing::new("pw2".into()) };
    assert_eq!(extension.request(Op::Update(update)).await.result, Some(Answer::Done {}));
    let generate = GenerateRequest { facts: facts("https://signup.example/"), save: Some(GenerateSave::default()), ..GenerateRequest::default() };
    let made = extension.request(Op::Generate(generate)).await;
    assert!(matches!(made.result, Some(Answer::Generated(ref answer)) if answer.item_id.is_some()), "{made:?}");
    let rule = SiteRuleRequest { kind: Some(SiteRuleKind::NeverFill), facts: facts("https://new.example/") };
    assert_eq!(extension.request(Op::SiteRule(rule)).await.result, Some(Answer::Done {}));
    let found = extension.request(Op::Search(SearchRequest { text: "github".into(), top_url: "https://github.com/".into() })).await;
    assert!(matches!(found.result, Some(Answer::Found { ref items }) if items.len() == 1));
    assert_eq!(app.core.state().site_rules.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_fill_that_waits_for_the_user_does_not_hold_up_other_requests() {
    let app = app().await;
    let mut extension = in_session(&app).await;
    app.core.item_add(ItemDraft { protection: Protection::Confirm, ..github() }).unwrap();
    let fill = extension.ask(Op::Fill(FillRequest { facts: facts("https://github.com/"), ..FillRequest::default() })).await;
    let status = extension.ask(Op::Status).await;
    let first = extension.response().await;
    assert_eq!((first.req, first.ok), (status, true), "the status answers while the fill waits");
    let waiting = loop {
        if let Some(waiting) = app.core.state().browser.confirms.first() {
            break waiting.id;
        }
        tokio::task::yield_now().await;
    };
    app.core.browser_confirm(waiting, pw(MASTER), None).await.unwrap();
    let filled = extension.response().await;
    assert_eq!(filled.req, fill);
    assert!(matches!(filled.result, Some(Answer::Fill(ref answer)) if answer.password.as_str() == PASSWORD));
    // A waiting fill goes from Lockra's screen when its connection does.
    extension.ask(Op::Fill(FillRequest { facts: facts("https://github.com/"), ..FillRequest::default() })).await;
    loop {
        if !app.core.state().browser.confirms.is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }
    drop(extension);
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert!(app.core.state().browser.confirms.is_empty());
    assert!(!app.core.state().browser.paired[0].connected);
}

#[tokio::test(start_paused = true)]
async fn broken_frames_end_the_connection() {
    let app = app().await;
    for frame in
        [b"not json".to_vec(), br#"{"v":2,"t":"hello","d":""}"#.to_vec(), br#"{"v":1,"t":"msg","d":"!!"}"#.to_vec(), br#"{"v":1,"t":"msg","d":""}"#.to_vec()]
    {
        let mut extension = Extension::connect(&app).await;
        write_frame(&mut extension.stream, Order::Little, &frame).await.unwrap();
        assert!(extension.frame().await.is_none(), "{}", String::from_utf8_lossy(&frame));
    }
    // A length over the limit.
    let mut extension = Extension::connect(&app).await;
    tokio::io::AsyncWriteExt::write_all(&mut extension.stream, &u32::MAX.to_le_bytes()).await.unwrap();
    assert!(extension.frame().await.is_none());
    // A pairing handshake whose last message does not decrypt.
    let mut extension = Extension::connect(&app).await;
    extension.hello().await;
    let mut initiator = Initiator::pairing(&extension.key, None).unwrap();
    extension.send(&Envelope::new(Kind::Hs, &initiator.write(b"").unwrap())).await;
    extension.frame().await.unwrap();
    extension.send(&Envelope::new(Kind::Hs, &[0u8; 96])).await;
    assert!(extension.frame().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn the_shutdown_ends_every_connection_and_session() {
    let mut app = app().await;
    let mut first = in_session(&app).await;
    let mut second = Extension::connect(&app).await;
    second.hello().await;
    assert!(app.core.state().browser.paired[0].connected);
    app.stop.take().unwrap().send(()).unwrap();
    (&mut app.serving).await.unwrap();
    assert!(first.frame().await.is_none() && second.frame().await.is_none());
    assert!(!app.core.state().browser.paired[0].connected, "the session closed with it");
    assert!(app.connector.connect().await.is_err(), "no longer listening");
}
