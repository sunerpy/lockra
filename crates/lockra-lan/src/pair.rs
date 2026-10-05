//! The pairing offer a hub shows: `lockra-pair:1:` and Base64url of JSON with the hub's id and
//! name, where it listens and the offer's key, good for two minutes and for one handshake. It is a
//! secret while it stands: whoever has it can ask to pair, and the user at the hub still says yes
//! or no after comparing the check code.

use std::fmt;
use std::net::IpAddr;

use data_encoding::{BASE64, BASE64URL_NOPAD};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::Key;

/// The beginning of every pairing offer's text.
pub const PAIR_PREFIX: &str = "lockra-pair:1:";
/// The longest text taken as an offer.
const MAX_TEXT: usize = 4 * 1024;

/// A hub's offer to pair.
#[derive(Clone, PartialEq, Eq)]
pub struct PairOffer {
    pub hub_id: Uuid,
    /// The hub's name, for the device's user.
    pub hub_name: String,
    /// Where the hub listens.
    pub addrs: Vec<IpAddr>,
    pub port: u16,
    /// The offer's key: a device that has it may ask to pair.
    pub key: Key,
    /// Until when, Unix milliseconds.
    pub expires_at_ms: u64,
}

impl fmt::Debug for PairOffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PairOffer")
            .field("hub_id", &self.hub_id)
            .field("hub_name", &self.hub_name)
            .field("addrs", &self.addrs)
            .field("port", &self.port)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize)]
struct Wire {
    hub_id: Uuid,
    name: String,
    addrs: Vec<IpAddr>,
    port: u16,
    key: Zeroizing<String>,
    expires_at_ms: u64,
}

/// Why a text is no offer to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PairTextError {
    #[error("not a Lockra pairing offer")]
    NotOne,
    #[error("the pairing offer expired")]
    Expired,
}

impl PairOffer {
    pub fn to_text(&self) -> Zeroizing<String> {
        let wire = Wire {
            hub_id: self.hub_id,
            name: self.hub_name.clone(),
            addrs: self.addrs.clone(),
            port: self.port,
            key: Zeroizing::new(BASE64.encode(self.key.as_ref())),
            expires_at_ms: self.expires_at_ms,
        };
        // Serializing plain data to JSON cannot fail.
        #[allow(clippy::expect_used)]
        let json = Zeroizing::new(serde_json::to_vec(&wire).expect("an offer serializes"));
        Zeroizing::new(format!("{PAIR_PREFIX}{}", BASE64URL_NOPAD.encode(&json)))
    }

    /// The offer in `text` (spaces around it ignored), if it still stands at `now_ms`.
    pub fn from_text(text: &str, now_ms: u64) -> Result<Self, PairTextError> {
        let text = text.trim();
        let body = text.strip_prefix(PAIR_PREFIX).filter(|_| text.len() <= MAX_TEXT).ok_or(PairTextError::NotOne)?;
        let json = Zeroizing::new(BASE64URL_NOPAD.decode(body.as_bytes()).map_err(|_| PairTextError::NotOne)?);
        let wire: Wire = serde_json::from_slice(&json).map_err(|_| PairTextError::NotOne)?;
        let bytes = Zeroizing::new(BASE64.decode(wire.key.as_bytes()).map_err(|_| PairTextError::NotOne)?);
        let mut key = Zeroizing::new([0u8; 32]);
        if bytes.len() != key.len() || wire.port == 0 || wire.addrs.is_empty() {
            return Err(PairTextError::NotOne);
        }
        key.copy_from_slice(&bytes);
        if now_ms >= wire.expires_at_ms {
            return Err(PairTextError::Expired);
        }
        Ok(Self { hub_id: wire.hub_id, hub_name: wire.name, addrs: wire.addrs, port: wire.port, key, expires_at_ms: wire.expires_at_ms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer() -> PairOffer {
        PairOffer {
            hub_id: Uuid::from_u128(1),
            hub_name: "Desktop".into(),
            addrs: vec!["192.168.1.20".parse().unwrap(), "fe80::1".parse().unwrap()],
            port: 47_100,
            key: Zeroizing::new([3u8; 32]),
            expires_at_ms: 1_000_000,
        }
    }

    #[test]
    fn an_offer_reads_back_until_it_expires() {
        let text = offer().to_text();
        assert!(text.starts_with(PAIR_PREFIX));
        assert!(!text.contains("Desktop"), "not readable at a glance");
        assert_eq!(PairOffer::from_text(&format!("  {}\n", text.as_str()), 999_999).unwrap(), offer());
        assert_eq!(PairOffer::from_text(&text, 1_000_000).err(), Some(PairTextError::Expired));
        assert!(!format!("{:?}", offer()).contains("3, 3"), "the key stays out of logs");
    }

    #[test]
    fn anything_else_is_no_offer() {
        let good = offer().to_text();
        let body = &good[PAIR_PREFIX.len()..];
        let json = String::from_utf8(BASE64URL_NOPAD.decode(body.as_bytes()).unwrap()).unwrap();
        let wrapped = |json: &str| format!("{PAIR_PREFIX}{}", BASE64URL_NOPAD.encode(json.as_bytes()));
        let short_key = json.replace(&BASE64.encode(&[3u8; 32]), &BASE64.encode(&[3u8; 16]));
        let no_addrs = json.replace(r#""addrs":["192.168.1.20","fe80::1"]"#, r#""addrs":[]"#);
        let no_port = json.replace(r#""port":47100"#, r#""port":0"#);
        for text in [
            String::new(),
            "lockra-invite:1:abc".into(),
            format!("{PAIR_PREFIX}not base64!"),
            wrapped("{}"),
            wrapped(&short_key),
            wrapped(&no_addrs),
            wrapped(&no_port),
            format!("{PAIR_PREFIX}{}", "A".repeat(MAX_TEXT)),
        ] {
            assert_eq!(PairOffer::from_text(&text, 0).err(), Some(PairTextError::NotOne), "{}", &text[..text.len().min(40)]);
        }
    }
}
