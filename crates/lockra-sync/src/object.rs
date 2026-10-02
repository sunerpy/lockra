//! A device's snapshot: its whole replica, encrypted under the space's data key.
//!
//! The header holds only what finding the key and checking the object's place need: the format,
//! the space and the device tag, and the nonce. The sequence number, the time, the device's name
//! and the replica are inside the ciphertext, padded to a multiple of [`PAD_TO`] bytes so that the
//! size says little about the number of accounts.

use std::fmt;

use chacha20poly1305::XNonce;
use chacha20poly1305::aead::{Aead, Payload};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::frame::{NONCE_LEN, Parsed, b64, cipher, header_bytes, parse, random};
use crate::{SpaceKeys, SyncError};

const SNAPSHOT_MAGIC: &[u8; 8] = b"LKSDEVS1";
const SNAPSHOT_FORMAT: u32 = 1;
/// The plaintext of a snapshot is padded to a multiple of this.
pub const PAD_TO: usize = 4096;
/// Device names are cut to this many characters.
const MAX_NAME_CHARS: usize = 64;

/// One device's snapshot of its replica.
#[derive(Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Counts the device's writes: a smaller number than one seen before is a rollback.
    pub seq: u64,
    /// When it was written, Unix milliseconds.
    pub written_at_ms: u64,
    /// The device's name as the user knows it ("Pixel 8", "Desktop").
    pub device_name: String,
    /// The replica, as the caller serializes it (its secrets included).
    pub payload: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("seq", &self.seq)
            .field("written_at_ms", &self.written_at_ms)
            .field("device_name", &self.device_name)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

/// `name` as a snapshot carries it: cut to [`MAX_NAME_CHARS`] characters.
pub(crate) fn device_name(name: &str) -> String {
    name.chars().take(MAX_NAME_CHARS).collect()
}

#[derive(Serialize, Deserialize)]
struct SnapshotHeader {
    format: u32,
    space_id: Uuid,
    tag: String,
    #[serde(with = "b64")]
    nonce: [u8; NONCE_LEN],
}

#[derive(Serialize, Deserialize)]
struct Meta {
    seq: u64,
    written_at_ms: u64,
    device_name: String,
}

/// Encrypt `snapshot` as the object of device `tag` in `keys`' space.
pub fn seal_snapshot(keys: &SpaceKeys, tag: &str, snapshot: &Snapshot) -> Result<Vec<u8>, SyncError> {
    let header = SnapshotHeader { format: SNAPSHOT_FORMAT, space_id: keys.space_id(), tag: tag.to_owned(), nonce: random()? };
    let mut object = header_bytes(SNAPSHOT_MAGIC, &header)?;
    let name = device_name(&snapshot.device_name);
    let meta = serde_json::to_vec(&Meta { seq: snapshot.seq, written_at_ms: snapshot.written_at_ms, device_name: name }).map_err(|_| SyncError::Corrupted)?;
    let meta_len = u32::try_from(meta.len()).map_err(|_| SyncError::Corrupted)?;
    let payload_len = u32::try_from(snapshot.payload.len()).map_err(|_| SyncError::Corrupted)?;
    let mut plain = Zeroizing::new(Vec::with_capacity(8 + meta.len() + snapshot.payload.len() + PAD_TO));
    plain.extend_from_slice(&meta_len.to_le_bytes());
    plain.extend_from_slice(&meta);
    plain.extend_from_slice(&payload_len.to_le_bytes());
    plain.extend_from_slice(&snapshot.payload);
    let padded = plain.len().div_ceil(PAD_TO) * PAD_TO;
    plain.resize(padded, 0);
    let ciphertext =
        cipher(&keys.snapshot_key()).encrypt(&XNonce::from(header.nonce), Payload { msg: &plain, aad: &object }).map_err(|_| SyncError::Corrupted)?;
    object.extend_from_slice(&ciphertext);
    Ok(object)
}

/// Decrypt the object stored under device `expected_tag`. An object of another space, or one
/// moved here from another device's name, is [`SyncError::Misplaced`]; one that does not
/// authenticate (altered, or under another data key) is [`SyncError::Corrupted`].
pub fn open_snapshot(keys: &SpaceKeys, expected_tag: &str, bytes: &[u8]) -> Result<Snapshot, SyncError> {
    let parsed: Parsed<'_, SnapshotHeader> = parse(bytes, SNAPSHOT_MAGIC, SNAPSHOT_FORMAT)?;
    if parsed.header.space_id != keys.space_id() || parsed.header.tag != expected_tag {
        return Err(SyncError::Misplaced);
    }
    let plain = Zeroizing::new(
        cipher(&keys.snapshot_key())
            .decrypt(&XNonce::from(parsed.header.nonce), Payload { msg: parsed.ciphertext, aad: parsed.associated })
            .map_err(|_| SyncError::Corrupted)?,
    );
    let (meta_bytes, rest) = take_block(&plain)?;
    let (payload, _padding) = take_block(rest)?;
    let meta: Meta = serde_json::from_slice(meta_bytes).map_err(|_| SyncError::Corrupted)?;
    Ok(Snapshot { seq: meta.seq, written_at_ms: meta.written_at_ms, device_name: meta.device_name, payload: Zeroizing::new(payload.to_vec()) })
}

/// A length-prefixed block and what follows it.
fn take_block(bytes: &[u8]) -> Result<(&[u8], &[u8]), SyncError> {
    let length: [u8; 4] = bytes.get(..4).and_then(|b| b.try_into().ok()).ok_or(SyncError::Corrupted)?;
    let length = usize::try_from(u32::from_le_bytes(length)).map_err(|_| SyncError::Corrupted)?;
    let block = bytes.get(4..4 + length).ok_or(SyncError::Corrupted)?;
    Ok((block, &bytes[4 + length..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(payload: &[u8]) -> Snapshot {
        Snapshot { seq: 3, written_at_ms: 1_790_000_000_000, device_name: "Pixel 8".into(), payload: Zeroizing::new(payload.to_vec()) }
    }

    #[test]
    fn a_snapshot_round_trips_under_its_space_and_tag() {
        let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
        let tag = keys.device_tag(1);
        let original = snapshot(br#"{"entries":[]}"#);
        let object = seal_snapshot(&keys, &tag, &original).unwrap();
        assert_eq!(open_snapshot(&keys, &tag, &object).unwrap(), original);
        // Nothing of the payload or the name is visible.
        let text = String::from_utf8_lossy(&object);
        assert!(!text.contains("entries") && !text.contains("Pixel"), "{text}");
    }

    #[test]
    fn sizes_are_padded_so_they_say_little_about_the_number_of_accounts() {
        let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
        let tag = keys.device_tag(1);
        let small = seal_snapshot(&keys, &tag, &snapshot(&[1; 10])).unwrap();
        let larger = seal_snapshot(&keys, &tag, &snapshot(&[1; 3_000])).unwrap();
        assert_eq!(small.len(), larger.len(), "both fit in one padded block");
        let big = seal_snapshot(&keys, &tag, &snapshot(&[1; 5_000])).unwrap();
        assert_eq!(big.len() - small.len(), PAD_TO);
        // A long name is cut.
        let long = Snapshot { device_name: "x".repeat(200), ..snapshot(b"{}") };
        let opened = open_snapshot(&keys, &tag, &seal_snapshot(&keys, &tag, &long).unwrap()).unwrap();
        assert_eq!(opened.device_name.chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn a_moved_altered_or_foreign_snapshot_is_refused() {
        let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
        let tag = keys.device_tag(1);
        let object = seal_snapshot(&keys, &tag, &snapshot(b"{}")).unwrap();
        // Copied under another device's name.
        assert_eq!(open_snapshot(&keys, &keys.device_tag(2), &object).err(), Some(SyncError::Misplaced));
        // Read in another space, even one with the same data key text but another id.
        let other_space = SpaceKeys::from_parts(Uuid::new_v4(), &keys.data_key_text()).unwrap();
        assert_eq!(open_snapshot(&other_space, &tag, &object).err(), Some(SyncError::Misplaced));
        // Altered ciphertext or header.
        let mut altered = object.clone();
        let last = altered.len() - 1;
        altered[last] ^= 1;
        assert_eq!(open_snapshot(&keys, &tag, &altered).err(), Some(SyncError::Corrupted));
        // A data key that is not the space's (a rotated key): the authentication fails.
        let impostor = SpaceKeys::generate(keys.space_id()).unwrap();
        assert_eq!(open_snapshot(&impostor, &tag, &object).err(), Some(SyncError::Corrupted));
        assert_eq!(open_snapshot(&keys, &tag, b"LKSKEYR1....").err(), Some(SyncError::NotLockra));
    }

    #[test]
    fn length_blocks_must_fit_inside_the_plaintext() {
        assert_eq!(take_block(&[1, 0, 0, 0, 9]).unwrap(), (&[9][..], &[][..]));
        assert_eq!(take_block(&[5, 0, 0, 0, 9]).err(), Some(SyncError::Corrupted));
        assert_eq!(take_block(&[1, 0]).err(), Some(SyncError::Corrupted));
    }
}
