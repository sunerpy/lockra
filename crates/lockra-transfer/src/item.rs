//! What an import yields and why an account is left out of an import or an export.

use lockra_otp::{OtpAuth, UriError};
use serde::{Deserialize, Serialize};

/// Where an account came from; kept on the entry so the list can say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Typed in by hand.
    Manual,
    /// An `otpauth://` URI: pasted, in a list file, or in a QR code.
    Uri,
    /// Google Authenticator's "Transfer accounts" QR codes.
    Google,
    /// Microsoft Authenticator's PhoneFactor database.
    Microsoft,
    /// A Lockra backup.
    Backup,
}

/// Why an account in the source cannot be imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    /// The text is not an `otpauth://` or `otpauth-migration://` URI.
    NotOtpauth,
    /// HMAC-MD5 (Google's migration format allows it; RFC 6238 does not).
    Md5Algorithm,
    /// An algorithm other than SHA1, SHA256, SHA512.
    UnknownAlgorithm,
    /// Neither TOTP nor HOTP (Steam, a typo, an enum value this build does not know).
    UnknownType,
    /// A digit count other than 6, 7, 8.
    UnsupportedDigits,
    /// A TOTP period outside 1..=3600 seconds.
    InvalidPeriod,
    /// A HOTP counter that is not a non-negative integer.
    InvalidCounter,
    /// No secret at all.
    EmptySecret,
    /// A secret that does not decode.
    InvalidSecret,
    /// Microsoft Authenticator stores only the encrypted form of this secret.
    EncryptedSecret,
    /// A Microsoft account type that has no portable secret (work or school accounts).
    UnsupportedAccountType,
}

impl From<&UriError> for RejectReason {
    fn from(error: &UriError) -> Self {
        match error {
            UriError::NotOtpauth => Self::NotOtpauth,
            UriError::UnknownType(_) => Self::UnknownType,
            UriError::MissingSecret => Self::EmptySecret,
            UriError::InvalidSecret(lockra_otp::base32::Base32Error::Empty) => Self::EmptySecret,
            UriError::InvalidSecret(_) => Self::InvalidSecret,
            UriError::UnsupportedAlgorithm(name) if name.eq_ignore_ascii_case("md5") => Self::Md5Algorithm,
            UriError::UnsupportedAlgorithm(_) => Self::UnknownAlgorithm,
            UriError::InvalidDigits(_) => Self::UnsupportedDigits,
            UriError::InvalidPeriod(_) => Self::InvalidPeriod,
            UriError::InvalidCounter(_) => Self::InvalidCounter,
        }
    }
}

/// One account found in a source: importable, or not with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// Ready to import.
    Account(OtpAuth),
    /// Left out, with what the source calls it.
    Rejected {
        /// `issuer: account` when the source names the account; empty otherwise.
        label: String,
        /// The 1-based line of a text source. The line itself is never echoed: a broken URI may
        /// still hold a secret.
        line: Option<u32>,
        /// Why.
        reason: RejectReason,
    },
}

/// Why an account cannot go to an export target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Incompatible {
    /// Google's migration format has no period: only 30-second TOTP survives.
    #[serde(rename = "period_not_30")]
    PeriodNot30,
    /// Google's migration format knows six and eight digits only.
    #[serde(rename = "digits_not_6_or_8")]
    DigitsNot6Or8,
    /// Microsoft Authenticator computes six-digit codes for added accounts.
    #[serde(rename = "digits_not_6")]
    DigitsNot6,
    /// Microsoft Authenticator computes HMAC-SHA1 for added accounts.
    AlgorithmNotSha1,
    /// Microsoft Authenticator adds time-based accounts only.
    HotpNotSupported,
    /// The account alone does not fit one QR code (names thousands of characters long).
    TooLarge,
}

/// `issuer: account`, or whichever of the two exists.
pub fn display_label(issuer: &str, account: &str) -> String {
    match (issuer.is_empty(), account.is_empty()) {
        (false, false) => format!("{issuer}: {account}"),
        (false, true) => issuer.to_owned(),
        (true, _) => account.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use lockra_otp::base32::Base32Error;

    use super::*;

    #[test]
    fn uri_errors_map_to_reasons() {
        let cases = [
            (UriError::NotOtpauth, RejectReason::NotOtpauth),
            (UriError::UnknownType("steam".into()), RejectReason::UnknownType),
            (UriError::MissingSecret, RejectReason::EmptySecret),
            (UriError::InvalidSecret(Base32Error::Empty), RejectReason::EmptySecret),
            (UriError::InvalidSecret(Base32Error::TooShort), RejectReason::InvalidSecret),
            (UriError::UnsupportedAlgorithm("md5".into()), RejectReason::Md5Algorithm),
            (UriError::UnsupportedAlgorithm("SHA3".into()), RejectReason::UnknownAlgorithm),
            (UriError::InvalidDigits("9".into()), RejectReason::UnsupportedDigits),
            (UriError::InvalidPeriod("0".into()), RejectReason::InvalidPeriod),
            (UriError::InvalidCounter("-1".into()), RejectReason::InvalidCounter),
        ];
        for (error, reason) in cases {
            assert_eq!(RejectReason::from(&error), reason, "{error:?}");
        }
    }

    #[test]
    fn labels_use_whatever_names_exist() {
        assert_eq!(display_label("GitHub", "octocat"), "GitHub: octocat");
        assert_eq!(display_label("GitHub", ""), "GitHub");
        assert_eq!(display_label("", "octocat"), "octocat");
        assert_eq!(display_label("", ""), "");
    }

    #[test]
    fn wire_names_are_readable() {
        assert_eq!(serde_json::to_value(Incompatible::DigitsNot6Or8).unwrap(), "digits_not_6_or_8");
        assert_eq!(serde_json::to_value(RejectReason::Md5Algorithm).unwrap(), "md5_algorithm");
        assert_eq!(serde_json::to_value(Origin::Microsoft).unwrap(), "microsoft");
    }
}
