//! The framing every sync object shares with Lockra's vault files: an 8-byte magic, the header's
//! length (u32, little-endian), the JSON header, then the ciphertext. Everything before the
//! ciphertext is the associated data, so a changed header byte fails the authentication.

use chacha20poly1305::{KeyInit, XChaCha20Poly1305};
use hkdf::Hkdf;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::SyncError;

pub(crate) const PREFIX_LEN: usize = 12;
/// A header is a few hundred bytes; anything far larger is not one Lockra wrote.
pub(crate) const MAX_HEADER_LEN: usize = 16 * 1024;
pub(crate) const TAG_LEN: usize = 16;
pub(crate) const NONCE_LEN: usize = 24;
pub(crate) const KEY_LEN: usize = 32;

#[derive(serde::Deserialize)]
struct FormatProbe {
    format: u32,
}

/// `magic | length | header`: the start of an object and the associated data of its ciphertext.
pub(crate) fn header_bytes(magic: &[u8; 8], header: &impl Serialize) -> Result<Vec<u8>, SyncError> {
    let json = serde_json::to_vec(header).map_err(|_| SyncError::Corrupted)?;
    let length = u32::try_from(json.len()).map_err(|_| SyncError::Corrupted)?;
    let mut out = Vec::with_capacity(PREFIX_LEN + json.len());
    out.extend_from_slice(magic);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(&json);
    Ok(out)
}

/// An object split into its header, its associated data and its ciphertext.
pub(crate) struct Parsed<'a, H> {
    pub header: H,
    pub associated: &'a [u8],
    pub ciphertext: &'a [u8],
}

/// Split an object with `magic` whose header is of `format`.
pub(crate) fn parse<'a, H: DeserializeOwned>(bytes: &'a [u8], magic: &[u8; 8], format: u32) -> Result<Parsed<'a, H>, SyncError> {
    if bytes.get(..8) != Some(&magic[..]) {
        return Err(SyncError::NotLockra);
    }
    let length: [u8; 4] = bytes.get(8..PREFIX_LEN).and_then(|b| b.try_into().ok()).ok_or(SyncError::Corrupted)?;
    let header_len = usize::try_from(u32::from_le_bytes(length)).map_err(|_| SyncError::Corrupted)?;
    if header_len == 0 || header_len > MAX_HEADER_LEN {
        return Err(SyncError::Corrupted);
    }
    let end = PREFIX_LEN + header_len;
    let json = bytes.get(PREFIX_LEN..end).ok_or(SyncError::Corrupted)?;
    let probe: FormatProbe = serde_json::from_slice(json).map_err(|_| SyncError::Corrupted)?;
    if probe.format != format {
        return Err(SyncError::Unsupported(probe.format));
    }
    let header = serde_json::from_slice(json).map_err(|_| SyncError::Corrupted)?;
    let ciphertext = &bytes[end..];
    if ciphertext.len() < TAG_LEN {
        return Err(SyncError::Corrupted);
    }
    Ok(Parsed { header, associated: &bytes[..end], ciphertext })
}

pub(crate) fn cipher(key: &[u8; KEY_LEN]) -> XChaCha20Poly1305 {
    // A 32-byte slice is exactly the key size, so this cannot fail.
    #[allow(clippy::expect_used)]
    XChaCha20Poly1305::new_from_slice(key).expect("32-byte key")
}

/// `N` bytes from the system's generator.
pub(crate) fn random<const N: usize>() -> Result<[u8; N], SyncError> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|_| SyncError::Random)?;
    Ok(bytes)
}

/// A 32-byte key from HKDF-SHA256 (RFC 5869).
pub(crate) fn hkdf32(salt: &[u8], ikm: &[u8], info: &[u8]) -> Zeroizing<[u8; KEY_LEN]> {
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    // 32 bytes is far below HKDF-SHA256's 8160-byte limit, so expanding cannot fail.
    #[allow(clippy::expect_used)]
    Hkdf::<Sha256>::new(Some(salt), ikm).expand(info, key.as_mut()).expect("a 32-byte output");
    key
}

/// Fixed-size byte arrays as standard Base64 in a JSON header.
pub(crate) mod b64 {
    use data_encoding::BASE64;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer, const N: usize>(bytes: &[u8; N], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&BASE64.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = BASE64.decode(text.as_bytes()).map_err(serde::de::Error::custom)?;
        bytes.try_into().map_err(|_| serde::de::Error::custom("wrong length"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, serde::Deserialize, Debug, PartialEq)]
    struct Header {
        format: u32,
        #[serde(with = "b64")]
        nonce: [u8; 4],
    }

    fn object(header: &Header, tail: &[u8]) -> Vec<u8> {
        let mut bytes = header_bytes(b"TESTOBJ1", header).unwrap();
        bytes.extend_from_slice(tail);
        bytes
    }

    #[test]
    fn a_framed_object_parses_back_with_its_associated_data() {
        let header = Header { format: 1, nonce: [1, 2, 3, 4] };
        let bytes = object(&header, &[9; TAG_LEN]);
        let parsed: Parsed<'_, Header> = parse(&bytes, b"TESTOBJ1", 1).unwrap();
        assert_eq!(parsed.header, header);
        assert_eq!(parsed.ciphertext, &[9; TAG_LEN]);
        assert_eq!(parsed.associated, &bytes[..bytes.len() - TAG_LEN]);
    }

    #[test]
    fn foreign_short_oversized_and_future_objects_are_refused() {
        let header = Header { format: 1, nonce: [0; 4] };
        let good = object(&header, &[0; TAG_LEN]);
        assert_eq!(parse::<Header>(&good, b"OTHERMAG", 1).err(), Some(SyncError::NotLockra));
        assert_eq!(parse::<Header>(b"TEST", b"TESTOBJ1", 1).err(), Some(SyncError::NotLockra));
        assert_eq!(parse::<Header>(&good[..10], b"TESTOBJ1", 1).err(), Some(SyncError::Corrupted));
        assert_eq!(parse::<Header>(&good[..good.len() - 1], b"TESTOBJ1", 1).err(), Some(SyncError::Corrupted), "the tag is cut");
        let mut huge = good.clone();
        huge[8..12].copy_from_slice(&(u32::try_from(MAX_HEADER_LEN).unwrap() + 1).to_le_bytes());
        assert_eq!(parse::<Header>(&huge, b"TESTOBJ1", 1).err(), Some(SyncError::Corrupted));
        let mut zero = good.clone();
        zero[8..12].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(parse::<Header>(&zero, b"TESTOBJ1", 1).err(), Some(SyncError::Corrupted));
        let future = object(&Header { format: 2, nonce: [0; 4] }, &[0; TAG_LEN]);
        assert_eq!(parse::<Header>(&future, b"TESTOBJ1", 1).err(), Some(SyncError::Unsupported(2)));
        let mut not_json = header_bytes(b"TESTOBJ1", &"x").unwrap();
        not_json.extend_from_slice(&[0; TAG_LEN]);
        assert_eq!(parse::<Header>(&not_json, b"TESTOBJ1", 1).err(), Some(SyncError::Corrupted));
        let bad_nonce = {
            let mut bytes = header_bytes(b"TESTOBJ1", &serde_json::json!({ "format": 1, "nonce": "AAAA" })).unwrap();
            bytes.extend_from_slice(&[0; TAG_LEN]);
            bytes
        };
        assert_eq!(parse::<Header>(&bad_nonce, b"TESTOBJ1", 1).err(), Some(SyncError::Corrupted), "a nonce of the wrong length");
    }

    #[test]
    fn hkdf_separates_by_salt_and_info_and_random_differs() {
        let a = hkdf32(b"salt", b"ikm", b"one");
        assert_eq!(*a, *hkdf32(b"salt", b"ikm", b"one"));
        assert_ne!(*a, *hkdf32(b"salt", b"ikm", b"two"));
        assert_ne!(*a, *hkdf32(b"other", b"ikm", b"one"));
        assert_ne!(random::<16>().unwrap(), random::<16>().unwrap());
    }
}
