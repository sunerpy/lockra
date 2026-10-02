//! The two secrets of a sync space, and the keyring object that joins them to the master password.
//!
//! The data key encrypts the device snapshots and never reaches the storage unwrapped. The
//! keyring object wraps it under HKDF(Argon2id(master password) ‖ sync key): someone holding the
//! storage's contents and guessing (or knowing) the master password opens nothing without the
//! 256-bit sync key, and the sync key alone opens nothing without the password.

use std::fmt;

use chacha20poly1305::XNonce;
use chacha20poly1305::aead::{Aead, Payload};
use data_encoding::{BASE32_NOPAD, BASE64};
use hmac::{Hmac, KeyInit, Mac};
use lockra_vault::{KdfCost, KdfParams};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::frame::{KEY_LEN, NONCE_LEN, Parsed, b64, cipher, header_bytes, hkdf32, parse, random};
use crate::{Hlc, SyncError};

const KEYRING_MAGIC: &[u8; 8] = b"LKSKEYR1";
const KEYRING_FORMAT: u32 = 1;
const KEYRING_INFO: &[u8] = b"lockra-sync v1 keyring";
const SNAPSHOT_INFO: &[u8] = b"lockra-sync v1 snapshot";
const TAG_INFO: &[u8] = b"lockra-sync v1 device tag";
const SPACE_ID_INFO: &[u8] = b"lockra-sync v1 space id";
const SYNC_KEY_PREFIX: &str = "LKS1";
/// Bytes of SHA-256 appended to the sync key text: a mistyped character is caught on entry.
const CHECK_LEN: usize = 3;
/// Characters per group of the sync key text.
const GROUP: usize = 4;
/// Bytes of a device tag (32 hex characters).
const TAG_BYTES: usize = 16;

/// The length of [`SyncKey::to_text`]: `LKS1-` and 14 groups of four.
pub const SYNC_KEY_TEXT_LEN: usize = SYNC_KEY_PREFIX.len() + 1 + 56 + 13;

/// The sync key: 32 random bytes the user keeps (written down, or in a password manager). With the
/// master password it joins a device to the space, or recovers the space when every device is
/// lost; it also names the space ([`Self::space_id`]), so the storage and the key are all a
/// recovery has to type.
#[derive(Clone, PartialEq, Eq)]
pub struct SyncKey(Zeroizing<[u8; KEY_LEN]>);

impl SyncKey {
    /// A fresh key.
    pub fn generate() -> Result<Self, SyncError> {
        Ok(Self(Zeroizing::new(random()?)))
    }

    /// `LKS1-XXXX-…`: Base32 of the key and a 3-byte checksum, in groups of four.
    pub fn to_text(&self) -> Zeroizing<String> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(KEY_LEN + CHECK_LEN));
        bytes.extend_from_slice(self.0.as_ref());
        bytes.extend_from_slice(&Sha256::digest(self.0.as_ref())[..CHECK_LEN]);
        let encoded = Zeroizing::new(BASE32_NOPAD.encode(&bytes));
        let mut text = Zeroizing::new(String::with_capacity(SYNC_KEY_TEXT_LEN));
        text.push_str(SYNC_KEY_PREFIX);
        for (i, ch) in encoded.chars().enumerate() {
            if i % GROUP == 0 {
                text.push('-');
            }
            text.push(ch);
        }
        text
    }

    /// The key from its text: case, spaces and dashes do not matter; the prefix and the checksum
    /// do.
    pub fn from_text(text: &str) -> Result<Self, SyncError> {
        let cleaned = Zeroizing::new(text.chars().filter(|c| !c.is_whitespace() && *c != '-').collect::<String>().to_ascii_uppercase());
        let body = cleaned.strip_prefix(SYNC_KEY_PREFIX).ok_or(SyncError::BadSyncKey)?;
        let bytes = Zeroizing::new(BASE32_NOPAD.decode(body.as_bytes()).map_err(|_| SyncError::BadSyncKey)?);
        if bytes.len() != KEY_LEN + CHECK_LEN || Sha256::digest(&bytes[..KEY_LEN])[..CHECK_LEN] != bytes[KEY_LEN..] {
            return Err(SyncError::BadSyncKey);
        }
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        key.copy_from_slice(&bytes[..KEY_LEN]);
        Ok(Self(key))
    }

    /// The id of the space this key belongs to: a one-way function of the key, so the id the
    /// storage shows in its paths says nothing about the key.
    pub fn space_id(&self) -> Uuid {
        let derived = hkdf32(SYNC_KEY_PREFIX.as_bytes(), self.0.as_ref(), SPACE_ID_INFO);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&derived[..16]);
        uuid::Builder::from_random_bytes(bytes).into_uuid()
    }
}

impl fmt::Debug for SyncKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SyncKey(…)")
    }
}

/// A sync space as every one of its devices holds it: its id and its data key.
#[derive(Clone)]
pub struct SpaceKeys {
    space_id: Uuid,
    dek: Zeroizing<[u8; KEY_LEN]>,
}

impl SpaceKeys {
    /// A new space under `space_id` with a fresh data key.
    pub fn generate(space_id: Uuid) -> Result<Self, SyncError> {
        Ok(Self { space_id, dek: Zeroizing::new(random()?) })
    }

    /// The space's id.
    pub fn space_id(&self) -> Uuid {
        self.space_id
    }

    /// The data key as Base64, for the vault's encrypted local part.
    pub fn data_key_text(&self) -> Zeroizing<String> {
        Zeroizing::new(BASE64.encode(self.dek.as_ref()))
    }

    /// The space back from its id and [`Self::data_key_text`].
    pub fn from_parts(space_id: Uuid, data_key_text: &str) -> Result<Self, SyncError> {
        let bytes = Zeroizing::new(BASE64.decode(data_key_text.as_bytes()).map_err(|_| SyncError::Corrupted)?);
        let dek: [u8; KEY_LEN] = bytes.as_slice().try_into().map_err(|_| SyncError::Corrupted)?;
        Ok(Self { space_id, dek: Zeroizing::new(dek) })
    }

    /// The key the snapshots are encrypted under.
    pub(crate) fn snapshot_key(&self) -> Zeroizing<[u8; KEY_LEN]> {
        hkdf32(self.space_id.as_bytes(), self.dek.as_ref(), SNAPSHOT_INFO)
    }

    /// The name `device`'s snapshot is stored under: a keyed hash of its number, so the storage
    /// cannot link a device across spaces or learn its number.
    pub fn device_tag(&self, device: u64) -> String {
        let key = hkdf32(self.space_id.as_bytes(), self.dek.as_ref(), TAG_INFO);
        // HMAC takes a key of any length (RFC 2104 §2), so `new_from_slice` cannot fail here.
        #[allow(clippy::expect_used)]
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_ref()).expect("HMAC accepts keys of any length");
        mac.update(&device.to_be_bytes());
        data_encoding::HEXLOWER.encode(&mac.finalize().into_bytes()[..TAG_BYTES])
    }
}

impl fmt::Debug for SpaceKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpaceKeys").field("space_id", &self.space_id).finish_non_exhaustive()
    }
}

/// `true` for a string [`SpaceKeys::device_tag`] could have produced.
pub(crate) fn is_tag(text: &str) -> bool {
    text.len() == TAG_BYTES * 2 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Serialize, Deserialize)]
struct KeyringHeader {
    format: u32,
    space_id: Uuid,
    /// When the keyring was sealed: a new master password's keyring replaces only an older one.
    stamp: Hlc,
    kdf: KdfParams,
    #[serde(with = "b64")]
    nonce: [u8; NONCE_LEN],
}

/// What a keyring object's header says, read without opening it: its space and when it was
/// sealed. Not authenticated (only opening the keyring proves it), so it may decide only what is
/// harmless to get wrong, such as not overwriting a keyring that reads as newer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyringInfo {
    /// The space.
    pub space_id: Uuid,
    /// When it was sealed.
    pub stamp: Hlc,
}

/// The header of a keyring object.
pub fn keyring_info(bytes: &[u8]) -> Result<KeyringInfo, SyncError> {
    let parsed: Parsed<'_, KeyringHeader> = parse(bytes, KEYRING_MAGIC, KEYRING_FORMAT)?;
    Ok(KeyringInfo { space_id: parsed.header.space_id, stamp: parsed.header.stamp })
}

/// The keyring object of `keys`' space: the data key wrapped under `password` and `sync_key`,
/// sealed at `stamp` (a change of the device that seals it).
pub fn seal_keyring(keys: &SpaceKeys, sync_key: &SyncKey, password: &[u8], cost: KdfCost, stamp: Hlc) -> Result<Vec<u8>, SyncError> {
    let kdf = KdfParams::fresh(cost).map_err(|_| SyncError::Random)?;
    let kek = keyring_kek(&kdf, keys.space_id, sync_key, password)?;
    let header = KeyringHeader { format: KEYRING_FORMAT, space_id: keys.space_id, stamp, kdf, nonce: random()? };
    let mut object = header_bytes(KEYRING_MAGIC, &header)?;
    let wrapped = cipher(&kek).encrypt(&XNonce::from(header.nonce), Payload { msg: keys.dek.as_ref(), aad: &object }).map_err(|_| SyncError::Corrupted)?;
    object.extend_from_slice(&wrapped);
    Ok(object)
}

/// Open the keyring object of space `space_id`. A wrong password, a wrong sync key and an altered
/// object all read as [`SyncError::WrongCredentials`]: the authentication cannot tell them apart.
pub fn open_keyring(bytes: &[u8], space_id: Uuid, sync_key: &SyncKey, password: &[u8]) -> Result<SpaceKeys, SyncError> {
    let parsed: Parsed<'_, KeyringHeader> = parse(bytes, KEYRING_MAGIC, KEYRING_FORMAT)?;
    if parsed.header.space_id != space_id {
        return Err(SyncError::Misplaced);
    }
    let kek = keyring_kek(&parsed.header.kdf, space_id, sync_key, password)?;
    let plain = Zeroizing::new(
        cipher(&kek)
            .decrypt(&XNonce::from(parsed.header.nonce), Payload { msg: parsed.ciphertext, aad: parsed.associated })
            .map_err(|_| SyncError::WrongCredentials)?,
    );
    let dek: [u8; KEY_LEN] = plain.as_slice().try_into().map_err(|_| SyncError::Corrupted)?;
    Ok(SpaceKeys { space_id, dek: Zeroizing::new(dek) })
}

fn keyring_kek(kdf: &KdfParams, space_id: Uuid, sync_key: &SyncKey, password: &[u8]) -> Result<Zeroizing<[u8; KEY_LEN]>, SyncError> {
    // Hostile parameters (gigabytes of memory, minutes of passes) are refused before any work.
    let stretched = kdf.derive(password).map_err(|_| SyncError::Corrupted)?;
    let mut ikm = Zeroizing::new([0u8; 2 * KEY_LEN]);
    ikm[..KEY_LEN].copy_from_slice(stretched.as_ref());
    ikm[KEY_LEN..].copy_from_slice(sync_key.0.as_ref());
    Ok(hkdf32(space_id.as_bytes(), ikm.as_ref(), KEYRING_INFO))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWORD: &[u8] = b"correct horse battery";

    fn space() -> SpaceKeys {
        SpaceKeys::generate(Uuid::new_v4()).unwrap()
    }

    #[test]
    fn the_sync_key_text_round_trips_and_forgives_case_spaces_and_dashes() {
        let key = SyncKey::generate().unwrap();
        let text = key.to_text();
        assert_eq!(text.len(), SYNC_KEY_TEXT_LEN);
        assert!(text.starts_with("LKS1-"));
        assert_eq!(text.matches('-').count(), 14);
        assert_eq!(*SyncKey::from_text(&text).unwrap().0, *key.0);
        let sloppy = text.to_ascii_lowercase().replace('-', " ");
        assert_eq!(*SyncKey::from_text(&sloppy).unwrap().0, *key.0);
        assert_eq!(format!("{key:?}"), "SyncKey(…)");
    }

    #[test]
    fn the_sync_key_names_its_space() {
        let key = SyncKey::generate().unwrap();
        let id = key.space_id();
        assert_eq!(SyncKey::from_text(&key.to_text()).unwrap().space_id(), id, "the same key, the same space");
        assert_ne!(SyncKey::generate().unwrap().space_id(), id);
        assert_eq!(id.get_version_num(), 4);
        assert!(!key.to_text().to_ascii_lowercase().contains(&id.simple().to_string()[..8]));
    }

    #[test]
    fn a_mistyped_or_foreign_sync_key_is_refused() {
        let text = SyncKey::generate().unwrap().to_text();
        // One character changed: the checksum catches it.
        let mut chars: Vec<char> = text.chars().collect();
        let i = chars.len() - 6;
        chars[i] = if chars[i] == 'A' { 'B' } else { 'A' };
        let typo: String = chars.into_iter().collect();
        assert!(matches!(SyncKey::from_text(&typo), Err(SyncError::BadSyncKey)));
        for bad in ["", "LKS1", "LKS2-AAAA", &text[5..], &text[..text.len() - 4], "LKS1-!!!!"] {
            assert!(matches!(SyncKey::from_text(bad), Err(SyncError::BadSyncKey)), "{bad}");
        }
    }

    #[test]
    fn the_keyring_opens_only_with_the_password_and_the_sync_key_of_its_space() {
        let keys = space();
        let sync_key = SyncKey::generate().unwrap();
        let object = seal_keyring(&keys, &sync_key, PASSWORD, KdfCost::FAST_INSECURE, Hlc::at(1_000)).unwrap();
        let opened = open_keyring(&object, keys.space_id(), &sync_key, PASSWORD).unwrap();
        assert_eq!(*opened.dek, *keys.dek);
        assert_eq!(opened.space_id(), keys.space_id());

        assert_eq!(open_keyring(&object, keys.space_id(), &sync_key, b"wrong password").err(), Some(SyncError::WrongCredentials));
        let other_key = SyncKey::generate().unwrap();
        assert_eq!(open_keyring(&object, keys.space_id(), &other_key, PASSWORD).err(), Some(SyncError::WrongCredentials), "the password alone is not enough");
        assert_eq!(open_keyring(&object, Uuid::new_v4(), &sync_key, PASSWORD).err(), Some(SyncError::Misplaced));
        // Any altered byte, in the header or the ciphertext, fails.
        for i in [14, object.len() - 1] {
            let mut altered = object.clone();
            altered[i] ^= 1;
            assert!(open_keyring(&altered, keys.space_id(), &sync_key, PASSWORD).is_err(), "byte {i}");
        }
    }

    #[test]
    fn the_header_names_the_space_and_when_it_was_sealed() {
        let keys = space();
        let sync_key = SyncKey::generate().unwrap();
        let stamp = Hlc { wall_ms: 5, counter: 1, device: 7 };
        let object = seal_keyring(&keys, &sync_key, PASSWORD, KdfCost::FAST_INSECURE, stamp).unwrap();
        assert_eq!(keyring_info(&object).unwrap(), KeyringInfo { space_id: keys.space_id(), stamp });
        assert_eq!(keyring_info(b"LKSDEVS1 not a keyring").err(), Some(SyncError::NotLockra));
        assert!(keyring_info(&object[..10]).is_err());
    }

    #[test]
    fn a_keyring_naming_hostile_kdf_parameters_is_refused_before_any_work() {
        let keys = space();
        let sync_key = SyncKey::generate().unwrap();
        let object = seal_keyring(&keys, &sync_key, PASSWORD, KdfCost::FAST_INSECURE, Hlc::at(1_000)).unwrap();
        let parsed: Parsed<'_, KeyringHeader> = parse(&object, KEYRING_MAGIC, KEYRING_FORMAT).unwrap();
        let mut header = serde_json::to_value(&parsed.header).unwrap();
        header["kdf"]["m_kib"] = serde_json::json!(16 * 1024 * 1024);
        let mut hostile = header_bytes(KEYRING_MAGIC, &header).unwrap();
        hostile.extend_from_slice(parsed.ciphertext);
        assert_eq!(open_keyring(&hostile, keys.space_id(), &sync_key, PASSWORD).err(), Some(SyncError::Corrupted));
    }

    #[test]
    fn the_data_key_round_trips_as_text_and_derives_stable_distinct_tags() {
        let keys = space();
        let back = SpaceKeys::from_parts(keys.space_id(), &keys.data_key_text()).unwrap();
        assert_eq!(*back.dek, *keys.dek);
        assert!(SpaceKeys::from_parts(keys.space_id(), "not base64!").is_err());
        assert!(SpaceKeys::from_parts(keys.space_id(), "AAAA").is_err(), "too short");
        let tag = keys.device_tag(42);
        assert!(is_tag(&tag), "{tag}");
        assert_eq!(tag, back.device_tag(42));
        assert_ne!(tag, keys.device_tag(43));
        assert_ne!(tag, space().device_tag(42), "another space, another name");
        assert!(!is_tag("XYZ") && !is_tag(&tag.to_ascii_uppercase()) && !is_tag(&tag[1..]));
        assert!(format!("{keys:?}").contains(&keys.space_id().to_string()));
        assert!(!format!("{keys:?}").contains(keys.data_key_text().as_str()));
    }
}
