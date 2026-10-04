//! The invitation one device of a space shows another: the storage, its credentials and the sync
//! key, as one text (and the QR code of that text). It holds everything but a master password,
//! so the device that scans it still needs the master password of a device in the space to open
//! that device's keyring.
//!
//! The QR code carries the invitation as it is (`lockra-invite:1:`): it never leaves the screen.
//! The text to send through a chat or a mail is sealed (`lockra-invite:2:`) under a one-time code
//! shown only on the screen: 10 Crockford Base32 characters (50 bits), stretched with Argon2id at
//! the vault's cost, so a sealed text that leaks is of no use without the code.

use std::fmt;

use chacha20poly1305::XNonce;
use chacha20poly1305::aead::{Aead, Payload};
use data_encoding::BASE64URL_NOPAD;
use lockra_vault::{KdfCost, KdfParams};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::frame::{NONCE_LEN, b64, cipher, header_bytes, hkdf32, parse, random};
use crate::{StorageConfig, SyncError, SyncKey};

const INVITE_PREFIX: &str = "lockra-invite:1:";
const SHARED_PREFIX: &str = "lockra-invite:2:";
const SHARED_MAGIC: &[u8; 8] = b"LKSINVT2";
const SHARED_FORMAT: u32 = 2;
const SHARED_INFO: &[u8] = b"lockra-sync v1 shared invitation";
/// The one-time code: 10 characters of Crockford's Base32, 50 bits.
pub const INVITE_CODE_LEN: usize = 10;
const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// The longest shared text read: far above any invitation, far below anything that costs.
const MAX_SHARED_TEXT: usize = 64 * 1024;

#[derive(Serialize, Deserialize)]
struct SharedHeader {
    format: u32,
    kdf: KdfParams,
    #[serde(with = "b64")]
    nonce: [u8; NONCE_LEN],
}

/// What a device needs, besides the master password, to join a space.
#[derive(Clone, PartialEq, Eq)]
pub struct Invite {
    /// Where the space is stored.
    pub storage: StorageConfig,
    /// The space's sync key (it names the space too, [`SyncKey::space_id`]).
    pub sync_key: SyncKey,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    storage: StorageConfig,
    sync_key: Zeroizing<String>,
}

impl Invite {
    /// `lockra-invite:1:` and the invitation in Base64url.
    pub fn to_text(&self) -> Zeroizing<String> {
        let wire = Wire { storage: self.storage.clone(), sync_key: self.sync_key.to_text() };
        // Serializing plain data structures to JSON cannot fail.
        #[allow(clippy::expect_used)]
        let json = Zeroizing::new(serde_json::to_vec(&wire).expect("an invitation serializes"));
        let mut text = Zeroizing::new(String::with_capacity(INVITE_PREFIX.len() + json.len() * 4 / 3 + 4));
        text.push_str(INVITE_PREFIX);
        text.push_str(&Zeroizing::new(BASE64URL_NOPAD.encode(&json)));
        text
    }

    /// The invitation from its text (surrounding white space does not matter). A well-formed
    /// sealed text is [`SyncError::BadInviteCode`]: it opens with [`Invite::from_any_text`] and
    /// its code.
    pub fn from_text(text: &str) -> Result<Self, SyncError> {
        let text = text.trim();
        if let Some(body) = text.strip_prefix(SHARED_PREFIX) {
            return Err(match sealed_object(body) {
                Ok(_) => SyncError::BadInviteCode,
                Err(error) => error,
            });
        }
        let body = text.strip_prefix(INVITE_PREFIX).ok_or(SyncError::BadInvite)?;
        let json = Zeroizing::new(BASE64URL_NOPAD.decode(body.as_bytes()).map_err(|_| SyncError::BadInvite)?);
        Self::from_json(&json)
    }

    fn from_json(json: &[u8]) -> Result<Self, SyncError> {
        let wire: Wire = serde_json::from_slice(json).map_err(|_| SyncError::BadInvite)?;
        let sync_key = SyncKey::from_text(&wire.sync_key).map_err(|_| SyncError::BadInvite)?;
        Ok(Self { storage: wire.storage, sync_key })
    }

    /// The invitation sealed for sending (`lockra-invite:2:…`) and the one-time code that opens
    /// it (`ABCDE-FGHJK`), stretched at `cost`.
    pub fn to_shared_text(&self, cost: KdfCost) -> Result<(Zeroizing<String>, Zeroizing<String>), SyncError> {
        let code = random_code()?;
        let kdf = KdfParams::fresh(cost).map_err(|_| SyncError::Random)?;
        let key = shared_key(&kdf, &code)?;
        let header = SharedHeader { format: SHARED_FORMAT, kdf, nonce: random()? };
        let mut object = header_bytes(SHARED_MAGIC, &header)?;
        let wire = Wire { storage: self.storage.clone(), sync_key: self.sync_key.to_text() };
        // Serializing plain data structures to JSON cannot fail.
        #[allow(clippy::expect_used)]
        let json = Zeroizing::new(serde_json::to_vec(&wire).expect("an invitation serializes"));
        let sealed = cipher(&key).encrypt(&XNonce::from(header.nonce), Payload { msg: json.as_ref(), aad: &object }).map_err(|_| SyncError::Corrupted)?;
        object.extend_from_slice(&sealed);
        let mut text = Zeroizing::new(String::with_capacity(SHARED_PREFIX.len() + object.len() * 4 / 3 + 4));
        text.push_str(SHARED_PREFIX);
        text.push_str(&BASE64URL_NOPAD.encode(&object));
        Ok((text, Zeroizing::new(format!("{}-{}", &code[..5], &code[5..]))))
    }

    /// The invitation from either text: as it is (`code` ignored), or sealed and opened with
    /// `code` (case, spaces and dashes do not matter; O reads as 0, I and L as 1). A missing or
    /// wrong code is [`SyncError::BadInviteCode`]; anything else that is not an invitation,
    /// [`SyncError::BadInvite`].
    pub fn from_any_text(text: &str, code: Option<&str>) -> Result<Self, SyncError> {
        let text = text.trim();
        let Some(body) = text.strip_prefix(SHARED_PREFIX) else { return Self::from_text(text) };
        let object = sealed_object(body)?;
        let parsed = parse::<SharedHeader>(&object, SHARED_MAGIC, SHARED_FORMAT).map_err(|_| SyncError::BadInvite)?;
        let code = code.and_then(normalize_code).ok_or(SyncError::BadInviteCode)?;
        // Hostile parameters (gigabytes of memory, minutes of passes) are refused before any work.
        let key = shared_key(&parsed.header.kdf, &code)?;
        let json = Zeroizing::new(
            cipher(&key)
                .decrypt(&XNonce::from(parsed.header.nonce), Payload { msg: parsed.ciphertext, aad: parsed.associated })
                .map_err(|_| SyncError::BadInviteCode)?,
        );
        Self::from_json(&json)
    }

    /// Whether `text` is a sealed invitation, which needs its code.
    pub fn is_shared_text(text: &str) -> bool {
        text.trim().starts_with(SHARED_PREFIX)
    }
}

/// The sealed object of a shared text's `body`, its framing checked (nothing opened yet).
fn sealed_object(body: &str) -> Result<Vec<u8>, SyncError> {
    if body.len() > MAX_SHARED_TEXT {
        return Err(SyncError::BadInvite);
    }
    let object = BASE64URL_NOPAD.decode(body.as_bytes()).map_err(|_| SyncError::BadInvite)?;
    parse::<SharedHeader>(&object, SHARED_MAGIC, SHARED_FORMAT).map_err(|_| SyncError::BadInvite)?;
    Ok(object)
}

fn shared_key(kdf: &KdfParams, code: &str) -> Result<Zeroizing<[u8; 32]>, SyncError> {
    let stretched = kdf.derive(code.as_bytes()).map_err(|_| SyncError::BadInvite)?;
    Ok(hkdf32(SHARED_INFO, stretched.as_ref(), SHARED_INFO))
}

/// Ten random characters of [`CODE_ALPHABET`]: 50 bits from the system's generator.
fn random_code() -> Result<Zeroizing<String>, SyncError> {
    let bytes: [u8; 7] = random()?;
    let mut bits = bytes.iter().fold(0u64, |acc, &b| (acc << 8) | u64::from(b));
    let mut code = Zeroizing::new(String::with_capacity(INVITE_CODE_LEN));
    for _ in 0..INVITE_CODE_LEN {
        code.push(char::from(CODE_ALPHABET[usize::try_from(bits & 31).unwrap_or(0)]));
        bits >>= 5;
    }
    Ok(code)
}

/// A typed code in its canonical form, or `None` when it cannot be one.
fn normalize_code(typed: &str) -> Option<Zeroizing<String>> {
    let mut code = Zeroizing::new(String::with_capacity(INVITE_CODE_LEN));
    for c in typed.chars().filter(|c| !c.is_whitespace() && *c != '-') {
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        if !CODE_ALPHABET.contains(&u8::try_from(c).ok()?) {
            return None;
        }
        code.push(c);
    }
    (code.len() == INVITE_CODE_LEN).then_some(code)
}

impl fmt::Debug for Invite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Invite").field("storage", &self.storage).finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invite() -> Invite {
        Invite {
            storage: StorageConfig::Webdav {
                url: "https://dav.example.com/dav/".into(),
                prefix: "lockra".into(),
                username: "me".into(),
                password: Zeroizing::new("app password".into()),
            },
            sync_key: SyncKey::generate().unwrap(),
        }
    }

    #[test]
    fn an_invitation_round_trips_and_names_the_same_space() {
        let original = invite();
        let text = original.to_text();
        assert!(text.starts_with(INVITE_PREFIX));
        assert!(!text.contains("app password"), "nothing in clear");
        let back = Invite::from_text(&format!("  {}\n", text.as_str())).unwrap();
        assert_eq!(back, original);
        assert_eq!(back.sync_key.space_id(), original.sync_key.space_id());
        let debug = format!("{back:?}");
        assert!(!debug.contains("app password") && !debug.contains(back.sync_key.to_text().as_str()), "{debug}");
    }

    #[test]
    fn a_shared_invitation_opens_with_its_code_only() {
        let original = invite();
        let (text, code) = original.to_shared_text(KdfCost::FAST_INSECURE).unwrap();
        assert!(text.starts_with(SHARED_PREFIX));
        assert!(Invite::is_shared_text(&text) && !Invite::is_shared_text(&original.to_text()));
        assert!(!text.contains("app password") && !text.contains(code.as_str()), "nothing in clear");
        assert_eq!(code.len(), INVITE_CODE_LEN + 1);
        assert_eq!(Invite::from_any_text(&format!(" {} ", text.as_str()), Some(&code)).unwrap(), original);
        // Case, spaces, dashes and the letters Crockford reads as digits do not matter.
        let sloppy = code.to_ascii_lowercase().replace('-', " ").replace('0', "o").replace('1', "l");
        assert_eq!(Invite::from_any_text(&text, Some(&sloppy)).unwrap(), original);
        // A plain invitation needs no code, and ignores one.
        assert_eq!(Invite::from_any_text(&original.to_text(), Some("whatever")).unwrap(), original);
        // A sealed text where a plain one is expected asks for its code.
        assert_eq!(Invite::from_text(&text).err(), Some(SyncError::BadInviteCode));
        for wrong in [None, Some(""), Some("ABCDE"), Some("ABCDE-FGHJKX"), Some("ABCDE-FGHJU"), Some("00000-00000")] {
            assert_eq!(Invite::from_any_text(&text, wrong).err(), Some(SyncError::BadInviteCode), "{wrong:?}");
        }
        // Two invitations of the same space do not read alike, nor share a code.
        let (again, other_code) = original.to_shared_text(KdfCost::FAST_INSECURE).unwrap();
        assert_ne!(again, text);
        assert_ne!(other_code, code);
    }

    #[test]
    fn a_damaged_shared_invitation_is_not_one() {
        let (text, code) = invite().to_shared_text(KdfCost::FAST_INSECURE).unwrap();
        let body = &text[SHARED_PREFIX.len()..];
        let mut bytes = BASE64URL_NOPAD.decode(body.as_bytes()).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let flipped = format!("{SHARED_PREFIX}{}", BASE64URL_NOPAD.encode(&bytes));
        assert_eq!(Invite::from_any_text(&flipped, Some(&code)).err(), Some(SyncError::BadInviteCode));
        for bad in [
            format!("{SHARED_PREFIX}not base64!"),
            format!("{SHARED_PREFIX}{}", BASE64URL_NOPAD.encode(b"LKSKEYR1junk")),
            format!("{SHARED_PREFIX}{}", "A".repeat(MAX_SHARED_TEXT + 1)),
        ] {
            assert_eq!(Invite::from_any_text(&bad, Some(&code)).err(), Some(SyncError::BadInvite), "{}", &bad[..40]);
        }
        // A header asking for hostile Argon2 parameters is refused before any work.
        let original = BASE64URL_NOPAD.decode(body.as_bytes()).unwrap();
        let parsed: crate::frame::Parsed<'_, SharedHeader> = parse(&original, SHARED_MAGIC, SHARED_FORMAT).unwrap();
        let mut header = serde_json::to_value(&parsed.header).unwrap();
        header["kdf"]["m_kib"] = serde_json::json!(4 * 1024 * 1024);
        let mut object = header_bytes(SHARED_MAGIC, &header).unwrap();
        object.extend_from_slice(parsed.ciphertext);
        let hostile = format!("{SHARED_PREFIX}{}", BASE64URL_NOPAD.encode(&object));
        assert_eq!(Invite::from_any_text(&hostile, Some(&code)).err(), Some(SyncError::BadInvite));
    }

    #[test]
    fn codes_use_crockfords_alphabet_only() {
        for _ in 0..64 {
            let code = random_code().unwrap();
            assert_eq!(code.len(), INVITE_CODE_LEN);
            assert!(code.bytes().all(|c| CODE_ALPHABET.contains(&c)), "{}", code.as_str());
        }
        assert_eq!(normalize_code("ab cd-ef gh jk").as_deref().map(String::as_str), Some("ABCDEFGHJK"));
        assert_eq!(normalize_code("ABCDE-FGHJ€").as_deref(), None);
    }

    #[test]
    fn anything_else_is_not_an_invitation() {
        let text = invite().to_text();
        let body = &text[INVITE_PREFIX.len()..];
        let tampered_key = String::from_utf8(BASE64URL_NOPAD.decode(body.as_bytes()).unwrap()).unwrap().replacen("LKS1-", "LKS1-A", 1);
        for bad in [
            String::new(),
            "otpauth://totp/x?secret=GEZDGNBV".to_owned(),
            format!("lockra-invite:2:{body}"),
            format!("{INVITE_PREFIX}not base64!"),
            format!("{INVITE_PREFIX}{}", BASE64URL_NOPAD.encode(b"{\"storage\":1}")),
            format!("{INVITE_PREFIX}{}", BASE64URL_NOPAD.encode(tampered_key.as_bytes())),
        ] {
            assert_eq!(Invite::from_text(&bad).err(), Some(SyncError::BadInvite), "{bad}");
        }
    }
}
