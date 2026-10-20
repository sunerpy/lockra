//! The browser extension as the core sees it (docs/security.md, "Browser extension"): what the
//! vault keeps of it (the app's static key, the paired extensions, the policy), what the interface
//! shows, and the requests a paired extension makes, every decision on them taken here: the match
//! for the frame, the frame rule, the HTTP downgrade, the protection level, the rate limits. The
//! channel, Noise over the native-messaging host, is `lockra-browser`'s: it calls the
//! `Core::browser_*` methods, and the core holds no port of it and calls nothing back.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::time::Duration;

use data_encoding::BASE64;
use lockra_autofill::{DomainScope, EquivalentDomains, MatchLevel, SavedUrl, WebUrl};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::sync::oneshot;
use tokio::time::Instant;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::entry::VaultData;
use crate::item::{ItemKind, Matching, Protection, SiteRule, SiteRuleKind, Tri, VaultItem};
use crate::lenient::Lenient;
use crate::logins::CopyPart;

/// How long a pairing request waits for the user.
pub const PAIR_TTL: Duration = Duration::from_secs(120);
/// The most pairing requests waiting at once.
pub const MAX_PENDING_PAIRS: usize = 3;
/// How long a request to fill or copy a login that asks first waits for the user.
pub const CONFIRM_TTL: Duration = Duration::from_secs(60);
/// The most entries the activity list keeps (in memory, emptied as the vault locks).
pub const MAX_ACTIVITY: usize = 100;
/// The least time between two requests to unlock.
pub const UNLOCK_REQUEST_EVERY: Duration = Duration::from_secs(10);
/// The most fills (and other answers carrying a secret) one session gets in a minute.
pub const FILLS_PER_MINUTE: usize = 10;
/// The most queries one session makes in a minute.
pub const QUERIES_PER_MINUTE: usize = 120;
/// The most sites one session gets secrets for in ten minutes.
pub const SITES_PER_TEN_MINUTES: usize = 20;
/// How long after a fill the linked code is given for the same site (a sign-in in two steps).
pub const OTP_FOLLOW_UP: Duration = Duration::from_secs(5 * 60);
/// How long after a fill the user asked for its sign-in's next step may get the password, once: a
/// first step that took the user name alone, then the page that asks for the password.
pub const NEXT_STEP_FOR: Duration = Duration::from_secs(2 * 60);
/// The most results a search returns.
pub const SEARCH_MAX: usize = 50;
/// The longest URL a request may carry (characters).
pub const MAX_URL_CHARS: usize = 8192;
/// The most ancestors a frame may have.
pub const MAX_FRAME_CHAIN: usize = 32;

const MINUTE: Duration = Duration::from_secs(60);
const TEN_MINUTES: Duration = Duration::from_secs(600);

// ---- what the vault keeps -----------------------------------------------------------------------

/// The app's static Noise key, an X25519 private key: in the vault's local part only, never in a
/// backup, never synced, never sent to the webview.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct StaticKey(pub(crate) Zeroizing<[u8; 32]>);

impl fmt::Debug for StaticKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StaticKey(..)")
    }
}

impl Serialize for StaticKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&Zeroizing::new(BASE64.encode(self.0.as_slice())))
    }
}

impl<'de> Deserialize<'de> for StaticKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = Zeroizing::new(String::deserialize(deserializer)?);
        let bytes = Zeroizing::new(BASE64.decode(text.as_bytes()).map_err(serde::de::Error::custom)?);
        let key: [u8; 32] = bytes.as_slice().try_into().map_err(|_| serde::de::Error::custom("a key is 32 bytes"))?;
        Ok(Self(Zeroizing::new(key)))
    }
}

/// A public key in base64.
mod public_key {
    use data_encoding::BASE64;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(key: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&BASE64.encode(key))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 32], D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = BASE64.decode(text.as_bytes()).map_err(serde::de::Error::custom)?;
        bytes.as_slice().try_into().map_err(|_| serde::de::Error::custom("a key is 32 bytes"))
    }
}

/// The browser part of the vault's local data: this device's alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BrowserLocal {
    /// The app's static key, made the first time the channel needs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<StaticKey>,
    /// The extensions the user paired.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paired: Vec<PairedExtension>,
    /// What the extensions may do.
    #[serde(default, skip_serializing_if = "BrowserPolicy::is_default")]
    pub policy: BrowserPolicy,
}

impl BrowserLocal {
    pub(crate) fn is_empty(&self) -> bool {
        self.key.is_none() && self.paired.is_empty() && self.policy.is_default()
    }
}

/// An extension the user paired with Lockra after comparing the code both showed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PairedExtension {
    /// The pairing's id: the extension names it to open a session.
    pub id: Uuid,
    /// The browser, as the extension named it (`chrome`, `edge`, `firefox`…).
    pub browser: String,
    /// What the extension called itself (a browser profile), when it did.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// The extension's id, as it said.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub extension: String,
    /// Its static Noise key.
    #[serde(with = "public_key")]
    pub public_key: [u8; 32],
    /// When it was paired, Unix milliseconds.
    pub paired_at_ms: u64,
    /// When it last opened a session, Unix milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_ms: Option<u64>,
}

/// A sign-in form in a frame of another site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameRule {
    /// Never fill it. The default.
    #[default]
    Never,
    /// Ask in Lockra first.
    Ask,
}

/// What the paired extensions may do, the user's choice for this device. Kept in the vault, not in
/// `settings.json`: editing that file must not turn a risky option on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserPolicy {
    /// Fill a login's linked code into the page's one-time-code field.
    #[serde(default = "yes")]
    pub fill_otp: bool,
    /// Copy that code to the clipboard as well.
    #[serde(default = "yes")]
    pub copy_otp: bool,
    /// Submit the form once filled.
    #[serde(default)]
    pub auto_submit: bool,
    /// Ask in the page before filling.
    #[serde(default)]
    pub confirm_fill: bool,
    /// A sign-in form in a frame of another site; a rule of a later version reads as `never`.
    #[serde(default)]
    pub cross_origin_frames: Lenient<FrameRule>,
    /// Fill as a page loads, without a click (the risky one: docs/security.md).
    #[serde(default)]
    pub on_page_load: bool,
    /// The port counts too: a login saved for another port of the site (or for none, where the
    /// page has one) is not offered there, so each port of an address has its own logins.
    #[serde(default)]
    pub match_port: bool,
    /// Pages match by their full host: the domain rule asks for the page's host and port, as the
    /// host rule does, so a login saved for one host of a site (`git.example.com`) is not offered
    /// on another (`wiki.example.com`, `example.com`). The `match_host` and `match_domain` site
    /// rules choose otherwise for a domain and its subdomains.
    #[serde(default)]
    pub match_host: bool,
}

fn yes() -> bool {
    true
}

impl Default for BrowserPolicy {
    fn default() -> Self {
        Self {
            fill_otp: true,
            copy_otp: true,
            auto_submit: false,
            confirm_fill: false,
            cross_origin_frames: Lenient::Known(FrameRule::Never),
            on_page_load: false,
            match_port: false,
            match_host: false,
        }
    }
}

impl BrowserPolicy {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The frame rule; one of a later version counts as `never`.
    pub fn frames(&self) -> FrameRule {
        self.cross_origin_frames.or(FrameRule::Never)
    }

    /// How far the domain rule reaches where pages match by the full host (`host`) or by the
    /// site: the port counts as [`Self::match_port`] says, and always for the host.
    pub(crate) fn scope(&self, host: bool) -> DomainScope {
        match (host, self.match_port) {
            (true, _) => DomainScope::Host,
            (false, true) => DomainScope::SitePort,
            (false, false) => DomainScope::Site,
        }
    }

    /// Whether `next` lets the extensions do something this policy does not: such a change asks
    /// for the master password or the biometric check.
    pub(crate) fn riskier(&self, next: &Self) -> bool {
        (next.on_page_load && !self.on_page_load)
            || (next.auto_submit && !self.auto_submit)
            || (next.fill_otp && !self.fill_otp)
            || (self.confirm_fill && !next.confirm_fill)
            || (next.frames() == FrameRule::Ask && self.frames() == FrameRule::Never)
    }
}

// ---- what the interface shows ---------------------------------------------------------------------

/// Settings › Browser and the requests waiting for the user.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct BrowserView {
    /// This build offers the browser extension here: password management is on and this is the
    /// desktop app.
    pub available: bool,
    /// The integration is on: Lockra listens for the extensions (`settings.json`, so it shows
    /// while locked too).
    pub enabled: bool,
    /// The paired extensions; empty unless unlocked.
    pub paired: Vec<PairedView>,
    /// Pairing requests waiting for the user.
    pub pairing: Vec<PairRequestView>,
    /// Requests to fill or copy a login that wait for the user.
    pub confirms: Vec<ConfirmView>,
    /// What the extensions may do (the defaults unless unlocked).
    pub policy: BrowserPolicy,
    /// What the extensions did, newest first; in memory only, emptied as the vault locks.
    pub activity: Vec<ActivityView>,
}

/// A paired extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PairedView {
    /// The pairing.
    pub id: Uuid,
    /// The browser it runs in.
    pub browser: String,
    /// What it called itself.
    pub label: String,
    /// Its id.
    pub extension: String,
    /// When it was paired.
    pub paired_at_ms: u64,
    /// When it last connected.
    pub last_seen_ms: Option<u64>,
    /// It is connected now.
    pub connected: bool,
    /// It may run in pages (the browser's site access), as it said when it connected last; none
    /// until it has connected.
    pub page_access: Option<bool>,
    /// The version it said it was when it connected last; none until it has, or when it named
    /// none. Settings › 浏览器 tells one older than Lockra.
    pub version: Option<String>,
    /// It asked too often and gets nothing until the user resumes it.
    pub paused: bool,
}

/// A pairing request: the user compares the code with the extension's and approves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PairRequestView {
    /// The request.
    pub id: Uuid,
    /// The six digits both sides show.
    pub code: String,
    /// The browser.
    pub browser: String,
    /// What the extension called itself.
    pub label: String,
    /// Its id.
    pub extension: String,
    /// When the request lapses, Unix milliseconds.
    pub expires_at_ms: u64,
}

/// Why a request waits for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmReason {
    /// The login asks for the master password or the biometric check first (`confirm`).
    Protected,
    /// The form is in a frame of another site and the policy says to ask.
    CrossOrigin,
}

/// What the extension wants to do with the login.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmAction {
    /// Fill the form.
    Fill,
    /// Put a part of it on the clipboard.
    Copy,
    /// Delete it (the user agreed in the extension's menu or popup first).
    Delete,
}

/// A request to fill or copy a login, waiting for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfirmView {
    /// The request.
    pub id: Uuid,
    /// The login.
    pub item_id: Uuid,
    /// Its title.
    pub title: String,
    /// Its user name.
    pub username: String,
    /// The frame's address (the form's), for a fill.
    pub frame_url: String,
    /// The page's address, when the form is in a frame of another site.
    pub top_url: Option<String>,
    /// The browser.
    pub browser: String,
    /// What the extension called itself.
    pub label: String,
    /// Why it waits.
    pub reasons: Vec<ConfirmReason>,
    /// Fill or copy.
    pub action: ConfirmAction,
    /// It needs the master password or the biometric check, not a click.
    pub needs_proof: bool,
    /// When it lapses, Unix milliseconds.
    pub expires_at_ms: u64,
}

/// What an extension did, or was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityEvent {
    /// It was paired.
    Paired,
    /// Its pairing was removed.
    Unpaired,
    /// A form was filled.
    Filled,
    /// A linked code was given for a second step.
    OtpFilled,
    /// A part of a login went to the clipboard.
    Copied,
    /// A new login was saved.
    Saved,
    /// A login's password was changed.
    Updated,
    /// A login was deleted, from the extension's menu or popup.
    Deleted,
    /// A password was made for a sign-up form.
    Generated,
    /// A request was refused.
    Refused,
    /// It asked too often and was paused.
    Paused,
}

/// One line of the activity list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivityView {
    /// When, Unix milliseconds.
    pub at_ms: u64,
    /// The browser.
    pub browser: String,
    /// What the extension called itself.
    pub label: String,
    /// The site (registrable domain), when there was one.
    pub site: String,
    /// What happened.
    pub event: ActivityEvent,
    /// Why it was refused.
    pub refusal: Option<Refusal>,
    /// The login, when one was involved.
    pub item_id: Option<Uuid>,
    /// Its title then.
    pub title: Option<String>,
}

// ---- what the channel asks and is answered ------------------------------------------------------

/// Why a request of an extension gets nothing. The extension words it for the user; none of
/// them is an error of Lockra's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// This build or platform offers no browser extension.
    #[error("unavailable")]
    Unavailable,
    /// The vault is locked.
    #[error("locked")]
    Locked,
    /// The pairing is gone (removed in Lockra) or was never made.
    #[error("not paired")]
    NotPaired,
    /// The extension asked too often and waits for the user to resume it.
    #[error("paused")]
    Paused,
    /// Too many pairing requests are waiting.
    #[error("busy")]
    Busy,
    /// The request is malformed: an address that is not a web page, a value too long.
    #[error("invalid")]
    Invalid,
    /// The user does not want anything filled on this site.
    #[error("never fill")]
    NeverFill,
    /// The user does not want passwords of this site saved.
    #[error("never save")]
    NeverSave,
    /// No login matches the frame, or not the one named.
    #[error("no match")]
    NoMatch,
    /// The form is in a frame of another site and the policy does not fill there.
    #[error("cross origin")]
    CrossOrigin,
    /// The page is `http` while the login was saved for `https`: the user agrees in the page first.
    #[error("http downgrade")]
    HttpDowngrade,
    /// The login is not handed to the extension, or not this way (as a page loads).
    #[error("protected")]
    Protected,
    /// The user said no.
    #[error("denied")]
    Denied,
    /// The user did not answer in time.
    #[error("expired")]
    Expired,
    /// Too many requests: the extension is paused.
    #[error("rate limited")]
    RateLimited,
    /// No such login, or nothing to give (no password, no linked account).
    #[error("not found")]
    NotFound,
}

/// What an extension says of itself. Shown to the user; no decision rests on it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientInfo {
    /// The browser (`chrome`, `edge`, `firefox`, `chromium`, `brave`…).
    #[serde(default)]
    pub browser: String,
    /// A name of its own (a browser profile).
    #[serde(default)]
    pub label: String,
    /// The extension's id.
    #[serde(default)]
    pub extension: String,
    /// The extension's version.
    #[serde(default)]
    pub version: String,
    /// It may run in pages (the browser's site access).
    #[serde(default)]
    pub page_access: bool,
}

impl ClientInfo {
    /// The same, cut to sizes the interface can show and without control characters.
    pub fn cleaned(self) -> Self {
        Self {
            browser: clean_label(&self.browser, 32).to_lowercase(),
            label: clean_label(&self.label, 64),
            extension: clean_label(&self.extension, 128),
            version: clean_label(&self.version, 32),
            page_access: self.page_access,
        }
    }
}

fn clean_label(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_owned()
}

/// Where a request comes from, as the extension's background script took it from the browser
/// (never from the page): the frame holding the form, the tab's page, and the frame's ancestors.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameFacts {
    /// The address of the frame the form is in.
    pub frame_url: String,
    /// The address of the tab's page.
    pub top_url: String,
    /// The frame's ancestors, the nearest first, up to (and without) the page; empty for a form in
    /// the page itself.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frame_chain: Vec<String>,
}

/// What made the extension ask for a fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// The keyboard shortcut. The default.
    #[default]
    Shortcut,
    /// The toolbar's popup.
    Popup,
    /// The menu next to the field.
    InlineMenu,
    /// The page loaded (only where the policy and the login allow it).
    PageLoad,
    /// The next step of a sign-in the user started moments before on this site: the page that
    /// asks for the password after the one that took the user name. Named, filled once, never for
    /// a login that asks first and never where the user would be asked.
    NextStep,
}

/// What a query is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryPurpose {
    /// The logins to offer for the form. The default.
    #[default]
    Fill,
    /// Whether to offer saving what was just submitted.
    Save,
}

/// A query: the logins for a frame, without secrets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryRequest {
    /// Where.
    #[serde(flatten)]
    pub facts: FrameFacts,
    /// What for.
    #[serde(default)]
    pub purpose: QueryPurpose,
    /// For `save`: the user name submitted.
    #[serde(default)]
    pub username: String,
    /// For `save`: the password submitted (compared here, kept nowhere).
    #[serde(default)]
    pub password: Zeroizing<String>,
}

/// A login as the extension lists it: no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemSummary {
    /// The login.
    pub item_id: Uuid,
    /// Its title.
    pub title: String,
    /// Its user name.
    pub username: String,
    /// Its email address.
    pub email: String,
    /// It holds a password.
    pub has_password: bool,
    /// A code account is linked to it.
    pub has_totp: bool,
    /// Its protection level.
    pub protection: Protection,
    /// Pinned to the top.
    pub favorite: bool,
    /// Its matching URL is `https` while the page is `http`.
    pub downgrade: bool,
    /// It matches the frame (a search also finds logins of other sites, to copy only).
    pub matches: bool,
}

/// What a submitted form calls for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveAction {
    /// Nothing: the login is saved already, or the site is on the never-save list.
    None,
    /// Offer to save a new login.
    Save,
    /// Offer to change the password of the login with this user name.
    Update,
}

/// The answer to a save query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveAdvice {
    /// What to offer.
    pub action: SaveAction,
    /// The login to update.
    pub item_id: Option<Uuid>,
}

/// The answer to a query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryAnswer {
    /// The frame's site (registrable domain).
    pub site: String,
    /// The level the logins share; none when nothing matches.
    pub level: Option<MatchLevel>,
    /// The logins at the best level, the likeliest first; none on a never-fill site.
    pub items: Vec<ItemSummary>,
    /// The site is on the never-fill list: nothing is filled there.
    pub never_fill: bool,
    /// The site is on the never-save list.
    pub never_save: bool,
    /// The form is in a frame of another site.
    pub cross_origin: bool,
    /// The frame matches by its full host, as the policy or a site rule about its host or a
    /// parent of it says: the popup's switch for the site.
    pub host_matching: bool,
    /// What the extension may do (the policy).
    pub policy: BrowserPolicy,
    /// For a save query: what to offer.
    pub save: Option<SaveAdvice>,
}

/// A request to fill a form.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FillRequest {
    /// The login; none: the likeliest match.
    #[serde(default)]
    pub item_id: Option<Uuid>,
    /// Where.
    #[serde(flatten)]
    pub facts: FrameFacts,
    /// What made it ask.
    #[serde(default)]
    pub trigger: Trigger,
    /// The user agreed in the page to fill an `https` login into this `http` page.
    #[serde(default)]
    pub allow_http: bool,
}

/// A custom field to fill, by the page field it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FillField {
    /// The page's field (`name` or `id`).
    pub page_field: String,
    /// The value.
    pub value: Zeroizing<String>,
}

/// What a fill gets: one login's secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FillAnswer {
    /// The login.
    pub item_id: Uuid,
    /// The user name (or the email address, when it has none).
    pub username: String,
    /// The password.
    pub password: Zeroizing<String>,
    /// Custom fields that name a page field.
    pub fields: Vec<FillField>,
    /// The linked account's current code, when the page may get it now.
    pub otp: Option<String>,
    /// Submit the form once filled.
    pub submit: bool,
}

/// A request for a login's code: for a second sign-in step, or where the user asked for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtpRequest {
    /// The login filled on the step before, or the one the user chose.
    pub item_id: Uuid,
    /// Where.
    #[serde(flatten)]
    pub facts: FrameFacts,
    /// What made it ask. None: the next step of a fill, which gets the code of a login filled on
    /// this site in the last minutes. The popup, the menu or the shortcut: the user asked for this
    /// login's code, which a login offered for the frame gets (as the page load and the next step
    /// ask for nothing themselves, they are the next step's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<Trigger>,
}

/// A request to delete a login, which the user agreed to in the extension's menu or popup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteRequest {
    /// The login: one offered for the frame.
    pub item_id: Uuid,
    /// Where the extension offered it.
    #[serde(flatten)]
    pub facts: FrameFacts,
}

/// A request to link a code to a login: the save prompt took a one-time code a page asks for in a
/// password field, and the user chose 「关联验证码…」 there. Lockra comes up on the login with its
/// code picker open, and the user picks the account in Lockra: the extension never sees them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkCodeRequest {
    /// The login: one offered for the frame.
    pub item_id: Uuid,
    /// Where the extension offered it.
    #[serde(flatten)]
    pub facts: FrameFacts,
}

/// The code for a second step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtpAnswer {
    /// The current code.
    pub code: String,
    /// It went to the clipboard too.
    pub copied: bool,
    /// Submit the code's form once filled (the login's choice, or the policy's).
    #[serde(default)]
    pub submit: bool,
}

/// A request to put a part of a login on the clipboard (the popup's search).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyRequest {
    /// The login.
    pub item_id: Uuid,
    /// What.
    pub part: CopyPart,
}

/// A request to put a password the app generated for a sign-up form on the clipboard (the sign-up
/// menu's copy).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyGeneratedRequest {
    /// Where the menu is: the password must have been made for this site.
    #[serde(flatten)]
    pub facts: FrameFacts,
    /// The password the menu shows.
    pub password: Zeroizing<String>,
}

/// A request to save what a form submitted, as a new login.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveRequest {
    /// Where it was submitted.
    #[serde(flatten)]
    pub facts: FrameFacts,
    /// The title the user gave; empty: the site's name.
    #[serde(default)]
    pub title: String,
    /// The user name.
    #[serde(default)]
    pub username: String,
    /// The password.
    pub password: Zeroizing<String>,
    /// The group the user chose.
    #[serde(default)]
    pub group: Option<String>,
}

/// A request to change a login's password to what a form submitted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRequest {
    /// The login.
    pub item_id: Uuid,
    /// Where it was submitted.
    #[serde(flatten)]
    pub facts: FrameFacts,
    /// The new password.
    pub password: Zeroizing<String>,
}

/// A new login saved with a generated password, in the same step.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerateSave {
    /// The user name typed on the page; none: an empty one.
    #[serde(default)]
    pub username: Option<String>,
    /// The title; empty: the site's name.
    #[serde(default)]
    pub title: String,
}

/// A request for a password for a sign-up form.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerateRequest {
    /// Where.
    #[serde(flatten)]
    pub facts: FrameFacts,
    /// The page's `passwordrules` attribute.
    #[serde(default)]
    pub rules: Option<String>,
    /// The password field's `minlength`.
    #[serde(default)]
    pub minlength: Option<u32>,
    /// Its `maxlength`.
    #[serde(default)]
    pub maxlength: Option<u32>,
    /// Save a login with it now.
    #[serde(default)]
    pub save: Option<GenerateSave>,
}

/// A generated password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerateAnswer {
    /// The password.
    pub password: Zeroizing<String>,
    /// The user name to fill with it (the saved login's).
    pub username: String,
    /// The login saved with it.
    pub item_id: Option<Uuid>,
}

/// A site rule the extension adds for the frame's site ("not on this site").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteRuleRequest {
    /// `never_save` or `never_fill`.
    pub kind: Option<SiteRuleKind>,
    /// Where.
    #[serde(flatten)]
    pub facts: FrameFacts,
}

/// The popup's switch for the frame's site: its pages match by the full host or by the site.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteMatchingRequest {
    /// Where.
    #[serde(flatten)]
    pub facts: FrameFacts,
    /// By the full host; false: by the site (the registrable domain).
    pub host: bool,
}

/// A search of the popup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    /// What was typed.
    pub text: String,
    /// The tab's page, to tell the logins that match it.
    #[serde(default)]
    pub top_url: String,
}

/// The session's state, for the popup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusAnswer {
    /// The pairing.
    pub pairing: Uuid,
    /// Lockra's version.
    pub app_version: String,
    /// What the extension may do.
    pub policy: BrowserPolicy,
    /// How many logins the extensions may see.
    pub items: u32,
    /// The extension asked too often and gets nothing until the user resumes it in Lockra.
    pub paused: bool,
}

/// A session of a paired extension, as the core counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);

// ---- what the core holds while running ----------------------------------------------------------

/// The extensions' sessions and the requests waiting for the user: in memory only.
#[derive(Debug, Default)]
pub(crate) struct BrowserRuntime {
    pub next_session: u64,
    pub sessions: HashMap<u64, SessionState>,
    pub pairing: Vec<PendingPair>,
    pub confirms: Vec<PendingConfirm>,
    /// Pairings paused for asking too often, until the user resumes them.
    pub paused: HashSet<Uuid>,
    /// What each pairing said of its site access when it connected last.
    pub page_access: HashMap<Uuid, bool>,
    /// The version each pairing said it was when it connected last (when it named one).
    pub versions: HashMap<Uuid, String>,
    pub activity: VecDeque<Activity>,
    pub unlock_asked: Option<Instant>,
}

impl BrowserRuntime {
    /// The vault locked: every session ends, every waiting request is refused, the activity list
    /// empties. Paused pairings stay paused.
    pub(crate) fn locked(&mut self) {
        self.sessions.clear();
        // Dropping the senders tells the waiting requests no.
        self.pairing.clear();
        self.confirms.clear();
        self.activity.clear();
    }

    pub(crate) fn record(&mut self, entry: Activity) {
        self.activity.push_front(entry);
        self.activity.truncate(MAX_ACTIVITY);
    }

    pub(crate) fn connected(&self, pairing: Uuid) -> bool {
        self.sessions.values().any(|s| s.pairing == pairing)
    }
}

/// One line of the activity list, as kept.
#[derive(Debug, Clone)]
pub(crate) struct Activity {
    pub at_ms: u64,
    pub browser: String,
    pub label: String,
    pub site: String,
    pub event: ActivityEvent,
    pub refusal: Option<Refusal>,
    pub item_id: Option<Uuid>,
    pub title: Option<String>,
}

impl Activity {
    pub(crate) fn view(&self) -> ActivityView {
        ActivityView {
            at_ms: self.at_ms,
            browser: self.browser.clone(),
            label: self.label.clone(),
            site: self.site.clone(),
            event: self.event,
            refusal: self.refusal,
            item_id: self.item_id,
            title: self.title.clone(),
        }
    }
}

/// A paired extension's session.
#[derive(Debug)]
pub(crate) struct SessionState {
    pub pairing: Uuid,
    pub client: ClientInfo,
    /// When it queried, the last minute.
    pub queries: VecDeque<Instant>,
    /// When it got a secret, the last minute.
    pub secrets: VecDeque<Instant>,
    /// The sites it got secrets for, the last ten minutes.
    pub sites: VecDeque<(Instant, String)>,
    /// The logins it filled, with the site and when: a second step may get their codes.
    pub filled: Vec<(Uuid, String, Instant)>,
    /// The fills the user asked for, with the site and when: their next step may get the
    /// password, once.
    pub steps: Vec<(Uuid, String, Instant)>,
}

impl SessionState {
    pub(crate) fn new(pairing: Uuid, client: ClientInfo) -> Self {
        Self { pairing, client, queries: VecDeque::new(), secrets: VecDeque::new(), sites: VecDeque::new(), filled: Vec::new(), steps: Vec::new() }
    }

    /// Count a query at `now`; false once over the limit.
    pub(crate) fn query(&mut self, now: Instant) -> bool {
        prune(&mut self.queries, now, MINUTE);
        self.queries.push_back(now);
        self.queries.len() <= QUERIES_PER_MINUTE
    }

    /// Count a secret given for `site` at `now`; false once over a limit.
    pub(crate) fn secret(&mut self, site: &str, now: Instant) -> bool {
        prune(&mut self.secrets, now, MINUTE);
        while self.sites.front().is_some_and(|(at, _)| now.duration_since(*at) >= TEN_MINUTES) {
            self.sites.pop_front();
        }
        self.secrets.push_back(now);
        if !self.sites.iter().any(|(_, s)| s == site) {
            self.sites.push_back((now, site.to_owned()));
        }
        self.secrets.len() <= FILLS_PER_MINUTE && self.sites.len() <= SITES_PER_TEN_MINUTES
    }

    /// `item` was filled on `site` at `now`.
    pub(crate) fn filled(&mut self, item: Uuid, site: &str, now: Instant) {
        self.filled.retain(|(id, s, at)| !(*id == item && s == site) && now.duration_since(*at) < OTP_FOLLOW_UP);
        self.filled.push((item, site.to_owned(), now));
    }

    /// `item` was filled on `site` in the last [`OTP_FOLLOW_UP`].
    pub(crate) fn filled_lately(&self, item: Uuid, site: &str, now: Instant) -> bool {
        self.filled.iter().any(|(id, s, at)| *id == item && s == site && now.duration_since(*at) < OTP_FOLLOW_UP)
    }

    /// The user asked for `item` on `site` at `now`: its next step may come.
    pub(crate) fn stepped(&mut self, item: Uuid, site: &str, now: Instant) {
        self.steps.retain(|(id, s, at)| !(*id == item && s == site) && now.duration_since(*at) < NEXT_STEP_FOR);
        self.steps.push((item, site.to_owned(), now));
    }

    /// The next step of a fill of `item` on `site` in the last [`NEXT_STEP_FOR`], taken: once.
    pub(crate) fn take_step(&mut self, item: Uuid, site: &str, now: Instant) -> bool {
        let before = self.steps.len();
        self.steps.retain(|(id, s, at)| !(*id == item && s == site && now.duration_since(*at) < NEXT_STEP_FOR));
        self.steps.len() < before
    }
}

fn prune(times: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while times.front().is_some_and(|at| now.duration_since(*at) >= window) {
        times.pop_front();
    }
}

/// How a pairing request ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PairOutcome {
    /// Approved: the new pairing's id.
    Approved(Uuid),
    /// Denied.
    Denied,
}

/// A pairing request waiting for the user.
#[derive(Debug)]
pub(crate) struct PendingPair {
    pub id: Uuid,
    pub client: ClientInfo,
    pub public_key: [u8; 32],
    pub code: String,
    pub expires_at: Instant,
    pub expires_at_ms: u64,
    pub outcome: Option<oneshot::Sender<PairOutcome>>,
}

impl PendingPair {
    pub(crate) fn view(&self) -> PairRequestView {
        PairRequestView {
            id: self.id,
            code: self.code.clone(),
            browser: self.client.browser.clone(),
            label: self.client.label.clone(),
            extension: self.client.extension.clone(),
            expires_at_ms: self.expires_at_ms,
        }
    }
}

/// A request to fill or copy a login, waiting for the user.
#[derive(Debug)]
pub(crate) struct PendingConfirm {
    pub view: ConfirmView,
    pub session: u64,
    pub expires_at: Instant,
    pub outcome: Option<oneshot::Sender<bool>>,
}

// ---- the decisions --------------------------------------------------------------------------------

/// A page address the extension sent: an `http` or `https` URL of a reasonable size.
pub(crate) fn page(text: &str) -> Result<WebUrl, Refusal> {
    if text.chars().count() > MAX_URL_CHARS {
        return Err(Refusal::Invalid);
    }
    WebUrl::parse(text).ok_or(Refusal::Invalid)
}

/// lockra-autofill's rule for a saved URL's.
pub(crate) fn rule(matching: Matching) -> lockra_autofill::Matching {
    match matching {
        Matching::Domain => lockra_autofill::Matching::Domain,
        Matching::Host => lockra_autofill::Matching::Host,
        Matching::StartsWith => lockra_autofill::Matching::StartsWith,
        Matching::Regex => lockra_autofill::Matching::Regex,
        Matching::Exact => lockra_autofill::Matching::Exact,
        Matching::Never => lockra_autofill::Matching::Never,
    }
}

/// The logins the extensions may see: logins (not items of a later kind) whose protection gives
/// the extension something.
pub(crate) fn offered(item: &VaultItem) -> bool {
    item.kind == Lenient::Known(ItemKind::Login) && item.protection_level() != Protection::NoExtension
}

/// The user's equivalent-domain groups, with the bundled ones.
pub(crate) fn equivalents(data: &VaultData) -> EquivalentDomains {
    EquivalentDomains::new(data.site_rules.iter().filter(|r| r.kind == Lenient::Known(SiteRuleKind::Equivalent)).map(|r| r.domains.clone()))
}

/// Whether a site rule of `kind` covers `page`: one of its domains is the page's host or a
/// parent of it.
pub(crate) fn listed(data: &VaultData, kind: SiteRuleKind, page: &WebUrl) -> bool {
    let host = page.host();
    data.site_rules.iter().filter(|r| r.kind == Lenient::Known(kind)).flat_map(|r| r.domains.iter()).any(|domain| covering(domain, host).is_some())
}

/// The host a site rule's domain names, normalized as a page's is (`例え.jp` and
/// `xn--r8jz45g.jp` are one); none for text that names no host.
pub(crate) fn rule_host(domain: &str) -> Option<String> {
    WebUrl::parse_saved(domain).map(|url| url.host().to_owned())
}

/// The length of the host a site rule's `domain` names, when that is `host` or a parent of it:
/// the longer, the closer the rule.
pub(crate) fn covering(domain: &str, host: &str) -> Option<usize> {
    let listed = rule_host(domain)?;
    (host == listed || host.strip_suffix(listed.as_str()).is_some_and(|rest| rest.ends_with('.'))).then_some(listed.len())
}

/// What the `match_host` and `match_domain` rules among `rules` say of `page`: by the full host
/// (`true`) or by the site, as the rule of the longest domain that is the page's host or a parent
/// of it says; of one domain under both kinds, which two devices may leave until they sync, the
/// later rule (the full host when they were made at once). None when no such rule covers the page.
pub(crate) fn host_rule<'a>(rules: impl IntoIterator<Item = &'a SiteRule>, page: &WebUrl) -> Option<bool> {
    let host = page.host();
    rules
        .into_iter()
        .filter_map(|rule| match rule.kind {
            Lenient::Known(SiteRuleKind::MatchHost) => Some((rule, true)),
            Lenient::Known(SiteRuleKind::MatchDomain) => Some((rule, false)),
            _ => None,
        })
        .flat_map(|(rule, by_host)| rule.domains.iter().filter_map(move |domain| covering(domain, host).map(|length| (length, rule.stamp, by_host))))
        .max()
        .map(|(_, _, by_host)| by_host)
}

/// How far the domain rule reaches on `page`: as the closest `match_host` or `match_domain` rule
/// says ([`host_rule`]), else as the policy's default; the port as the policy says.
pub(crate) fn domain_scope(data: &VaultData, policy: &BrowserPolicy, page: &WebUrl) -> DomainScope {
    policy.scope(host_rule(&data.site_rules, page).unwrap_or(policy.match_host))
}

/// The frame is in a page of another site: the frame, or one of its ancestors, is not of the
/// page's site.
pub(crate) fn cross_origin(frame: &WebUrl, top: &WebUrl, chain: &[WebUrl]) -> bool {
    !frame.same_site(top) || chain.iter().any(|ancestor| !ancestor.same_site(top))
}

/// The best matches for `page` among the offered logins, the likeliest first: favourites, then the
/// most recently used, then by title. The domain rule reaches as far as [`domain_scope`] says.
pub(crate) fn matches(data: &VaultData, page: &WebUrl, policy: &BrowserPolicy) -> (Option<MatchLevel>, Vec<(Uuid, bool)>) {
    let equivalents = equivalents(data);
    let scope = domain_scope(data, policy, page);
    let items = data.items.iter().filter(|i| offered(i)).map(|i| (i.id, i.urls.iter().map(|u| SavedUrl { url: &u.url, matching: rule(u.rule()) })));
    let Some(best) = lockra_autofill::best_matches(page, items, &equivalents, scope) else { return (None, Vec::new()) };
    let mut found: Vec<(Uuid, bool)> = best.items.iter().map(|m| (m.item, m.downgrade)).collect();
    found.sort_by_cached_key(|(id, _)| {
        let item = data.items.iter().find(|i| i.id == *id);
        let favorite = item.is_some_and(|i| i.favorite);
        let used = item.and_then(|i| i.last_used_at_ms).unwrap_or(0);
        let title = item.map(|i| i.title.to_lowercase()).unwrap_or_default();
        (!favorite, std::cmp::Reverse(used), title)
    });
    (Some(best.level), found)
}

/// An item's own choice, or the policy's.
pub(crate) fn choose(own: &Lenient<Tri>, policy: bool) -> bool {
    match own.or(Tri::Default) {
        Tri::Default => policy,
        Tri::On => true,
        Tri::Off => false,
    }
}

/// The site a request is about, for the activity list and the rate limit: the page's registrable
/// domain (its host for an address).
pub(crate) fn site_of(page: &WebUrl) -> String {
    page.site().to_owned()
}

#[cfg(test)]
mod tests;
