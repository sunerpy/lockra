//! Logins in other apps' exports (docs/formats.md §11): what the readers of the browsers', the
//! password managers' and KeePass's files yield, before the core makes them vault items. A login
//! holds secrets (its password, notes and hidden fields): its `Debug` leaves them out.

use std::fmt;

use lockra_otp::{Algorithm, Digits, OtpAuth, OtpKind, Period, base32, uri};
use zeroize::Zeroizing;

use crate::item::{Item, RejectReason, display_label};

/// The most bytes a compressed file of an import expands to (1Password's zip, a KeePass
/// database's payload): as much as an import reads of a file.
pub const MAX_EXPANDED_BYTES: usize = 64 * 1024 * 1024;

/// The app whose export a login came from: the vault keeps its [`App::name`] as the login's
/// source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum App {
    /// Chrome's and Edge's password export (CSV).
    Chrome,
    /// Firefox's (CSV).
    Firefox,
    /// Safari's and Apple Passwords' (CSV).
    Safari,
    /// Bitwarden's, as CSV or unencrypted JSON.
    Bitwarden,
    /// 1Password's, as 1PUX or CSV.
    OnePassword,
    /// LastPass's (CSV).
    LastPass,
    /// A KeePass database (KDBX 4).
    KeePass,
}

impl App {
    /// The name a login imported from it keeps as its source.
    pub fn name(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Firefox => "firefox",
            Self::Safari => "safari",
            Self::Bitwarden => "bitwarden",
            Self::OnePassword => "1password",
            Self::LastPass => "lastpass",
            Self::KeePass => "kdbx",
        }
    }
}

/// How a saved URL is compared with the page, where the source says (Bitwarden's URI match
/// detection); a URL without one matches by its registrable domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlMatch {
    /// The same registrable domain.
    Domain,
    /// The same host and port.
    Host,
    /// The page's URL starts with it.
    StartsWith,
    /// The same URL.
    Exact,
    /// A regular expression.
    Regex,
    /// Never.
    Never,
}

/// A URL saved with an exported login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginUrl {
    /// As the source wrote it, trimmed.
    pub url: String,
    /// The source's own rule for it, when it has one.
    pub matching: Option<UrlMatch>,
}

impl LoginUrl {
    /// A URL with no rule of its source's.
    pub fn plain(url: &str) -> Self {
        Self { url: url.trim().to_owned(), matching: None }
    }
}

/// A field of an exported login besides its user name and password.
#[derive(Clone, PartialEq, Eq)]
pub struct LoginField {
    /// Its name.
    pub label: String,
    /// Its value.
    pub value: Zeroizing<String>,
    /// The source keeps it masked (a password-like field): it stays hidden.
    pub hidden: bool,
}

impl fmt::Debug for LoginField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value: &dyn fmt::Debug = if self.hidden { &"<hidden>" } else { &*self.value };
        f.debug_struct("LoginField").field("label", &self.label).field("value", value).field("hidden", &self.hidden).finish()
    }
}

/// A login found in an export, as its source has it.
#[derive(Clone, PartialEq, Eq)]
pub struct Login {
    /// Which app's export it is.
    pub app: App,
    /// Its name.
    pub title: String,
    /// The user name.
    pub username: String,
    /// The password.
    pub password: Zeroizing<String>,
    /// The sites it is for, in the source's order.
    pub urls: Vec<LoginUrl>,
    /// Notes.
    pub notes: Zeroizing<String>,
    /// Its folder or group: a path such as `Work/Servers` where the source nests them.
    pub folder: Option<String>,
    /// Marked as a favourite.
    pub favorite: bool,
    /// Tags.
    pub tags: Vec<String>,
    /// Other fields, in the source's order.
    pub fields: Vec<LoginField>,
    /// The source asks for the master password again before showing it (Bitwarden's re-prompt,
    /// 1Password's guarded fields).
    pub reprompt: bool,
    /// Its one-time codes: the account, or why it cannot be imported.
    pub totp: Option<Item>,
}

impl Login {
    /// An empty login of `app`, to be filled in.
    pub(crate) fn new(app: App) -> Self {
        Self {
            app,
            title: String::new(),
            username: String::new(),
            password: Zeroizing::new(String::new()),
            urls: Vec::new(),
            notes: Zeroizing::new(String::new()),
            folder: None,
            favorite: false,
            tags: Vec::new(),
            fields: Vec::new(),
            reprompt: false,
            totp: None,
        }
    }

    /// Add `url` unless it is blank.
    pub(crate) fn add_url(&mut self, url: &str, matching: Option<UrlMatch>) {
        let url = url.trim();
        if !url.is_empty() {
            self.urls.push(LoginUrl { url: url.to_owned(), matching });
        }
    }

    /// Add a field unless its value is empty.
    pub(crate) fn add_field(&mut self, label: &str, value: &str, hidden: bool) {
        if !value.is_empty() {
            self.fields.push(LoginField { label: label.trim().to_owned(), value: Zeroizing::new(value.to_owned()), hidden });
        }
    }
}

impl fmt::Debug for Login {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Login")
            .field("app", &self.app)
            .field("title", &self.title)
            .field("username", &self.username)
            .field("password", &format_args!("<{} bytes>", self.password.len()))
            .field("urls", &self.urls)
            .field("notes", &format_args!("<{} bytes>", self.notes.len()))
            .field("folder", &self.folder)
            .field("favorite", &self.favorite)
            .field("tags", &self.tags)
            .field("fields", &self.fields)
            .field("reprompt", &self.reprompt)
            .field("totp", &self.totp)
            .finish()
    }
}

/// One item of an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// A login.
    Login(Box<Login>),
    /// An item the import leaves out: its name, and why.
    Left {
        /// What the source calls it.
        label: String,
        /// Why.
        reason: RejectReason,
    },
}

impl Found {
    pub(crate) fn left(label: &str, reason: RejectReason) -> Self {
        Self::Left { label: label.trim().to_owned(), reason }
    }
}

/// The code a login carries: an `otpauth://` URI, or a bare Base32 secret (Bitwarden, LastPass,
/// 1Password allow one), read as TOTP, SHA1, six digits every 30 s. An account the URI leaves
/// unnamed takes the login's title and user name; a code that does not read is listed with the
/// reason, under the login's name. Nothing when `text` is blank.
pub(crate) fn totp(text: &str, title: &str, username: &str) -> Option<Item> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let rejected = |reason| Item::Rejected { label: display_label(title.trim(), username.trim()), line: None, reason };
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("otpauth://") {
        return Some(match uri::parse(text) {
            Ok(auth) => Item::Account(named(auth, title, username)),
            Err(error) => rejected(RejectReason::from(&error)),
        });
    }
    if lower.contains("://") {
        // `steam://…`, `otpauth-migration://…`: codes of another kind than TOTP and HOTP.
        return Some(rejected(RejectReason::UnknownType));
    }
    Some(match base32::decode(text) {
        Ok(secret) => Item::Account(account(secret, title, username)),
        Err(_) => rejected(RejectReason::InvalidSecret),
    })
}

/// A TOTP account of `secret` with the default parameters, named after the login.
pub(crate) fn account(secret: Zeroizing<Vec<u8>>, title: &str, username: &str) -> OtpAuth {
    OtpAuth {
        kind: OtpKind::Totp { period: Period::THIRTY },
        algorithm: Algorithm::Sha1,
        digits: Digits::SIX,
        secret,
        issuer: title.trim().to_owned(),
        account: username.trim().to_owned(),
    }
}

/// `auth` with the login's title and user name where it names no issuer or account.
pub(crate) fn named(mut auth: OtpAuth, title: &str, username: &str) -> OtpAuth {
    if auth.issuer.trim().is_empty() {
        auth.issuer = title.trim().to_owned();
    }
    if auth.account.trim().is_empty() {
        auth.account = username.trim().to_owned();
    }
    auth
}

/// `Some` of the trimmed text, `None` when it is blank.
pub(crate) fn non_blank(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Tags written in one cell: separated by commas, semicolons or tabs, trimmed, the blank ones and
/// the repeated ones left out.
pub(crate) fn tags(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in text.split([',', ';', '\t']).map(str::trim).filter(|t| !t.is_empty()) {
        if !out.iter().any(|t| t == tag) {
            out.push(tag.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account_of(item: Option<Item>) -> OtpAuth {
        match item {
            Some(Item::Account(auth)) => auth,
            other => panic!("not an account: {other:?}"),
        }
    }

    #[test]
    fn a_login_s_code_is_a_uri_or_a_bare_secret() {
        let uri = account_of(totp(" otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP&digits=8 ", "GH", "me"));
        assert_eq!((uri.issuer.as_str(), uri.account.as_str(), uri.digits), ("GitHub", "octocat", Digits::EIGHT));
        // A URI that names nobody takes the login's names.
        let unnamed = account_of(totp("otpauth://totp/?secret=JBSWY3DPEHPK3PXP", "Mail", "me@example.com"));
        assert_eq!((unnamed.issuer.as_str(), unnamed.account.as_str()), ("Mail", "me@example.com"));
        let bare = account_of(totp("jbsw y3dp ehpk 3pxp", " Mail ", "me"));
        assert_eq!((bare.issuer.as_str(), bare.account.as_str(), bare.kind), ("Mail", "me", OtpKind::Totp { period: Period::THIRTY }));
        assert_eq!(bare.secret.as_slice(), b"Hello!\xde\xad\xbe\xef");
        assert_eq!(totp("  ", "x", "y"), None);
    }

    #[test]
    fn a_code_that_does_not_read_is_named_after_the_login() {
        let rejected = |text: &str| totp(text, "Steam", "gamer");
        let reason = |text: &str| match rejected(text) {
            Some(Item::Rejected { label, line: None, reason }) => {
                assert_eq!(label, "Steam: gamer");
                reason
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(reason("steam://ABCDEFGH"), RejectReason::UnknownType);
        assert_eq!(reason("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=12"), RejectReason::UnsupportedDigits);
        assert_eq!(reason("not base32 at all!"), RejectReason::InvalidSecret);
    }

    #[test]
    fn tags_split_on_commas_and_semicolons() {
        assert_eq!(tags(" work, home;work ;; "), ["work", "home"]);
        assert!(tags("").is_empty());
        assert_eq!(non_blank("  x "), Some("x".into()));
        assert_eq!(non_blank("   "), None);
    }

    #[test]
    fn secrets_stay_out_of_debug_output() {
        let mut login = Login::new(App::Bitwarden);
        login.password = Zeroizing::new("hunter2".into());
        login.notes = Zeroizing::new("my notes".into());
        login.add_field("PIN", "4242", true);
        login.add_field("Customer", "C-77", false);
        login.add_field("Empty", "", false);
        login.add_url("  ", None);
        login.add_url(" https://example.com ", Some(UrlMatch::Host));
        let text = format!("{login:?}");
        for secret in ["hunter2", "my notes", "4242"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
        assert!(text.contains("C-77"), "a field that is not hidden shows");
        assert_eq!(login.fields.len(), 2);
        assert_eq!(login.urls, [LoginUrl { url: "https://example.com".into(), matching: Some(UrlMatch::Host) }]);
        assert_eq!(LoginUrl::plain(" a "), LoginUrl { url: "a".into(), matching: None });
        let names: Vec<&str> =
            [App::Chrome, App::Firefox, App::Safari, App::Bitwarden, App::OnePassword, App::LastPass, App::KeePass].iter().map(|a| a.name()).collect();
        assert_eq!(names, ["chrome", "firefox", "safari", "bitwarden", "1password", "lastpass", "kdbx"]);
        assert_eq!(Found::left(" Card ", RejectReason::CardItem), Found::Left { label: "Card".into(), reason: RejectReason::CardItem });
    }
}
