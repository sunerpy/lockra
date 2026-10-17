//! What goes over the channel (docs/formats.md §10). Every frame is an [`Envelope`]: `hello`
//! carries the plain messages exchanged before a handshake, `hs` a handshake message, `msg` an
//! encrypted fragment. Requests carry a `req` number that their answer repeats; the app also
//! pushes messages of its own.

use data_encoding::BASE64;
use lockra_core::{
    ClientInfo, CopyGeneratedRequest, CopyRequest, DeleteRequest, FillAnswer, FillRequest, GenerateAnswer, GenerateRequest, ItemSummary, OtpAnswer, OtpRequest,
    QueryAnswer, QueryRequest, Refusal, SaveRequest, SearchRequest, SiteRuleRequest, StatusAnswer, UpdateRequest,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The envelope's version.
pub const ENVELOPE_VERSION: u32 = 1;
/// The protocol versions this app speaks.
pub const PROTO_MIN: u32 = 1;
/// The protocol versions this app speaks.
pub const PROTO_MAX: u32 = 1;

/// What a frame holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A plain message, before a handshake (or the app's lock and unlock).
    Hello,
    /// A handshake message.
    Hs,
    /// An encrypted fragment.
    Msg,
}

/// One frame: `{"v":1,"t":"hello"|"hs"|"msg","d":"<base64>"}`, with `p`, the pairing, on the first
/// message of a session's handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// [`ENVELOPE_VERSION`].
    pub v: u32,
    /// What it holds.
    pub t: Kind,
    /// The bytes, in base64.
    #[serde(default)]
    pub d: String,
    /// The pairing whose keys a session's handshake uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p: Option<Uuid>,
}

impl Envelope {
    /// An envelope of `t` with `bytes`.
    pub fn new(t: Kind, bytes: &[u8]) -> Self {
        Self { v: ENVELOPE_VERSION, t, d: BASE64.encode(bytes), p: None }
    }

    /// The bytes it carries; `None` when they are not base64.
    pub fn bytes(&self) -> Option<Vec<u8>> {
        BASE64.decode(self.d.as_bytes()).ok()
    }

    /// The frame for it.
    pub fn frame(&self) -> Vec<u8> {
        // An envelope is plain data: serializing it cannot fail.
        #[allow(clippy::expect_used)]
        serde_json::to_vec(self).expect("an envelope serializes")
    }
}

/// The extension's plain messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ClientPlain {
    /// The first message of a connection: the versions it speaks and what it is.
    Hello {
        /// The oldest protocol version it speaks.
        proto_min: u32,
        /// The newest.
        proto_max: u32,
        /// What it says of itself.
        #[serde(flatten)]
        client: ClientInfo,
    },
    /// Ask Lockra to unlock (the popup's 「在 Lockra 中解锁」).
    UnlockRequest,
    /// Start Lockra: for the host alone, which never passes it on. Without `show` it starts as
    /// the browser's (`--from-browser`: in the tray where Lockra runs in the background); with it,
    /// Lockra's window comes up. No answer: the extension says `hello` again until Lockra does.
    StartApp {
        /// Bring the window up (written only when it is wanted, as the extension writes it).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        show: bool,
    },
}

/// Whether the app can answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    /// Lockra has no vault yet.
    NoVault,
    /// Locked: nothing but `hello` and `unlock_request` until it unlocks.
    Locked,
    /// Unlocked: sessions open.
    Unlocked,
    /// Lockra is not running: the host says so, when the app's socket cannot be reached or the
    /// connection to it ends.
    NotRunning,
}

/// The app's plain messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AppPlain {
    /// The answer to `hello`: the versions, Lockra's own, and its state.
    Hello {
        /// The oldest protocol version the app speaks.
        proto_min: u32,
        /// The newest.
        proto_max: u32,
        /// The version both speak, the newest; none when they share none (one of them needs an
        /// update: the ranges say which).
        proto: Option<u32>,
        /// Lockra's version.
        app_version: String,
        /// Its state.
        state: AppState,
    },
    /// Lockra locked or unlocked; a session that was open has ended.
    State {
        /// The new state.
        state: AppState,
    },
    /// The answer to `unlock_request`: passed on to Lockra (at most every ten seconds).
    Unlock {
        /// It was.
        accepted: bool,
    },
    /// A plain message or a handshake that cannot be answered.
    Error {
        /// Why: `hello_first`, `version`, `bad_message`, or a refusal (`locked`, `not_paired`,
        /// `busy`, `unavailable`).
        error: String,
    },
}

/// A request over a session, `{"req":1,"op":"fill",…}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Its number, repeated by the answer.
    pub req: u64,
    /// What it asks.
    #[serde(flatten)]
    pub op: Op,
}

/// What a request asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// The session's state.
    Status,
    /// The logins for a frame, without secrets.
    Query(QueryRequest),
    /// One login's secrets for a frame.
    Fill(FillRequest),
    /// The code of a login filled on the step before, or of one the user chose.
    Otp(OtpRequest),
    /// A part of a login on the clipboard.
    Copy(CopyRequest),
    /// Save a submitted form as a new login.
    Save(SaveRequest),
    /// Change a login's password to a submitted one.
    Update(UpdateRequest),
    /// A password for a sign-up form.
    Generate(GenerateRequest),
    /// The sign-up form's generated password on the clipboard.
    CopyGenerated(CopyGeneratedRequest),
    /// "Not on this site."
    SiteRule(SiteRuleRequest),
    /// The popup's search.
    Search(SearchRequest),
    /// Delete a login the menu or the popup offered, once the user agreed there.
    Delete(DeleteRequest),
}

/// What a request gets. Untagged: the extension knows which op it asked; read back (the tests),
/// the variants are tried from the most specific shape to the least.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Answer {
    /// `status`.
    Status(StatusAnswer),
    /// `query`.
    Query(QueryAnswer),
    /// `fill`.
    Fill(FillAnswer),
    /// `generate`.
    Generated(GenerateAnswer),
    /// `otp`.
    Otp(OtpAnswer),
    /// `search`.
    Found {
        /// At most 50 logins.
        items: Vec<ItemSummary>,
    },
    /// `save`: the new login.
    Saved {
        /// Its id.
        item_id: Uuid,
    },
    /// `copy`, `update`, `site_rule`, `delete`: done.
    Done {},
}

/// The answer to request `req`: `{"req":1,"ok":true,"result":…}` or
/// `{"req":1,"ok":false,"error":"no_match"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    /// The request's number.
    pub req: u64,
    /// It was answered.
    pub ok: bool,
    /// The answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Answer>,
    /// Why it was not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Refusal>,
}

impl Response {
    /// The response to `req` from `outcome`.
    pub fn of(req: u64, outcome: Result<Answer, Refusal>) -> Self {
        match outcome {
            Ok(answer) => Self { req, ok: true, result: Some(answer), error: None },
            Err(refusal) => Self { req, ok: false, result: None, error: Some(refusal) },
        }
    }
}

/// What the app tells over a channel by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "push", rename_all = "snake_case")]
pub enum Push {
    /// The pairing handshake is done: the request waits in Lockra for the user, who compares the
    /// code.
    PairPending {
        /// Milliseconds until it lapses.
        expires_in_ms: u64,
    },
    /// The user approved: the pairing to open sessions with (the channel goes back to before a
    /// handshake).
    Paired {
        /// The pairing's id.
        pairing: Uuid,
    },
    /// The pairing request ended without one.
    PairFailed {
        /// Why: `denied`, `expired`, `busy`, `locked`.
        reason: Refusal,
    },
    /// The session is open.
    Ready {
        /// The pairing.
        pairing: Uuid,
    },
    /// The user removed this pairing: pair again.
    Revoked,
}

/// A message the app sends over a session's channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Outgoing {
    /// An answer.
    Response(Response),
    /// A push.
    Push(Push),
}
