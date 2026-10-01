//! Google Authenticator's "Transfer accounts" QR codes:
//! `otpauth-migration://offline?data=<percent-encoded Base64 of a MigrationPayload protobuf>`.
//!
//! The message layout is the one documented by the projects that read these codes (field numbers
//! and enum values in docs/formats.md). It carries no period, so only 30-second TOTP and HOTP
//! accounts travel through it, and it knows six and eight digits only.

use data_encoding::BASE64;
use lockra_otp::{Algorithm, Digits, OtpAuth, OtpKind, Period};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use prost::Message;
use qrcode::{EcLevel, QrCode, Version};
use zeroize::{Zeroize, Zeroizing};

use crate::encoding::decode_base64;
use crate::item::{Incompatible, Item, RejectReason, display_label};

const PREFIX: &str = "otpauth-migration://offline?";
/// The most accounts Google's own export puts into one code.
pub const MAX_PER_CODE: usize = 10;
/// The densest code Lockra emits for a batch: version 20 (97 × 97 modules) at level M still
/// scans from a laptop screen with a phone camera.
pub const MAX_VERSION: i16 = 20;

const ALGORITHM_SHA1: i32 = 1;
const ALGORITHM_SHA256: i32 = 2;
const ALGORITHM_SHA512: i32 = 3;
const ALGORITHM_MD5: i32 = 4;
const DIGITS_SIX: i32 = 1;
const DIGITS_EIGHT: i32 = 2;
const TYPE_HOTP: i32 = 1;
const TYPE_TOTP: i32 = 2;

#[derive(Clone, PartialEq, Message)]
struct MigrationPayload {
    #[prost(message, repeated, tag = "1")]
    otp_parameters: Vec<OtpParameters>,
    #[prost(int32, tag = "2")]
    version: i32,
    #[prost(int32, tag = "3")]
    batch_size: i32,
    #[prost(int32, optional, tag = "4")]
    batch_index: Option<i32>,
    #[prost(int32, tag = "5")]
    batch_id: i32,
}

#[derive(Clone, PartialEq, Message)]
struct OtpParameters {
    #[prost(bytes = "vec", tag = "1")]
    secret: Vec<u8>,
    #[prost(string, tag = "2")]
    name: String,
    #[prost(string, tag = "3")]
    issuer: String,
    #[prost(int32, tag = "4")]
    algorithm: i32,
    #[prost(int32, tag = "5")]
    digits: i32,
    #[prost(int32, tag = "6")]
    otp_type: i32,
    #[prost(int64, tag = "7")]
    counter: i64,
}

impl Drop for OtpParameters {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

/// Why a migration URI could not be read at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MigrationError {
    /// The text is not an `otpauth-migration://offline?` URI.
    #[error("not an otpauth-migration URI")]
    NotMigration,
    /// No `data=` parameter.
    #[error("the URI has no data")]
    MissingData,
    /// `data` is not Base64, or not the protobuf message.
    #[error("the data is damaged")]
    Damaged,
}

/// Which code of an export this was: Google splits a large export into several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Batch {
    /// Shared by all codes of one export.
    pub id: i32,
    /// Zero-based position of this code.
    pub index: u32,
    /// How many codes the export has.
    pub size: u32,
}

/// One decoded code: its place in the export and its accounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    /// Its place in the export.
    pub batch: Batch,
    /// The accounts, importable or not.
    pub items: Vec<Item>,
}

/// Read one `otpauth-migration://` URI.
pub fn decode(uri: &str) -> Result<Migration, MigrationError> {
    let uri = uri.trim();
    let query = uri.get(..PREFIX.len()).filter(|head| head.eq_ignore_ascii_case(PREFIX)).map(|_| &uri[PREFIX.len()..]).ok_or(MigrationError::NotMigration)?;
    let data = query
        .split('&')
        .find_map(|pair| pair.split_once('=').filter(|(key, _)| key.eq_ignore_ascii_case("data")).map(|(_, value)| value))
        .ok_or(MigrationError::MissingData)?;
    let text = Zeroizing::new(percent_decode_str(data).decode_utf8_lossy().into_owned());
    let bytes = decode_base64(&text).ok_or(MigrationError::Damaged)?;
    let payload = MigrationPayload::decode(bytes.as_slice()).map_err(|_| MigrationError::Damaged)?;
    let batch = Batch {
        id: payload.batch_id,
        index: u32::try_from(payload.batch_index.unwrap_or(0)).unwrap_or(0),
        size: u32::try_from(payload.batch_size).unwrap_or(1).max(1),
    };
    let items = payload.otp_parameters.iter().map(to_item).collect();
    Ok(Migration { batch, items })
}

fn to_item(p: &OtpParameters) -> Item {
    // Google writes the account as the label it scanned: `Issuer:account` when the issuer field is
    // empty is split the same way an otpauth label is.
    let (issuer, account) = match (p.issuer.trim(), p.name.split_once(':')) {
        ("", Some((prefix, rest))) => (prefix.trim().to_owned(), rest.trim().to_owned()),
        (issuer, _) => (issuer.to_owned(), p.name.trim().to_owned()),
    };
    let reject = |reason| Item::Rejected { label: display_label(&issuer, &account), line: None, reason };
    // proto3 drops fields at their zero value, so an absent algorithm or digit count reads as
    // "unspecified"; the otpauth defaults (SHA1, six digits, TOTP) apply, as they would to a URI.
    let algorithm = match p.algorithm {
        0 | ALGORITHM_SHA1 => Algorithm::Sha1,
        ALGORITHM_SHA256 => Algorithm::Sha256,
        ALGORITHM_SHA512 => Algorithm::Sha512,
        ALGORITHM_MD5 => return reject(RejectReason::Md5Algorithm),
        _ => return reject(RejectReason::UnknownAlgorithm),
    };
    let digits = match p.digits {
        0 | DIGITS_SIX => Digits::SIX,
        DIGITS_EIGHT => Digits::EIGHT,
        _ => return reject(RejectReason::UnsupportedDigits),
    };
    let kind = match p.otp_type {
        0 | TYPE_TOTP => OtpKind::Totp { period: Period::THIRTY },
        TYPE_HOTP => match u64::try_from(p.counter) {
            Ok(counter) => OtpKind::Hotp { counter },
            Err(_) => return reject(RejectReason::InvalidCounter),
        },
        _ => return reject(RejectReason::UnknownType),
    };
    if p.secret.is_empty() {
        return reject(RejectReason::EmptySecret);
    }
    Item::Account(OtpAuth { kind, algorithm, digits, secret: Zeroizing::new(p.secret.clone()), issuer, account })
}

/// `Ok` when the account survives Google's format unchanged.
pub fn exportable(auth: &OtpAuth) -> Result<(), Incompatible> {
    if let OtpKind::Totp { period } = auth.kind
        && period != Period::THIRTY
    {
        return Err(Incompatible::PeriodNot30);
    }
    if auth.digits != Digits::SIX && auth.digits != Digits::EIGHT {
        return Err(Incompatible::DigitsNot6Or8);
    }
    Ok(())
}

/// One code of an export: its URI and which of the input accounts it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    /// The `otpauth-migration://` URI the QR code holds.
    pub uri: Zeroizing<String>,
    /// Indices into the accounts passed to [`encode`].
    pub accounts: Vec<usize>,
}

/// The codes for `accounts`, all of which must be [`exportable`]: at most [`MAX_PER_CODE`] per
/// code, and fewer when the code would be denser than [`MAX_VERSION`] at level M. An account that
/// does not fit any code on its own is reported with its index.
pub fn encode(accounts: &[&OtpAuth]) -> Result<Vec<Code>, (usize, Incompatible)> {
    if let Some((index, reason)) = accounts.iter().enumerate().find_map(|(i, a)| exportable(a).err().map(|e| (i, e))) {
        return Err((index, reason));
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut start = 0;
    while start < accounts.len() {
        let mut take = MAX_PER_CODE.min(accounts.len() - start);
        loop {
            let indices: Vec<usize> = (start..start + take).collect();
            // Sized with the longest varints the batch fields can take, so the real code is never denser.
            match qr_version(&uri_for(accounts, &indices, i32::MAX, i32::MAX, i32::MAX)) {
                Some(version) if version <= MAX_VERSION || take == 1 => {
                    groups.push(indices);
                    break;
                }
                None if take == 1 => return Err((start, Incompatible::TooLarge)),
                _ => take -= 1,
            }
        }
        start += take;
    }
    let size = i32::try_from(groups.len()).unwrap_or(i32::MAX);
    let id = random_batch_id();
    Ok(groups
        .into_iter()
        .enumerate()
        .map(|(index, indices)| Code { uri: uri_for(accounts, &indices, i32::try_from(index).unwrap_or(0), size, id), accounts: indices })
        .collect())
}

fn uri_for(accounts: &[&OtpAuth], indices: &[usize], batch_index: i32, batch_size: i32, batch_id: i32) -> Zeroizing<String> {
    let payload = MigrationPayload {
        otp_parameters: indices.iter().map(|&i| parameters(accounts[i])).collect(),
        version: 1,
        batch_size,
        batch_index: Some(batch_index),
        batch_id,
    };
    let bytes = Zeroizing::new(payload.encode_to_vec());
    let base64 = Zeroizing::new(BASE64.encode(&bytes));
    Zeroizing::new(format!("{PREFIX}data={}", utf8_percent_encode(&base64, DATA_ENCODE)))
}

fn parameters(auth: &OtpAuth) -> OtpParameters {
    let (otp_type, counter) = match auth.kind {
        OtpKind::Totp { .. } => (TYPE_TOTP, 0),
        OtpKind::Hotp { counter } => (TYPE_HOTP, i64::try_from(counter).unwrap_or(i64::MAX)),
    };
    OtpParameters {
        secret: auth.secret.to_vec(),
        name: auth.account.clone(),
        issuer: auth.issuer.clone(),
        algorithm: match auth.algorithm {
            Algorithm::Sha1 => ALGORITHM_SHA1,
            Algorithm::Sha256 => ALGORITHM_SHA256,
            Algorithm::Sha512 => ALGORITHM_SHA512,
        },
        digits: if auth.digits == Digits::EIGHT { DIGITS_EIGHT } else { DIGITS_SIX },
        otp_type,
        counter,
    }
}

fn qr_version(data: &str) -> Option<i16> {
    match QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M).ok()?.version() {
        Version::Normal(v) => Some(v),
        Version::Micro(_) => Some(0),
    }
}

/// A positive id, as Google's own exports carry.
fn random_batch_id() -> i32 {
    let mut bytes = [0u8; 4];
    if getrandom::fill(&mut bytes).is_err() {
        return 1;
    }
    (i32::from_le_bytes(bytes) & i32::MAX).max(1)
}

/// `+`, `/` and `=` must not reach the query raw, or a form decoder would turn `+` into a space.
const DATA_ENCODE: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

#[cfg(test)]
mod tests {
    use lockra_otp::base32;

    use super::*;

    fn account(issuer: &str, name: &str, kind: OtpKind, digits: Digits) -> OtpAuth {
        OtpAuth { kind, algorithm: Algorithm::Sha1, digits, secret: base32::decode("JBSWY3DPEHPK3PXP").unwrap(), issuer: issuer.into(), account: name.into() }
    }

    fn totp(issuer: &str, name: &str) -> OtpAuth {
        account(issuer, name, OtpKind::Totp { period: Period::THIRTY }, Digits::SIX)
    }

    /// A payload written byte by byte from the field numbers, independent of prost's encoder: one
    /// TOTP account (secret "Hello!\xde\xad\xbe\xef", name "alice@google.com", issuer "Example",
    /// SHA1, six digits), version 1, batch 2 of 3, batch id 12345.
    fn handmade_payload() -> Vec<u8> {
        let secret = b"Hello!\xde\xad\xbe\xef";
        let mut otp = vec![0x0a, secret.len() as u8];
        otp.extend_from_slice(secret);
        otp.extend_from_slice(&[0x12, 16]);
        otp.extend_from_slice(b"alice@google.com");
        otp.extend_from_slice(&[0x1a, 7]);
        otp.extend_from_slice(b"Example");
        otp.extend_from_slice(&[0x20, 0x01, 0x28, 0x01, 0x30, 0x02]);
        let mut payload = vec![0x0a, otp.len() as u8];
        payload.extend_from_slice(&otp);
        // version = 1, batch_size = 3, batch_index = 1, batch_id = 12345 (varint 0xb9 0x60).
        payload.extend_from_slice(&[0x10, 0x01, 0x18, 0x03, 0x20, 0x01, 0x28, 0xb9, 0x60]);
        payload
    }

    fn uri_of(payload: &[u8]) -> String {
        format!("{PREFIX}data={}", utf8_percent_encode(&BASE64.encode(payload), DATA_ENCODE))
    }

    #[test]
    fn decodes_a_handmade_payload() {
        let migration = decode(&uri_of(&handmade_payload())).unwrap();
        assert_eq!(migration.batch, Batch { id: 12345, index: 1, size: 3 });
        let [Item::Account(auth)] = migration.items.as_slice() else { panic!("{:?}", migration.items) };
        assert_eq!((auth.issuer.as_str(), auth.account.as_str()), ("Example", "alice@google.com"));
        assert_eq!(auth.secret.as_slice(), b"Hello!\xde\xad\xbe\xef");
        assert_eq!((auth.kind, auth.algorithm, auth.digits), (OtpKind::Totp { period: Period::THIRTY }, Algorithm::Sha1, Digits::SIX));
    }

    #[test]
    fn prost_writes_what_the_handmade_reader_expects() {
        let payload = MigrationPayload {
            otp_parameters: vec![OtpParameters {
                secret: b"Hello!\xde\xad\xbe\xef".to_vec(),
                name: "alice@google.com".into(),
                issuer: "Example".into(),
                algorithm: ALGORITHM_SHA1,
                digits: DIGITS_SIX,
                otp_type: TYPE_TOTP,
                counter: 0,
            }],
            version: 1,
            batch_size: 3,
            batch_index: Some(1),
            batch_id: 12345,
        };
        assert_eq!(payload.encode_to_vec(), handmade_payload());
    }

    #[test]
    fn data_survives_every_way_it_gets_mangled() {
        let encoded = BASE64.encode(&handmade_payload());
        let url_safe = encoded.replace('+', "-").replace('/', "_").trim_end_matches('=').to_owned();
        let spaced = encoded.replace('+', " ");
        for data in [utf8_percent_encode(&encoded, DATA_ENCODE).to_string(), encoded.clone(), url_safe, spaced] {
            let uri = format!("  OTPAUTH-MIGRATION://offline?data={data}&extra=1  ");
            assert_eq!(decode(&uri).unwrap().batch.id, 12345, "{uri}");
        }
    }

    #[test]
    fn issuer_comes_out_of_the_name_when_the_field_is_empty() {
        let mut p = OtpParameters::default();
        p.secret = vec![1; 10];
        p.name = "GitHub: octocat".into();
        p.otp_type = TYPE_TOTP;
        let Item::Account(auth) = to_item(&p) else { panic!() };
        assert_eq!((auth.issuer.as_str(), auth.account.as_str()), ("GitHub", "octocat"));
    }

    #[test]
    fn unsupported_parameters_are_rejected_with_their_reason() {
        let mut base = OtpParameters::default();
        base.secret = vec![1; 10];
        base.name = "n".into();
        base.issuer = "i".into();
        let with = |edit: fn(&mut OtpParameters)| {
            let mut p = base.clone();
            edit(&mut p);
            p
        };
        let cases = [
            (with(|p| p.algorithm = ALGORITHM_MD5), RejectReason::Md5Algorithm),
            (with(|p| p.algorithm = 9), RejectReason::UnknownAlgorithm),
            (with(|p| p.digits = 3), RejectReason::UnsupportedDigits),
            (with(|p| p.otp_type = 7), RejectReason::UnknownType),
            (
                with(|p| {
                    p.otp_type = TYPE_HOTP;
                    p.counter = -1;
                }),
                RejectReason::InvalidCounter,
            ),
            (with(|p| p.secret.clear()), RejectReason::EmptySecret),
        ];
        for (p, reason) in cases {
            assert_eq!(to_item(&p), Item::Rejected { label: "i: n".into(), line: None, reason });
        }
        // Absent (zero) fields take the otpauth defaults.
        let Item::Account(auth) = to_item(&base) else { panic!() };
        assert_eq!((auth.kind, auth.algorithm, auth.digits), (OtpKind::Totp { period: Period::THIRTY }, Algorithm::Sha1, Digits::SIX));
    }

    #[test]
    fn broken_uris_are_refused() {
        assert_eq!(decode("otpauth://totp/x?secret=A").unwrap_err(), MigrationError::NotMigration);
        assert_eq!(decode("otpauth-migration://offline?foo=bar").unwrap_err(), MigrationError::MissingData);
        assert_eq!(decode("otpauth-migration://offline?data=%%%").unwrap_err(), MigrationError::Damaged);
        assert_eq!(decode("otpauth-migration://offline?data=AAAA%FF").unwrap_err(), MigrationError::Damaged);
    }

    #[test]
    fn encode_then_decode_returns_the_accounts() {
        let hotp = account("Bank", "me", OtpKind::Hotp { counter: 7 }, Digits::EIGHT);
        let mut sha512 = totp("Mail", "you");
        sha512.algorithm = Algorithm::Sha512;
        let accounts = [totp("GitHub", "octocat"), hotp, sha512];
        let refs: Vec<&OtpAuth> = accounts.iter().collect();
        let codes = encode(&refs).unwrap();
        assert_eq!(codes.len(), 1);
        let data = codes[0].uri.split_once("data=").unwrap().1;
        assert!(!data.contains(['+', '/', '=']), "{data}");
        let migration = decode(&codes[0].uri).unwrap();
        assert_eq!((migration.batch.index, migration.batch.size), (0, 1));
        assert!(migration.batch.id > 0);
        let decoded: Vec<OtpAuth> = migration.items.into_iter().map(|i| if let Item::Account(a) = i { a } else { panic!() }).collect();
        assert_eq!(decoded, accounts);
    }

    #[test]
    fn large_exports_are_split_by_count_and_by_density() {
        // Short names: ten fit one code, so the count decides.
        let many: Vec<OtpAuth> = (0..23).map(|i| totp("S", &format!("u{i}"))).collect();
        let refs: Vec<&OtpAuth> = many.iter().collect();
        let codes = encode(&refs).unwrap();
        assert_eq!(codes.iter().map(|c| c.accounts.len()).collect::<Vec<_>>(), [10, 10, 3]);
        let ids: Vec<i32> = codes.iter().map(|c| decode(&c.uri).unwrap().batch.id).collect();
        assert!(ids.windows(2).all(|w| w[0] == w[1]), "one batch id per export");
        for (i, code) in codes.iter().enumerate() {
            let batch = decode(&code.uri).unwrap().batch;
            assert_eq!((batch.index, batch.size), (i as u32, 3));
            assert!(qr_version(&code.uri).unwrap() <= MAX_VERSION);
        }
        // Long names: fewer than ten per code, still version 20 at most, nothing lost.
        let wide: Vec<OtpAuth> = (0..10).map(|i| totp(&"Long issuer name ".repeat(4), &format!("{}{i}", "a".repeat(60)))).collect();
        let refs: Vec<&OtpAuth> = wide.iter().collect();
        let codes = encode(&refs).unwrap();
        assert!(codes.len() > 1);
        assert_eq!(codes.iter().map(|c| c.accounts.len()).sum::<usize>(), 10);
        assert!(codes.iter().all(|c| qr_version(&c.uri).unwrap() <= MAX_VERSION));
    }

    #[test]
    fn incompatible_accounts_are_named() {
        let sixty = account("x", "y", OtpKind::Totp { period: Period::new(60).unwrap() }, Digits::SIX);
        let seven = account("x", "y", OtpKind::Totp { period: Period::THIRTY }, Digits::new(7).unwrap());
        assert_eq!(exportable(&sixty), Err(Incompatible::PeriodNot30));
        assert_eq!(exportable(&seven), Err(Incompatible::DigitsNot6Or8));
        assert_eq!(exportable(&account("x", "y", OtpKind::Hotp { counter: 1 }, Digits::EIGHT)), Ok(()));
        let ok = totp("a", "b");
        assert_eq!(encode(&[&ok, &sixty]).unwrap_err(), (1, Incompatible::PeriodNot30));
        let huge = totp(&"x".repeat(4000), "y");
        assert_eq!(encode(&[&huge]).unwrap_err(), (0, Incompatible::TooLarge));
    }
}
