//! `otpauth://` URIs: Google's Key Uri Format, read leniently and written strictly.
//!
//! Reading follows what generators in the wild produce: the scheme and parameter names in any
//! case, `+` for a space in parameter values (form encoding), the issuer/account separator as a
//! literal or an encoded colon, and defaults for every optional parameter. Writing always emits a
//! literal separator, encodes everything outside RFC 3986's unreserved set, and spells out the
//! algorithm, digits and period, so that the result reads back to exactly the same account.

use std::fmt;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::base32::{self, Base32Error};
use crate::code::{Algorithm, Digits, Period, hotp, totp};

/// What drives the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OtpKind {
    /// Time-based (RFC 6238).
    Totp {
        /// The time step.
        period: Period,
    },
    /// Counter-based (RFC 4226).
    Hotp {
        /// The counter of the next code.
        counter: u64,
    },
}

/// One account as an `otpauth://` URI carries it.
#[derive(Clone, PartialEq, Eq)]
pub struct OtpAuth {
    /// TOTP or HOTP.
    pub kind: OtpKind,
    /// The HMAC hash.
    pub algorithm: Algorithm,
    /// Digits per code.
    pub digits: Digits,
    /// The shared secret, raw bytes.
    pub secret: Zeroizing<Vec<u8>>,
    /// Who issued the account (`GitHub`); empty when the URI names none.
    pub issuer: String,
    /// The account name (`alice@example.com`); may be empty.
    pub account: String,
}

impl fmt::Debug for OtpAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OtpAuth")
            .field("kind", &self.kind)
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("secret", &format_args!("<{} bytes>", self.secret.len()))
            .field("issuer", &self.issuer)
            .field("account", &self.account)
            .finish()
    }
}

/// Why a string is not a usable `otpauth://` URI.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UriError {
    /// The scheme is not `otpauth://`.
    #[error("not an otpauth:// URI")]
    NotOtpauth,
    /// The type is neither `totp` nor `hotp` (`steam`, a typo, …).
    #[error("unknown OTP type {0:?}")]
    UnknownType(String),
    /// No `secret=` parameter.
    #[error("the URI has no secret")]
    MissingSecret,
    /// The secret is not Base32.
    #[error("the secret is not Base32: {0}")]
    InvalidSecret(#[from] Base32Error),
    /// `algorithm=` names something other than SHA1, SHA256 or SHA512.
    #[error("unsupported algorithm {0:?}")]
    UnsupportedAlgorithm(String),
    /// `digits=` is not 6, 7 or 8.
    #[error("invalid digits {0:?}")]
    InvalidDigits(String),
    /// `period=` is not 1 to 3600 seconds.
    #[error("invalid period {0:?}")]
    InvalidPeriod(String),
    /// `counter=` is not a non-negative integer.
    #[error("invalid counter {0:?}")]
    InvalidCounter(String),
}

const SCHEME: &str = "otpauth://";

/// Everything outside RFC 3986's unreserved characters is percent-encoded when writing.
const ENCODE: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// Parse an `otpauth://totp/…` or `otpauth://hotp/…` URI.
pub fn parse(input: &str) -> Result<OtpAuth, UriError> {
    let input = input.trim();
    let rest = strip_prefix_ignore_ascii_case(input, SCHEME).ok_or(UriError::NotOtpauth)?;
    let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
    let type_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (type_name, rest) = rest.split_at(type_end);
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let (raw_label, query) = rest.split_once('?').unwrap_or((rest, ""));

    let mut params = Params::default();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.set(&decode_form(key).to_ascii_lowercase(), decode_form(value));
    }

    let secret_text = params.secret.ok_or(UriError::MissingSecret)?;
    let secret = base32::decode(&secret_text)?;
    let algorithm = match params.algorithm.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        None => Algorithm::Sha1,
        Some(name) => Algorithm::from_uri_name(name).ok_or_else(|| UriError::UnsupportedAlgorithm(name.to_owned()))?,
    };
    let digits = match params.digits.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        None => Digits::default(),
        Some(text) => text.parse::<u8>().ok().and_then(|n| Digits::new(n).ok()).ok_or_else(|| UriError::InvalidDigits(text.to_owned()))?,
    };
    let kind = match type_name.to_ascii_lowercase().as_str() {
        "totp" => {
            let period = match params.period.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                None => Period::default(),
                Some(text) => text.parse::<u32>().ok().and_then(|n| Period::new(n).ok()).ok_or_else(|| UriError::InvalidPeriod(text.to_owned()))?,
            };
            OtpKind::Totp { period }
        }
        "hotp" => {
            let counter = match params.counter.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                None => 0,
                Some(text) => text.parse::<u64>().map_err(|_| UriError::InvalidCounter(text.to_owned()))?,
            };
            OtpKind::Hotp { counter }
        }
        _ => return Err(UriError::UnknownType(type_name.to_owned())),
    };

    let issuer_param = params.issuer.map(|i| i.trim().to_owned()).filter(|i| !i.is_empty());
    let (label_issuer, account) = split_label(raw_label, issuer_param.as_deref());
    let issuer = issuer_param.unwrap_or(label_issuer);
    Ok(OtpAuth { kind, algorithm, digits, secret, issuer, account })
}

impl OtpAuth {
    /// The URI that [`parse`] reads back to exactly this account.
    pub fn to_uri(&self) -> Zeroizing<String> {
        let secret = base32::encode(&self.secret);
        let issuer = utf8_percent_encode(&self.issuer, ENCODE).to_string();
        let account = utf8_percent_encode(&self.account, ENCODE).to_string();
        let (type_name, tail) = match self.kind {
            OtpKind::Totp { period } => ("totp", format!("&period={}", period.get())),
            OtpKind::Hotp { counter } => ("hotp", format!("&counter={counter}")),
        };
        // Sized up front so the buffer holding the secret is never reallocated (and left behind).
        let mut uri = Zeroizing::new(String::with_capacity(64 + 2 * issuer.len() + account.len() + secret.len() + tail.len()));
        uri.push_str(SCHEME);
        uri.push_str(type_name);
        uri.push('/');
        if !issuer.is_empty() {
            uri.push_str(&issuer);
            uri.push(':');
        }
        uri.push_str(&account);
        uri.push_str("?secret=");
        uri.push_str(&secret);
        if !issuer.is_empty() {
            uri.push_str("&issuer=");
            uri.push_str(&issuer);
        }
        uri.push_str("&algorithm=");
        uri.push_str(self.algorithm.uri_name());
        uri.push_str("&digits=");
        uri.push_str(&self.digits.get().to_string());
        uri.push_str(&tail);
        uri
    }

    /// The code at `unix_ms` for TOTP, or the code of the current counter for HOTP.
    pub fn code(&self, unix_ms: u64) -> String {
        match self.kind {
            OtpKind::Totp { period } => totp(&self.secret, unix_ms, period, self.algorithm, self.digits),
            OtpKind::Hotp { counter } => hotp(&self.secret, counter, self.algorithm, self.digits),
        }
    }
}

#[derive(Default)]
struct Params {
    secret: Option<Zeroizing<String>>,
    issuer: Option<String>,
    algorithm: Option<String>,
    digits: Option<String>,
    period: Option<String>,
    counter: Option<String>,
}

impl Params {
    /// The first occurrence of a parameter wins; unknown parameters (`image=`, …) are ignored.
    fn set(&mut self, key: &str, value: String) {
        let slot = match key {
            "secret" => {
                if self.secret.is_none() {
                    self.secret = Some(Zeroizing::new(value));
                }
                return;
            }
            "issuer" => &mut self.issuer,
            "algorithm" => &mut self.algorithm,
            "digits" => &mut self.digits,
            "period" => &mut self.period,
            "counter" => &mut self.counter,
            _ => return,
        };
        if slot.is_none() {
            *slot = Some(value);
        }
    }
}

/// Split a raw (still encoded) label into the issuer prefix and the account name.
///
/// A literal `:` is the separator. Without one, an encoded colon (`%3A`) separates only when the
/// decoded text starts with the `issuer=` value followed by a colon (the Key Uri Format says the
/// two must agree); otherwise the colon belongs to the account name. This keeps
/// `otpauth://totp/Example%3Aalice?issuer=Example` and an account literally named `a:b` apart.
fn split_label(raw: &str, issuer_param: Option<&str>) -> (String, String) {
    if let Some((issuer, account)) = raw.split_once(':') {
        return (decode_path(issuer).trim().to_owned(), decode_path(account).trim().to_owned());
    }
    let decoded = decode_path(raw);
    if let Some(issuer) = issuer_param
        && let Some(account) = decoded.strip_prefix(issuer).and_then(|rest| rest.strip_prefix(':'))
    {
        return (issuer.to_owned(), account.trim().to_owned());
    }
    (String::new(), decoded.trim().to_owned())
}

/// Percent-decoding with path semantics: `+` is a plus sign.
fn decode_path(text: &str) -> String {
    percent_decode_str(text).decode_utf8_lossy().into_owned()
}

/// Percent-decoding with form semantics: `+` is a space (what `urlencode` writes into queries).
fn decode_form(text: &str) -> String {
    let spaced = text.replace('+', " ");
    percent_decode_str(&spaced).decode_utf8_lossy().into_owned()
}

fn strip_prefix_ignore_ascii_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn secret(text: &str) -> Zeroizing<Vec<u8>> {
        base32::decode(text).unwrap()
    }

    #[test]
    fn google_key_uri_example() {
        let auth = parse("otpauth://totp/Example:alice@google.com?secret=JBSWY3DPEHPK3PXP&issuer=Example").unwrap();
        assert_eq!(auth.issuer, "Example");
        assert_eq!(auth.account, "alice@google.com");
        assert_eq!(auth.kind, OtpKind::Totp { period: Period::THIRTY });
        assert_eq!(auth.algorithm, Algorithm::Sha1);
        assert_eq!(auth.digits, Digits::SIX);
        assert_eq!(auth.secret, secret("JBSWY3DPEHPK3PXP"));
    }

    #[test]
    fn every_parameter_is_read() {
        let auth =
            parse("otpauth://totp/ACME%20Co:john.doe%40email.com?secret=HXDMVJECJJWSRB3HWIZR4IFUGFTMXBOZ&issuer=ACME%20Co&algorithm=SHA512&digits=8&period=60")
                .unwrap();
        assert_eq!(auth.issuer, "ACME Co");
        assert_eq!(auth.account, "john.doe@email.com");
        assert_eq!(auth.algorithm, Algorithm::Sha512);
        assert_eq!(auth.digits, Digits::EIGHT);
        assert_eq!(auth.kind, OtpKind::Totp { period: Period::new(60).unwrap() });
    }

    #[test]
    fn hotp_reads_the_counter_and_defaults_it() {
        let auth = parse("otpauth://hotp/Bank:me?secret=GEZDGNBV&counter=42").unwrap();
        assert_eq!(auth.kind, OtpKind::Hotp { counter: 42 });
        assert_eq!(auth.issuer, "Bank");
        assert_eq!(parse("otpauth://hotp/me?secret=GEZDGNBV").unwrap().kind, OtpKind::Hotp { counter: 0 });
        assert_eq!(parse("otpauth://hotp/me?secret=GEZDGNBV&counter=-1").unwrap_err(), UriError::InvalidCounter("-1".into()));
    }

    #[test]
    fn issuer_parameter_wins_over_the_label_prefix() {
        let auth = parse("otpauth://totp/Old:alice?secret=GEZDGNBV&issuer=New").unwrap();
        assert_eq!(auth.issuer, "New");
        assert_eq!(auth.account, "alice");
        let auth = parse("otpauth://totp/Prefix:%20alice?secret=GEZDGNBV").unwrap();
        assert_eq!((auth.issuer.as_str(), auth.account.as_str()), ("Prefix", "alice"));
    }

    #[test]
    fn encoded_colon_separates_only_when_the_issuer_agrees() {
        let auth = parse("otpauth://totp/Example%3Aalice?secret=GEZDGNBV&issuer=Example").unwrap();
        assert_eq!((auth.issuer.as_str(), auth.account.as_str()), ("Example", "alice"));
        let auth = parse("otpauth://totp/a%3Ab?secret=GEZDGNBV").unwrap();
        assert_eq!((auth.issuer.as_str(), auth.account.as_str()), ("", "a:b"));
        let auth = parse("otpauth://totp/a%3Ab?secret=GEZDGNBV&issuer=Other").unwrap();
        assert_eq!((auth.issuer.as_str(), auth.account.as_str()), ("Other", "a:b"));
    }

    #[test]
    fn plus_is_a_space_in_parameters_but_not_in_the_label() {
        let auth = parse("otpauth://totp/C%2B%2B+Club:bob?secret=GEZDGNBV&issuer=My+Company").unwrap();
        assert_eq!(auth.issuer, "My Company");
        assert_eq!(auth.account, "bob");
        let auth = parse("otpauth://totp/a+b?secret=GEZDGNBV").unwrap();
        assert_eq!(auth.account, "a+b");
    }

    #[test]
    fn scheme_type_and_names_are_case_insensitive() {
        let auth = parse("  OTPAUTH://TOTP/x?SECRET=gezdgnbv&Digits=7&ALGORITHM=sha256  ").unwrap();
        assert_eq!(auth.digits.get(), 7);
        assert_eq!(auth.algorithm, Algorithm::Sha256);
    }

    #[test]
    fn missing_label_and_fragment_are_tolerated() {
        let auth = parse("otpauth://totp?secret=GEZDGNBV&issuer=Solo").unwrap();
        assert_eq!((auth.issuer.as_str(), auth.account.as_str()), ("Solo", ""));
        let auth = parse("otpauth://totp/x?secret=GEZDGNBV#ignored").unwrap();
        assert_eq!(auth.account, "x");
    }

    #[test]
    fn first_occurrence_wins_and_unknown_parameters_are_ignored() {
        let auth = parse("otpauth://totp/x?secret=GEZDGNBV&secret=MZXW6&image=https%3A%2F%2Fe.x%2Fa.png&digits=8&digits=6").unwrap();
        assert_eq!(auth.secret, secret("GEZDGNBV"));
        assert_eq!(auth.digits, Digits::EIGHT);
    }

    #[test]
    fn invalid_uris_are_refused() {
        assert_eq!(parse("https://example.com").unwrap_err(), UriError::NotOtpauth);
        assert_eq!(parse("otpauth:/totp/x?secret=GEZDGNBV").unwrap_err(), UriError::NotOtpauth);
        assert_eq!(parse("otpauth://steam/x?secret=GEZDGNBV").unwrap_err(), UriError::UnknownType("steam".into()));
        assert_eq!(parse("otpauth://totp/x?issuer=a").unwrap_err(), UriError::MissingSecret);
        assert_eq!(parse("otpauth://totp/x?secret=").unwrap_err(), UriError::InvalidSecret(Base32Error::Empty));
        assert_eq!(parse("otpauth://totp/x?secret=GEZ0").unwrap_err(), UriError::InvalidSecret(Base32Error::InvalidCharacter('0')));
        assert_eq!(parse("otpauth://totp/x?secret=GEZDGNBV&algorithm=MD5").unwrap_err(), UriError::UnsupportedAlgorithm("MD5".into()));
        assert_eq!(parse("otpauth://totp/x?secret=GEZDGNBV&digits=10").unwrap_err(), UriError::InvalidDigits("10".into()));
        assert_eq!(parse("otpauth://totp/x?secret=GEZDGNBV&digits=six").unwrap_err(), UriError::InvalidDigits("six".into()));
        assert_eq!(parse("otpauth://totp/x?secret=GEZDGNBV&period=0").unwrap_err(), UriError::InvalidPeriod("0".into()));
    }

    #[test]
    fn empty_optional_parameters_fall_back_to_defaults() {
        let auth = parse("otpauth://totp/x?secret=GEZDGNBV&algorithm=&digits=&period=&issuer=").unwrap();
        assert_eq!((auth.algorithm, auth.digits, auth.kind), (Algorithm::Sha1, Digits::SIX, OtpKind::Totp { period: Period::THIRTY }));
        assert_eq!(auth.issuer, "");
    }

    #[test]
    fn written_uri_is_strict_and_complete() {
        let auth = OtpAuth {
            kind: OtpKind::Totp { period: Period::THIRTY },
            algorithm: Algorithm::Sha1,
            digits: Digits::SIX,
            secret: secret("JBSWY3DPEHPK3PXP"),
            issuer: "ACME Co".into(),
            account: "a:b@x.y".into(),
        };
        assert_eq!(auth.to_uri().as_str(), "otpauth://totp/ACME%20Co:a%3Ab%40x.y?secret=JBSWY3DPEHPK3PXP&issuer=ACME%20Co&algorithm=SHA1&digits=6&period=30");
        assert_eq!(parse(&auth.to_uri()).unwrap(), auth);
    }

    #[test]
    fn code_follows_the_kind() {
        let totp_auth = parse("otpauth://totp/x?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8").unwrap();
        assert_eq!(totp_auth.code(59_000), "94287082");
        let hotp_auth = parse("otpauth://hotp/x?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&counter=1").unwrap();
        assert_eq!(hotp_auth.code(0), "287082");
    }

    #[test]
    fn debug_never_prints_the_secret() {
        let auth = parse("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP").unwrap();
        let printed = format!("{auth:?}");
        assert!(printed.contains("<10 bytes>"), "{printed}");
        assert!(!printed.contains("72, 101"), "{printed}");
    }

    fn text() -> impl Strategy<Value = String> {
        proptest::string::string_regex("[a-zA-Z0-9 :@._+%&=/?#~-]{0,16}|[\\p{Han}é ü]{0,6}").unwrap().prop_map(|s| s.trim().to_owned())
    }

    fn kind() -> impl Strategy<Value = OtpKind> {
        prop_oneof![(1u32..=3600).prop_map(|p| OtpKind::Totp { period: Period::new(p).unwrap() }), any::<u64>().prop_map(|counter| OtpKind::Hotp { counter }),]
    }

    proptest! {
        #[test]
        fn parse_inverts_to_uri(
            issuer in text(),
            account in text(),
            bytes in proptest::collection::vec(any::<u8>(), 1..64),
            kind in kind(),
            algorithm in prop_oneof![Just(Algorithm::Sha1), Just(Algorithm::Sha256), Just(Algorithm::Sha512)],
            digits in 6u8..=8,
        ) {
            let auth = OtpAuth { kind, algorithm, digits: Digits::new(digits).unwrap(), secret: Zeroizing::new(bytes), issuer, account };
            prop_assert_eq!(parse(&auth.to_uri()).unwrap(), auth);
        }
    }
}
