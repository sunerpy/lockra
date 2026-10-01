//! Entries: what the vault payload stores, and the secret-free view the webview receives.

use lockra_otp::{Algorithm, Digits, OtpAuth, OtpKind, base32};
use lockra_transfer::{Incompatible, Origin, google, microsoft};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;
use zeroize::Zeroizing;

/// The longest issuer, account or group name kept (characters).
pub const MAX_NAME_CHARS: usize = 200;

/// One account in the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Stable id.
    pub id: Uuid,
    /// Who issued it.
    pub issuer: String,
    /// The account name.
    pub account: String,
    /// TOTP with its period, or HOTP with the counter of the code shown now.
    pub kind: OtpKind,
    /// HMAC hash.
    pub algorithm: Algorithm,
    /// Digits per code.
    pub digits: Digits,
    /// The shared secret; Base32 inside the encrypted payload.
    #[serde(serialize_with = "secret_to_base32", deserialize_with = "secret_from_base32")]
    pub secret: Zeroizing<Vec<u8>>,
    /// Optional group (a folder name).
    pub group: Option<String>,
    /// Pinned to the top of the list.
    pub favorite: bool,
    /// Where it came from.
    pub origin: Origin,
    /// Unix milliseconds.
    pub created_at_ms: u64,
    /// Unix milliseconds.
    pub updated_at_ms: u64,
    /// Last copy, Unix milliseconds.
    pub last_used_at_ms: Option<u64>,
}

impl Entry {
    /// A new entry for an account.
    pub fn from_auth(auth: OtpAuth, origin: Origin, now_ms: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            issuer: clean_name(&auth.issuer),
            account: clean_name(&auth.account),
            kind: auth.kind,
            algorithm: auth.algorithm,
            digits: auth.digits,
            secret: auth.secret,
            group: None,
            favorite: false,
            origin,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            last_used_at_ms: None,
        }
    }

    /// The account in `otpauth://` form.
    pub fn to_auth(&self) -> OtpAuth {
        OtpAuth {
            kind: self.kind,
            algorithm: self.algorithm,
            digits: self.digits,
            secret: self.secret.clone(),
            issuer: self.issuer.clone(),
            account: self.account.clone(),
        }
    }

    /// Same secret and the same code parameters: the same account, whatever the names.
    pub fn same_account(&self, auth: &OtpAuth) -> bool {
        let same_kind = match (self.kind, auth.kind) {
            (OtpKind::Totp { period: a }, OtpKind::Totp { period: b }) => a == b,
            (OtpKind::Hotp { .. }, OtpKind::Hotp { .. }) => true,
            _ => false,
        };
        same_kind && self.algorithm == auth.algorithm && self.digits == auth.digits && self.secret == auth.secret
    }

    /// Same issuer and account name (case-insensitive) as `auth`.
    pub fn same_names(&self, auth: &OtpAuth) -> bool {
        self.issuer.to_lowercase() == clean_name(&auth.issuer).to_lowercase() && self.account.to_lowercase() == clean_name(&auth.account).to_lowercase()
    }

    /// The webview's view: everything but the secret.
    pub fn view(&self) -> EntryView {
        let auth = self.to_auth();
        EntryView {
            id: self.id,
            issuer: self.issuer.clone(),
            account: self.account.clone(),
            kind: self.kind,
            algorithm: self.algorithm,
            digits: self.digits,
            group: self.group.clone(),
            favorite: self.favorite,
            origin: self.origin,
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.updated_at_ms,
            last_used_at_ms: self.last_used_at_ms,
            export: ExportCompat { google: google::exportable(&auth).err(), microsoft: microsoft::exportable(&auth).err() },
        }
    }
}

/// An entry as the webview sees it: no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryView {
    /// Stable id.
    pub id: Uuid,
    /// Who issued it.
    pub issuer: String,
    /// The account name.
    pub account: String,
    /// TOTP with its period, or HOTP with its counter.
    pub kind: OtpKind,
    /// HMAC hash.
    pub algorithm: Algorithm,
    /// Digits per code.
    pub digits: Digits,
    /// Group.
    pub group: Option<String>,
    /// Pinned.
    pub favorite: bool,
    /// Where it came from.
    pub origin: Origin,
    /// Unix milliseconds.
    pub created_at_ms: u64,
    /// Unix milliseconds.
    pub updated_at_ms: u64,
    /// Last copy, Unix milliseconds.
    pub last_used_at_ms: Option<u64>,
    /// Why it cannot go to each export target; `null` when it can.
    pub export: ExportCompat,
}

/// Export compatibility per target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExportCompat {
    /// Google Authenticator's migration codes.
    pub google: Option<Incompatible>,
    /// Microsoft Authenticator's "add account" scan.
    pub microsoft: Option<Incompatible>,
}

/// An account typed in by hand.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EntryDraft {
    /// Who issued it.
    #[serde(default)]
    pub issuer: String,
    /// The account name.
    #[serde(default)]
    pub account: String,
    /// The secret as the service shows it (Base32).
    pub secret: Zeroizing<String>,
    /// TOTP or HOTP with its period or counter.
    pub kind: OtpKind,
    /// HMAC hash.
    #[serde(default)]
    pub algorithm: Algorithm,
    /// Digits per code.
    #[serde(default)]
    pub digits: Digits,
    /// Group.
    #[serde(default)]
    pub group: Option<String>,
}

/// What an edit may change: the names, the group and the pin. Secrets and code parameters are
/// fixed for the life of an entry (delete it and add it again to change them).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct EntryPatch {
    /// New issuer.
    pub issuer: Option<String>,
    /// New account name.
    pub account: Option<String>,
    /// New group; `Some("")` removes it.
    pub group: Option<String>,
    /// Pin or unpin.
    pub favorite: Option<bool>,
}

/// The decrypted payload of the vault.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultData {
    /// Payload format; 1.
    #[serde(default = "format_one")]
    pub format: u32,
    /// The accounts, in insertion order.
    #[serde(default)]
    pub entries: Vec<Entry>,
}

fn format_one() -> u32 {
    1
}

impl VaultData {
    /// The payload bytes for `lockra-vault`.
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        // Serializing plain data structures to JSON cannot fail.
        #[allow(clippy::expect_used)]
        Zeroizing::new(serde_json::to_vec(self).expect("vault data serializes"))
    }

    /// Parse a payload; `None` when it is not a Lockra payload.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    /// The entry with `id`.
    pub fn get(&self, id: Uuid) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The entry with `id`, mutably.
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }
}

/// Trimmed, control characters removed, at most [`MAX_NAME_CHARS`].
pub fn clean_name(name: &str) -> String {
    name.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect::<String>().trim().to_owned()
}

fn secret_to_base32<S: Serializer>(secret: &Zeroizing<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&base32::encode(secret))
}

fn secret_from_base32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Zeroizing<Vec<u8>>, D::Error> {
    let text = Zeroizing::new(String::deserialize(deserializer)?);
    base32::decode(&text).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use lockra_otp::{Period, uri};

    use super::*;

    fn auth(text: &str) -> OtpAuth {
        uri::parse(text).unwrap()
    }

    #[test]
    fn payload_round_trips_with_the_secret_in_base32() {
        let entry = Entry::from_auth(auth("otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 5);
        let data = VaultData { format: 1, entries: vec![entry.clone()] };
        let bytes = data.to_bytes();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("\"secret\":\"JBSWY3DPEHPK3PXP\""), "{text}");
        assert_eq!(VaultData::from_bytes(&bytes).unwrap(), data);
        assert!(VaultData::from_bytes(b"[1,2]").is_none());
        assert_eq!(VaultData::from_bytes(b"{}").unwrap(), VaultData { format: 1, entries: vec![] });
    }

    #[test]
    fn the_view_has_no_secret_and_names_export_limits() {
        let entry = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP&period=60&digits=8"), Origin::Manual, 1);
        let view = serde_json::to_value(entry.view()).unwrap();
        assert!(view.get("secret").is_none());
        assert_eq!(view["kind"], serde_json::json!({"type": "totp", "period": 60}));
        assert_eq!(view["export"]["google"], "period_not_30");
        assert_eq!(view["export"]["microsoft"], "period_not_30");
        let plain = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Manual, 1).view();
        assert_eq!((plain.export.google, plain.export.microsoft), (None, None));
    }

    #[test]
    fn sameness_is_about_secrets_and_parameters_not_names() {
        let entry = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1);
        assert!(entry.same_account(&auth("otpauth://totp/Other:name?secret=JBSWY3DPEHPK3PXP")));
        assert!(!entry.same_account(&auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP&digits=8")));
        assert!(!entry.same_account(&auth("otpauth://hotp/A:b?secret=JBSWY3DPEHPK3PXP")));
        assert!(entry.same_names(&auth("otpauth://totp/a:B?secret=GEZDGNBV")));
        assert!(!entry.same_names(&auth("otpauth://totp/a:c?secret=GEZDGNBV")));
        let hotp = Entry::from_auth(auth("otpauth://hotp/A:b?secret=JBSWY3DPEHPK3PXP&counter=5"), Origin::Uri, 1);
        assert!(hotp.same_account(&auth("otpauth://hotp/A:b?secret=JBSWY3DPEHPK3PXP&counter=9")), "a counter is state, not identity");
        let _ = Period::THIRTY;
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  GitHub\u{0007}\n "), "GitHub");
        assert_eq!(clean_name(&"x".repeat(500)).chars().count(), MAX_NAME_CHARS);
    }
}
