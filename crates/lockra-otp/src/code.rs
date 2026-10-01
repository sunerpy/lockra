//! The code itself: HMAC, dynamic truncation (RFC 4226 §5.3) and the TOTP time step (RFC 6238 §4).

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};

/// The HMAC hash behind a code. RFC 6238 §1.2 names these three; anything else (MD5 in Google's
/// migration format) is refused where it enters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Algorithm {
    /// HMAC-SHA-1: what nearly every service uses.
    #[default]
    Sha1,
    /// HMAC-SHA-256.
    Sha256,
    /// HMAC-SHA-512.
    Sha512,
}

impl Algorithm {
    /// The spelling `otpauth://` URIs use.
    pub fn uri_name(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha512 => "SHA512",
        }
    }

    /// An `algorithm=` value; any case, and a `-` or `_` inside (`sha-256`), is accepted.
    pub fn from_uri_name(name: &str) -> Option<Self> {
        let normalized: String = name.chars().filter(|c| *c != '-' && *c != '_').collect::<String>().to_ascii_uppercase();
        match normalized.as_str() {
            "SHA1" => Some(Self::Sha1),
            "SHA256" => Some(Self::Sha256),
            "SHA512" => Some(Self::Sha512),
            _ => None,
        }
    }
}

/// How many digits a code has: 6 to 8. Six is what nearly every service uses, eight is what
/// Microsoft's own accounts use, and no phone app accepts more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Digits(u8);

/// A digit count outside 6..=8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a code has 6 to 8 digits, not {0}")]
pub struct InvalidDigits(pub u64);

impl Digits {
    /// The fewest digits accepted.
    pub const MIN: u8 = 6;
    /// The most digits accepted.
    pub const MAX: u8 = 8;
    /// Six digits, the default.
    pub const SIX: Self = Self(6);
    /// Eight digits.
    pub const EIGHT: Self = Self(8);

    /// `Ok` for 6, 7 or 8.
    pub fn new(value: u8) -> Result<Self, InvalidDigits> {
        if (Self::MIN..=Self::MAX).contains(&value) { Ok(Self(value)) } else { Err(InvalidDigits(u64::from(value))) }
    }

    /// The count as a number.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl Default for Digits {
    fn default() -> Self {
        Self::SIX
    }
}

impl TryFrom<u8> for Digits {
    type Error = InvalidDigits;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Digits> for u8 {
    fn from(digits: Digits) -> Self {
        digits.0
    }
}

/// The TOTP time step in seconds: 1 to 3600, 30 by default (RFC 6238 §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Period(u32);

/// A period outside 1..=3600 seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a TOTP period is 1 to 3600 seconds, not {0}")]
pub struct InvalidPeriod(pub u64);

impl Period {
    /// The longest period accepted, in seconds.
    pub const MAX: u32 = 3600;
    /// Thirty seconds, the default and the only period Google Authenticator's migration format keeps.
    pub const THIRTY: Self = Self(30);

    /// `Ok` for 1 to 3600 seconds.
    pub fn new(seconds: u32) -> Result<Self, InvalidPeriod> {
        if (1..=Self::MAX).contains(&seconds) { Ok(Self(seconds)) } else { Err(InvalidPeriod(u64::from(seconds))) }
    }

    /// The period in seconds.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl Default for Period {
    fn default() -> Self {
        Self::THIRTY
    }
}

impl TryFrom<u32> for Period {
    type Error = InvalidPeriod;

    fn try_from(seconds: u32) -> Result<Self, Self::Error> {
        Self::new(seconds)
    }
}

impl From<Period> for u32 {
    fn from(period: Period) -> Self {
        period.0
    }
}

/// The time step a moment falls into, and the half-open window `[valid_from_ms, valid_until_ms)`
/// its code is valid for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TotpWindow {
    /// `T` of RFC 6238 §4.2: the HOTP counter for this window.
    pub counter: u64,
    /// Start of the window, Unix milliseconds.
    pub valid_from_ms: u64,
    /// End of the window (exclusive), Unix milliseconds.
    pub valid_until_ms: u64,
}

/// The window `unix_ms` falls into for `period` (T0 = 0).
pub fn totp_window(unix_ms: u64, period: Period) -> TotpWindow {
    let step_ms = u64::from(period.get()) * 1000;
    let counter = unix_ms / step_ms;
    TotpWindow { counter, valid_from_ms: counter * step_ms, valid_until_ms: (counter + 1) * step_ms }
}

/// The HOTP code for `counter` (RFC 4226 §5.3), zero-padded to `digits`.
pub fn hotp(secret: &[u8], counter: u64, algorithm: Algorithm, digits: Digits) -> String {
    let message = counter.to_be_bytes();
    let value = match algorithm {
        Algorithm::Sha1 => truncate(&mac::<Hmac<sha1::Sha1>>(secret, &message)),
        Algorithm::Sha256 => truncate(&mac::<Hmac<sha2::Sha256>>(secret, &message)),
        Algorithm::Sha512 => truncate(&mac::<Hmac<sha2::Sha512>>(secret, &message)),
    };
    let width = usize::from(digits.get());
    format!("{:0width$}", value % 10u32.pow(u32::from(digits.get())))
}

/// The TOTP code at `unix_ms` (RFC 6238 §4): HOTP of the window's counter.
pub fn totp(secret: &[u8], unix_ms: u64, period: Period, algorithm: Algorithm, digits: Digits) -> String {
    hotp(secret, totp_window(unix_ms, period).counter, algorithm, digits)
}

fn mac<M: Mac + KeyInit>(key: &[u8], message: &[u8]) -> Vec<u8> {
    // HMAC takes a key of any length (RFC 2104 §2), so `new_from_slice` cannot fail here.
    #[allow(clippy::expect_used)]
    let mut mac = <M as KeyInit>::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

/// Dynamic truncation: the low nibble of the last byte picks four bytes, the top bit is masked.
fn truncate(digest: &[u8]) -> u32 {
    let offset = usize::from(digest[digest.len() - 1] & 0x0f);
    u32::from_be_bytes([digest[offset] & 0x7f, digest[offset + 1], digest[offset + 2], digest[offset + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC4226_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn hotp_matches_rfc4226_appendix_d() {
        let expected = ["755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583", "399871", "520489"];
        for (counter, code) in expected.iter().enumerate() {
            assert_eq!(hotp(RFC4226_SECRET, counter as u64, Algorithm::Sha1, Digits::SIX), *code, "counter {counter}");
        }
    }

    #[test]
    fn totp_matches_rfc6238_appendix_b() {
        let sha1 = b"12345678901234567890".as_slice();
        let sha256 = b"12345678901234567890123456789012".as_slice();
        let sha512 = b"1234567890123456789012345678901234567890123456789012345678901234".as_slice();
        let vectors: [(u64, &str, &str, &str); 6] = [
            (59, "94287082", "46119246", "90693936"),
            (1_111_111_109, "07081804", "68084774", "25091201"),
            (1_111_111_111, "14050471", "67062674", "99943326"),
            (1_234_567_890, "89005924", "91819424", "93441116"),
            (2_000_000_000, "69279037", "90698825", "38618901"),
            (20_000_000_000, "65353130", "77737706", "47863826"),
        ];
        for (seconds, code_sha1, code_sha256, code_sha512) in vectors {
            let ms = seconds * 1000;
            assert_eq!(totp(sha1, ms, Period::THIRTY, Algorithm::Sha1, Digits::EIGHT), code_sha1, "SHA1 at {seconds}");
            assert_eq!(totp(sha256, ms, Period::THIRTY, Algorithm::Sha256, Digits::EIGHT), code_sha256, "SHA256 at {seconds}");
            assert_eq!(totp(sha512, ms, Period::THIRTY, Algorithm::Sha512, Digits::EIGHT), code_sha512, "SHA512 at {seconds}");
        }
    }

    #[test]
    fn seven_digits_are_zero_padded() {
        // 1111111109 s, SHA1: the eight-digit code is 07081804, so seven digits keep the zero.
        let code = totp(RFC4226_SECRET, 1_111_111_109_000, Period::THIRTY, Algorithm::Sha1, Digits::new(7).unwrap());
        assert_eq!(code, "7081804");
        assert_eq!(code.len(), 7);
    }

    #[test]
    fn window_is_half_open_and_aligned() {
        let w = totp_window(59_999, Period::THIRTY);
        assert_eq!(w, TotpWindow { counter: 1, valid_from_ms: 30_000, valid_until_ms: 60_000 });
        let next = totp_window(60_000, Period::THIRTY);
        assert_eq!(next.counter, 2);
        assert_eq!(next.valid_from_ms, w.valid_until_ms);
        assert_eq!(totp_window(0, Period::new(60).unwrap()), TotpWindow { counter: 0, valid_from_ms: 0, valid_until_ms: 60_000 });
    }

    #[test]
    fn digits_and_period_reject_out_of_range() {
        assert_eq!(Digits::new(5), Err(InvalidDigits(5)));
        assert_eq!(Digits::new(9), Err(InvalidDigits(9)));
        assert_eq!(Digits::new(8).map(Digits::get), Ok(8));
        assert_eq!(Period::new(0), Err(InvalidPeriod(0)));
        assert_eq!(Period::new(3601), Err(InvalidPeriod(3601)));
        assert_eq!(Period::new(3600).map(Period::get), Ok(3600));
        assert_eq!(Digits::default(), Digits::SIX);
        assert_eq!(Period::default(), Period::THIRTY);
    }

    #[test]
    fn serde_validates_digits_and_period() {
        assert_eq!(serde_json::to_string(&Digits::EIGHT).unwrap(), "8");
        assert_eq!(serde_json::from_str::<Digits>("7").unwrap().get(), 7);
        assert!(serde_json::from_str::<Digits>("10").is_err());
        assert_eq!(serde_json::from_str::<Period>("60").unwrap().get(), 60);
        assert!(serde_json::from_str::<Period>("0").is_err());
        assert_eq!(serde_json::to_string(&Algorithm::Sha256).unwrap(), "\"sha256\"");
        assert_eq!(serde_json::from_str::<Algorithm>("\"sha512\"").unwrap(), Algorithm::Sha512);
    }

    #[test]
    fn algorithm_names_round_trip_and_tolerate_spelling() {
        for algorithm in [Algorithm::Sha1, Algorithm::Sha256, Algorithm::Sha512] {
            assert_eq!(Algorithm::from_uri_name(algorithm.uri_name()), Some(algorithm));
        }
        assert_eq!(Algorithm::from_uri_name("sha-256"), Some(Algorithm::Sha256));
        assert_eq!(Algorithm::from_uri_name("Sha_512"), Some(Algorithm::Sha512));
        assert_eq!(Algorithm::from_uri_name("MD5"), None);
        assert_eq!(Algorithm::from_uri_name(""), None);
    }
}
