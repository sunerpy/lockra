//! The channel's contract with the extension, as files its own implementation is checked against
//! (`tests/vectors/`): every message the protocol has, written by these types, and the Noise
//! handshakes and channel messages with fixed keys, made by snow. They are compared byte for byte;
//! after a deliberate change: `UPDATE_BROWSER_FIXTURES=1 cargo test -p lockra-browser --test protocol`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use data_encoding::HEXLOWER;
use lockra_browser::noise::{PAIRING_CODE_INFO, PAIRING_PATTERN, PROLOGUE, Responder, SESSION_PATTERN, pairing_code};
use lockra_browser::protocol::{Answer, AppPlain, AppState, ClientPlain, Envelope, Kind, Op, Outgoing, Push, Request, Response};
use lockra_core::{
    BrowserPolicy, ClientInfo, CopyRequest, FillAnswer, FillField, FillRequest, FrameFacts, GenerateAnswer, GenerateRequest, GenerateSave, ItemSummary,
    OtpAnswer, OtpRequest, Protection, QueryAnswer, QueryPurpose, QueryRequest, Refusal, SaveAction, SaveAdvice, SaveRequest, SearchRequest, SiteRuleKind,
    SiteRuleRequest, StatusAnswer, Trigger, UpdateRequest,
};
use serde_json::{Value, json};
use uuid::Uuid;
use zeroize::Zeroizing;

const UPDATE: &str = "UPDATE_BROWSER_FIXTURES";

fn id(n: u128) -> Uuid {
    Uuid::from_u128(0x5c0f_e1a2_0b3c_4d5e_8f60_0000_0000_0000 + n)
}

fn vectors() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("vectors")
}

/// Compare `value` with the file, or write it when updating.
fn check(name: &str, value: &Value) {
    let path = vectors().join(name);
    let text = serde_json::to_string_pretty(value).unwrap() + "\n";
    if std::env::var_os(UPDATE).is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let held = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{} is missing: run with {UPDATE}=1", path.display()));
    assert_eq!(held, text, "{name} changed: run with {UPDATE}=1 if that was meant, and update the extension");
}

fn to(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}

fn facts() -> FrameFacts {
    FrameFacts {
        frame_url: "https://accounts.example.com/login".into(),
        top_url: "https://www.example.com/".into(),
        frame_chain: vec!["https://www.example.com/frame".into()],
    }
}

fn client() -> ClientInfo {
    ClientInfo {
        browser: "chrome".into(),
        label: "Work".into(),
        extension: "abcdefghijklmnopabcdefghijklmnop".into(),
        version: "0.10.0".into(),
        page_access: true,
    }
}

fn summary() -> ItemSummary {
    ItemSummary {
        item_id: id(1),
        title: "Example".into(),
        username: "alice".into(),
        email: "alice@example.com".into(),
        has_password: true,
        has_totp: true,
        protection: Protection::Fill,
        favorite: false,
        downgrade: false,
        matches: true,
    }
}

fn requests() -> Vec<(&'static str, Request)> {
    let r = |req, op| Request { req, op };
    vec![
        ("status", r(1, Op::Status)),
        ("query", r(2, Op::Query(QueryRequest { facts: facts(), ..QueryRequest::default() }))),
        (
            "query_save",
            r(3, Op::Query(QueryRequest { facts: facts(), purpose: QueryPurpose::Save, username: "alice".into(), password: Zeroizing::new("s3cret".into()) })),
        ),
        ("fill", r(4, Op::Fill(FillRequest { item_id: Some(id(1)), facts: facts(), trigger: Trigger::InlineMenu, allow_http: false }))),
        ("fill_likeliest", r(5, Op::Fill(FillRequest { item_id: None, facts: facts(), trigger: Trigger::Shortcut, allow_http: true }))),
        ("otp", r(6, Op::Otp(OtpRequest { item_id: id(1), facts: facts() }))),
        ("copy", r(7, Op::Copy(CopyRequest { item_id: id(1), part: lockra_core::CopyPart::Password }))),
        (
            "save",
            r(
                8,
                Op::Save(SaveRequest {
                    facts: facts(),
                    title: String::new(),
                    username: "alice".into(),
                    password: Zeroizing::new("s3cret".into()),
                    group: Some("Work".into()),
                }),
            ),
        ),
        ("update", r(9, Op::Update(UpdateRequest { item_id: id(1), facts: facts(), password: Zeroizing::new("n3w".into()) }))),
        (
            "generate",
            r(
                10,
                Op::Generate(GenerateRequest {
                    facts: facts(),
                    rules: Some("minlength: 20; required: lower; required: digit;".into()),
                    minlength: Some(12),
                    maxlength: Some(64),
                    save: Some(GenerateSave { username: None, title: "Example".into() }),
                }),
            ),
        ),
        ("site_rule", r(11, Op::SiteRule(SiteRuleRequest { kind: Some(SiteRuleKind::NeverSave), facts: facts() }))),
        ("search", r(12, Op::Search(SearchRequest { text: "exam".into(), top_url: "https://www.example.com/".into() }))),
    ]
}

#[test]
fn protocol_messages() {
    let envelope = |t, bytes: &[u8]| Envelope::new(t, bytes);
    let mut session = envelope(Kind::Hs, b"handshake");
    session.p = Some(id(9));
    let ok = |req, answer| to(Outgoing::Response(Response::of(req, Ok(answer))));
    let refusals: Vec<Value> = [
        Refusal::Unavailable,
        Refusal::Locked,
        Refusal::NotPaired,
        Refusal::Paused,
        Refusal::Busy,
        Refusal::Invalid,
        Refusal::NeverFill,
        Refusal::NeverSave,
        Refusal::NoMatch,
        Refusal::CrossOrigin,
        Refusal::HttpDowngrade,
        Refusal::Protected,
        Refusal::Denied,
        Refusal::Expired,
        Refusal::RateLimited,
        Refusal::NotFound,
    ]
    .into_iter()
    .map(to)
    .collect();
    let mut request_map = serde_json::Map::new();
    for (name, request) in requests() {
        // Every example reads back as the request it is.
        let value = to(&request);
        assert_eq!(serde_json::from_value::<Request>(value.clone()).unwrap(), request, "{name}");
        request_map.insert(name.into(), value);
    }
    let fixtures = json!({
        "envelopes": {
            "hello": to(envelope(Kind::Hello, br#"{"op":"unlock_request"}"#)),
            "handshake": to(envelope(Kind::Hs, b"handshake")),
            "session_handshake": to(session),
            "message": to(envelope(Kind::Msg, b"sealed")),
        },
        "client_plain": {
            "hello": to(ClientPlain::Hello { proto_min: 1, proto_max: 1, client: client() }),
            "unlock_request": to(ClientPlain::UnlockRequest),
            // The host's alone: the app never gets them.
            "start_app": to(ClientPlain::StartApp { show: false }),
            "start_app_show": to(ClientPlain::StartApp { show: true }),
        },
        "app_plain": {
            "hello": to(AppPlain::Hello { proto_min: 1, proto_max: 1, proto: Some(1), app_version: "0.10.0".into(), state: AppState::Unlocked }),
            "hello_incompatible": to(AppPlain::Hello { proto_min: 1, proto_max: 1, proto: None, app_version: "0.10.0".into(), state: AppState::Locked }),
            "state": to(AppPlain::State { state: AppState::Locked }),
            "state_no_vault": to(AppPlain::State { state: AppState::NoVault }),
            // The host's own, when no app is there to answer.
            "state_not_running": to(AppPlain::State { state: AppState::NotRunning }),
            "unlock": to(AppPlain::Unlock { accepted: true }),
            "error": to(AppPlain::Error { error: "not_paired".into() }),
        },
        "requests": request_map,
        "responses": {
            "status": ok(1, Answer::Status(StatusAnswer { pairing: id(9), app_version: "0.10.0".into(), policy: BrowserPolicy::default(), items: 12, paused: false })),
            "query": ok(2, Answer::Query(QueryAnswer {
                site: "example.com".into(),
                level: Some(lockra_autofill_level()),
                items: vec![summary()],
                never_fill: false,
                never_save: false,
                cross_origin: false,
                policy: BrowserPolicy::default(),
                save: None,
            })),
            "query_save": ok(3, Answer::Query(QueryAnswer {
                site: "example.com".into(),
                level: None,
                items: Vec::new(),
                never_fill: false,
                never_save: false,
                cross_origin: true,
                policy: BrowserPolicy::default(),
                save: Some(SaveAdvice { action: SaveAction::Update, item_id: Some(id(1)) }),
            })),
            "fill": ok(4, Answer::Fill(FillAnswer {
                item_id: id(1),
                username: "alice".into(),
                password: Zeroizing::new("s3cret".into()),
                fields: vec![FillField { page_field: "pin".into(), value: Zeroizing::new("4242".into()) }],
                otp: Some("123456".into()),
                submit: false,
            })),
            "otp": ok(6, Answer::Otp(OtpAnswer { code: "654321".into(), copied: true })),
            "saved": ok(8, Answer::Saved { item_id: id(2) }),
            "generated": ok(10, Answer::Generated(GenerateAnswer { password: Zeroizing::new("q7vx2mz9kd4w8p1r6t3y".into()), username: "alice".into(), item_id: Some(id(3)) })),
            "found": ok(12, Answer::Found { items: vec![summary()] }),
            "done": ok(9, Answer::Done {}),
            "refused": to(Outgoing::Response(Response::of(5, Err(Refusal::HttpDowngrade)))),
        },
        "pushes": {
            "pair_pending": to(Outgoing::Push(Push::PairPending { expires_in_ms: 120_000 })),
            "paired": to(Outgoing::Push(Push::Paired { pairing: id(9) })),
            "pair_failed": to(Outgoing::Push(Push::PairFailed { reason: Refusal::Expired })),
            "ready": to(Outgoing::Push(Push::Ready { pairing: id(9) })),
            "revoked": to(Outgoing::Push(Push::Revoked)),
        },
        "refusals": refusals,
        "policy_default": to(BrowserPolicy::default()),
    });
    check("protocol.json", &fixtures);
}

fn lockra_autofill_level() -> lockra_core::MatchLevel {
    lockra_core::MatchLevel::Host
}

/// The fixed keys of the vectors (X25519 clamps them; any 32 bytes are a private key).
fn key(seed: u8) -> [u8; 32] {
    std::array::from_fn(|i| seed.wrapping_add(u8::try_from(i).unwrap()).wrapping_mul(7))
}

fn transport(state: snow::HandshakeState) -> snow::TransportState {
    state.into_transport_mode().unwrap()
}

fn write(state: &mut snow::HandshakeState, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 65_535];
    let n = state.write_message(payload, &mut out).unwrap();
    out.truncate(n);
    out
}

fn read(state: &mut snow::HandshakeState, message: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 65_535];
    let n = state.read_message(message, &mut out).unwrap();
    out.truncate(n);
    out
}

fn seal(state: &mut snow::TransportState, plain: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; plain.len() + 16];
    let n = state.write_message(plain, &mut out).unwrap();
    out.truncate(n);
    out
}

fn public(private: &[u8; 32]) -> Vec<u8> {
    // The public key the handshake carries: what the other side learns as the remote static.
    let mut initiator =
        snow::Builder::new(PAIRING_PATTERN.parse().unwrap()).prologue(PROLOGUE).unwrap().local_private_key(&[1; 32]).unwrap().build_initiator().unwrap();
    let mut responder =
        snow::Builder::new(PAIRING_PATTERN.parse().unwrap()).prologue(PROLOGUE).unwrap().local_private_key(private).unwrap().build_responder().unwrap();
    let m1 = write(&mut initiator, b"");
    read(&mut responder, &m1);
    let m2 = write(&mut responder, b"");
    read(&mut initiator, &m2);
    initiator.get_remote_static().unwrap().to_vec()
}

#[test]
fn noise_vectors() {
    let hex = |bytes: &[u8]| HEXLOWER.encode(bytes);
    let (app, extension) = (key(1), key(101));
    let (app_e, extension_e, app_e2, extension_e2) = (key(51), key(151), key(61), key(161));
    // A fragment's plaintext: the flag (0: the last), then the JSON.
    let request = br#"{"req":1,"op":"status"}"#;
    let mut fragment = vec![0u8];
    fragment.extend_from_slice(request);
    let answer = br#"{"push":"ready","pairing":"5c0fe1a2-0b3c-4d5e-8f60-000000000009"}"#;
    let mut answer_fragment = vec![0u8];
    answer_fragment.extend_from_slice(answer);

    // Pairing: XX, the extension initiates.
    let mut xi = snow::Builder::new(PAIRING_PATTERN.parse().unwrap())
        .prologue(PROLOGUE)
        .unwrap()
        .local_private_key(&extension)
        .unwrap()
        .fixed_ephemeral_key_for_testing_only(&extension_e)
        .build_initiator()
        .unwrap();
    let mut xr = snow::Builder::new(PAIRING_PATTERN.parse().unwrap())
        .prologue(PROLOGUE)
        .unwrap()
        .local_private_key(&app)
        .unwrap()
        .fixed_ephemeral_key_for_testing_only(&app_e)
        .build_responder()
        .unwrap();
    let m1 = write(&mut xi, b"");
    read(&mut xr, &m1);
    let m2 = write(&mut xr, b"");
    read(&mut xi, &m2);
    let m3 = write(&mut xi, b"");
    read(&mut xr, &m3);
    let hash = xr.get_handshake_hash().to_vec();
    assert_eq!(hash, xi.get_handshake_hash());
    let code = pairing_code(&hash);
    let (mut xi, mut xr) = (transport(xi), transport(xr));
    let xx_push = seal(&mut xr, &answer_fragment);
    let xx_request = seal(&mut xi, &fragment);
    // The library's responder takes the extension's messages the same way (its own ephemeral key
    // is random, so its messages differ; the code then too).
    let mut library = Responder::pairing(&app).unwrap();
    library.read(&m1).unwrap();
    assert!(!library.write(b"").unwrap().is_empty());

    // A session: KK between the paired static keys.
    let mut ki = snow::Builder::new(SESSION_PATTERN.parse().unwrap())
        .prologue(PROLOGUE)
        .unwrap()
        .local_private_key(&extension)
        .unwrap()
        .remote_public_key(&public(&app))
        .unwrap()
        .fixed_ephemeral_key_for_testing_only(&extension_e2)
        .build_initiator()
        .unwrap();
    let mut kr = snow::Builder::new(SESSION_PATTERN.parse().unwrap())
        .prologue(PROLOGUE)
        .unwrap()
        .local_private_key(&app)
        .unwrap()
        .remote_public_key(&public(&extension))
        .unwrap()
        .fixed_ephemeral_key_for_testing_only(&app_e2)
        .build_responder()
        .unwrap();
    let k1 = write(&mut ki, b"");
    read(&mut kr, &k1);
    let k2 = write(&mut kr, b"");
    read(&mut ki, &k2);
    let session_hash = kr.get_handshake_hash().to_vec();
    let (mut ki, mut kr) = (transport(ki), transport(kr));
    let kk_push = seal(&mut kr, &answer_fragment);
    let kk_request = seal(&mut ki, &fragment);
    let kk_request_2 = seal(&mut ki, &fragment);
    // The library's responder opens the extension's session from the same first message.
    let mut library = Responder::session(&app, &public(&extension).try_into().unwrap()).unwrap();
    library.read(&k1).unwrap();

    let vectors = json!({
        "about": "Made by snow 0.10.0 with fixed ephemeral keys (tests/protocol.rs). The extension's Noise must produce the initiator's messages byte for byte, read the responder's, and agree on the hashes, the code and the channel messages.",
        "prologue": String::from_utf8(PROLOGUE.to_vec()).unwrap(),
        "pairing_code_info": String::from_utf8(PAIRING_CODE_INFO.to_vec()).unwrap(),
        "pairing_code_of_zero_hash": pairing_code(&[0u8; 32]),
        "app_static": { "private": hex(&app), "public": hex(&public(&app)) },
        "extension_static": { "private": hex(&extension), "public": hex(&public(&extension)) },
        "pairing": {
            "pattern": PAIRING_PATTERN,
            "initiator_ephemeral": hex(&extension_e),
            "responder_ephemeral": hex(&app_e),
            "messages": [hex(&m1), hex(&m2), hex(&m3)],
            "handshake_hash": hex(&hash),
            "code": code,
            "responder_first_message_plain": hex(&answer_fragment),
            "responder_first_message": hex(&xx_push),
            "initiator_first_message_plain": hex(&fragment),
            "initiator_first_message": hex(&xx_request),
        },
        "session": {
            "pattern": SESSION_PATTERN,
            "initiator_ephemeral": hex(&extension_e2),
            "responder_ephemeral": hex(&app_e2),
            "messages": [hex(&k1), hex(&k2)],
            "handshake_hash": hex(&session_hash),
            "responder_first_message_plain": hex(&answer_fragment),
            "responder_first_message": hex(&kk_push),
            "initiator_messages_plain": hex(&fragment),
            "initiator_messages": [hex(&kk_request), hex(&kk_request_2)],
        },
    });
    check("noise.json", &vectors);
}
