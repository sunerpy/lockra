//! What goes over the wire in the clear: the preamble every connection and discovery probe opens
//! with, and the hub's answer to a probe. A preamble is 38 bytes: `LKLN`, the version, the kind, a
//! random nonce and a hint, the first 16 bytes of HMAC-SHA256(key, "lockra-lan-hint" ‖ nonce). The
//! hub finds the key a connection uses by its hint, among at most a few dozen, and drops the
//! connection when none matches; the nonce makes every hint new, so the hint names nothing an
//! onlooker could follow from one connection to the next.

use hmac::{Hmac, KeyInit, Mac as _};
use sha2::Sha256;

use crate::Key;

const MAGIC: [u8; 4] = *b"LKLN";
const VERSION: u8 = 1;
/// The preamble's length.
pub const PREAMBLE_LEN: usize = 38;
/// The length of the hub's answer to a probe: `LKLN`, the version, kind 2 and its hint.
pub const ANSWER_LEN: usize = 22;
const HINT: &[u8] = b"lockra-lan-hint";
const ANSWER: &[u8] = b"lockra-lan-answer";

/// What a connection is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A paired device's runs, under its own key.
    Session,
    /// A device asking to pair, under the offer's key.
    Pairing,
}

/// A connection's or a probe's opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preamble {
    pub kind: Kind,
    pub nonce: [u8; 16],
    pub hint: [u8; 16],
}

fn mac(key: &Key, label: &[u8], nonce: &[u8; 16]) -> Hmac<Sha256> {
    // HMAC takes a key of any length (RFC 2104 §2), so `new_from_slice` cannot fail here.
    #[allow(clippy::expect_used)]
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_ref()).expect("HMAC accepts keys of any length");
    mac.update(label);
    mac.update(nonce);
    mac
}

fn truncated(mac: Hmac<Sha256>) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(&mac.finalize().into_bytes()[..16]);
    out
}

impl Preamble {
    /// A new preamble for `kind` under `key`, with a fresh nonce.
    pub fn new(kind: Kind, key: &Key) -> Result<Self, lockra_sync::SyncError> {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).map_err(|_| lockra_sync::SyncError::Random)?;
        Ok(Self { kind, nonce, hint: truncated(mac(key, HINT, &nonce)) })
    }

    pub fn to_bytes(self) -> [u8; PREAMBLE_LEN] {
        let mut bytes = [0u8; PREAMBLE_LEN];
        bytes[..4].copy_from_slice(&MAGIC);
        bytes[4] = VERSION;
        bytes[5] = match self.kind {
            Kind::Session => 0,
            Kind::Pairing => 1,
        };
        bytes[6..22].copy_from_slice(&self.nonce);
        bytes[22..].copy_from_slice(&self.hint);
        bytes
    }

    /// A preamble of this version, or nothing.
    pub fn parse(bytes: &[u8; PREAMBLE_LEN]) -> Option<Self> {
        if bytes[..4] != MAGIC || bytes[4] != VERSION {
            return None;
        }
        let kind = match bytes[5] {
            0 => Kind::Session,
            1 => Kind::Pairing,
            _ => return None,
        };
        let (mut nonce, mut hint) = ([0u8; 16], [0u8; 16]);
        nonce.copy_from_slice(&bytes[6..22]);
        hint.copy_from_slice(&bytes[22..]);
        Some(Self { kind, nonce, hint })
    }

    /// The hint was made with `key` (compared in constant time).
    pub fn made_with(&self, key: &Key) -> bool {
        mac(key, HINT, &self.nonce).verify_truncated_left(&self.hint).is_ok()
    }

    /// The hub's answer to this probe, from the same key: the device knows its hub by it.
    pub fn answer(&self, key: &Key) -> [u8; ANSWER_LEN] {
        let mut bytes = [0u8; ANSWER_LEN];
        bytes[..4].copy_from_slice(&MAGIC);
        bytes[4] = VERSION;
        bytes[5] = 2;
        bytes[6..].copy_from_slice(&truncated(mac(key, ANSWER, &self.nonce)));
        bytes
    }

    /// `bytes` is the answer of a hub that knows `key` to this probe.
    pub fn answered_by(&self, bytes: &[u8], key: &Key) -> bool {
        bytes.len() == ANSWER_LEN
            && bytes[..4] == MAGIC
            && bytes[4] == VERSION
            && bytes[5] == 2
            && mac(key, ANSWER, &self.nonce).verify_truncated_left(&bytes[6..]).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::*;

    #[test]
    fn a_preamble_names_its_key_to_whoever_holds_it_and_nothing_else() {
        let (key, other) = (Zeroizing::new([1u8; 32]), Zeroizing::new([2u8; 32]));
        let preamble = Preamble::new(Kind::Session, &key).unwrap();
        let bytes = preamble.to_bytes();
        assert_eq!(&bytes[..6], b"LKLN\x01\x00");
        let parsed = Preamble::parse(&bytes).unwrap();
        assert_eq!(parsed, preamble);
        assert!(parsed.made_with(&key) && !parsed.made_with(&other));
        // A new nonce every time: two connections under one key share no bytes an onlooker could
        // match.
        let again = Preamble::new(Kind::Session, &key).unwrap();
        assert_ne!((again.nonce, again.hint), (preamble.nonce, preamble.hint));
        assert_eq!(Preamble::parse(&Preamble::new(Kind::Pairing, &key).unwrap().to_bytes()).unwrap().kind, Kind::Pairing);
        // Another magic, version or kind is no preamble.
        for (at, value) in [(0, b'X'), (4, 2), (5, 7)] {
            let mut changed = bytes;
            changed[at] = value;
            assert_eq!(Preamble::parse(&changed), None, "byte {at}");
        }
        // A changed hint no longer names the key.
        let mut changed = bytes;
        changed[30] ^= 1;
        assert!(!Preamble::parse(&changed).unwrap().made_with(&key));
    }

    #[test]
    fn a_hub_answers_a_probe_in_a_way_only_the_same_key_reads() {
        let (key, other) = (Zeroizing::new([1u8; 32]), Zeroizing::new([2u8; 32]));
        let probe = Preamble::new(Kind::Session, &key).unwrap();
        let answer = probe.answer(&key);
        assert!(probe.answered_by(&answer, &key));
        assert!(!probe.answered_by(&answer, &other));
        assert!(!probe.answered_by(&probe.answer(&other), &key));
        // Another probe's answer is not this one's.
        let later = Preamble::new(Kind::Session, &key).unwrap();
        assert!(!later.answered_by(&answer, &key));
        assert!(!probe.answered_by(&answer[..21], &key));
        assert!(!probe.answered_by(&probe.to_bytes(), &key));
    }
}
