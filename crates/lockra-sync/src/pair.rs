//! Pairing a device with a LAN hub (lockra-lan does the network part). The offer a hub shows is
//! `lockra-pair:1:` and Base64url of JSON with the hub's id and name, the space's id, where the hub
//! listens and the offer's key, good for two minutes and for one handshake. It is a secret while it
//! stands: whoever has it can ask to pair, and the user at the hub still says yes or no after
//! comparing the check code. The welcome is what the hub then hands the device.

use std::fmt;
use std::net::IpAddr;

use data_encoding::{BASE64, BASE64URL_NOPAD};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{SpaceKeys, StorageConfig, SyncError, SyncKey};

/// A 32-byte key: a device's with its hub, or an offer's.
pub type LanKey = Zeroizing<[u8; 32]>;

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
    /// The space the hub keeps: a device in another one is not asked in.
    pub space_id: Uuid,
    /// Where the hub listens.
    pub addrs: Vec<IpAddr>,
    pub port: u16,
    /// The offer's key: a device that has it may ask to pair.
    pub key: LanKey,
    /// Until when, Unix milliseconds.
    pub expires_at_ms: u64,
}

impl fmt::Debug for PairOffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PairOffer")
            .field("hub_id", &self.hub_id)
            .field("hub_name", &self.hub_name)
            .field("space_id", &self.space_id)
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
    space_id: Uuid,
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
            space_id: self.space_id,
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
        Ok(Self {
            hub_id: wire.hub_id,
            hub_name: wire.name,
            space_id: wire.space_id,
            addrs: wire.addrs,
            port: wire.port,
            key,
            expires_at_ms: wire.expires_at_ms,
        })
    }
}

/// What a hub hands a device the user let pair: the device's own key with the hub, where the hub
/// is, and the space: its keys, and the storage of the user's own with its credentials when the
/// space has one. A secret, carried inside the pairing's Noise channel.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanWelcome {
    /// The device, as the hub knows it.
    pub peer_id: Uuid,
    /// The device's key with the hub (Base64 of 32 bytes).
    pub key: Zeroizing<String>,
    pub hub_id: Uuid,
    pub hub_name: String,
    pub port: u16,
    pub addrs: Vec<IpAddr>,
    pub space_id: Uuid,
    /// The space's data key (Base64).
    pub data_key: Zeroizing<String>,
    /// The space's sync key (`LKS1-…`).
    pub sync_key: Zeroizing<String>,
    /// The space's storage of the user's own, when it has one.
    #[serde(default)]
    pub cloud: Option<StorageConfig>,
}

impl fmt::Debug for LanWelcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LanWelcome")
            .field("peer_id", &self.peer_id)
            .field("hub_id", &self.hub_id)
            .field("hub_name", &self.hub_name)
            .field("space_id", &self.space_id)
            .field("cloud", &self.cloud.is_some())
            .finish_non_exhaustive()
    }
}

/// The longest welcome read.
const MAX_WELCOME: usize = 64 * 1024;

impl LanWelcome {
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        // Serializing plain data to JSON cannot fail.
        #[allow(clippy::expect_used)]
        Zeroizing::new(serde_json::to_vec(self).expect("a welcome serializes"))
    }

    /// A welcome whose parts fit together: the device's key is 32 bytes, the data key opens the
    /// space it names, and the sync key is that space's.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SyncError> {
        if bytes.len() > MAX_WELCOME {
            return Err(SyncError::BadInvite);
        }
        let welcome: Self = serde_json::from_slice(bytes).map_err(|_| SyncError::BadInvite)?;
        welcome.device_key()?;
        welcome.keys()?;
        if welcome.sync_key()?.space_id() != welcome.space_id || welcome.port == 0 {
            return Err(SyncError::BadInvite);
        }
        Ok(welcome)
    }

    /// The device's key with the hub.
    pub fn device_key(&self) -> Result<LanKey, SyncError> {
        let bytes = Zeroizing::new(BASE64.decode(self.key.as_bytes()).map_err(|_| SyncError::BadInvite)?);
        let mut key = Zeroizing::new([0u8; 32]);
        if bytes.len() != key.len() {
            return Err(SyncError::BadInvite);
        }
        key.copy_from_slice(&bytes);
        Ok(key)
    }

    pub fn keys(&self) -> Result<SpaceKeys, SyncError> {
        SpaceKeys::from_parts(self.space_id, &self.data_key).map_err(|_| SyncError::BadInvite)
    }

    pub fn sync_key(&self) -> Result<SyncKey, SyncError> {
        SyncKey::from_text(&self.sync_key).map_err(|_| SyncError::BadInvite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer() -> PairOffer {
        PairOffer {
            hub_id: Uuid::from_u128(1),
            hub_name: "Desktop".into(),
            space_id: Uuid::from_u128(2),
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

    fn welcome() -> LanWelcome {
        let sync_key = SyncKey::generate().unwrap();
        let keys = SpaceKeys::generate(sync_key.space_id()).unwrap();
        LanWelcome {
            peer_id: Uuid::from_u128(5),
            key: Zeroizing::new(BASE64.encode(&[9u8; 32])),
            hub_id: Uuid::from_u128(1),
            hub_name: "Desktop".into(),
            port: 47_100,
            addrs: vec!["192.168.1.20".parse().unwrap()],
            space_id: keys.space_id(),
            data_key: keys.data_key_text(),
            sync_key: sync_key.to_text(),
            cloud: None,
        }
    }

    #[test]
    fn a_welcome_reads_back_when_its_parts_fit_together() {
        let good = welcome();
        let back = LanWelcome::from_bytes(&good.to_bytes()).unwrap();
        assert_eq!(back, good);
        assert_eq!(*back.device_key().unwrap(), [9u8; 32]);
        assert_eq!(back.keys().unwrap().space_id(), good.space_id);
        let logged = format!("{good:?}");
        assert!(!logged.contains(good.data_key.as_str()) && !logged.contains(good.sync_key.as_str()) && !logged.contains(good.key.as_str()), "{logged}");
        // Another space's sync key, a short device key, no port, garbage, too long.
        let other = LanWelcome { sync_key: SyncKey::generate().unwrap().to_text(), ..good.clone() };
        let short = LanWelcome { key: Zeroizing::new(BASE64.encode(&[9u8; 16])), ..good.clone() };
        let portless = LanWelcome { port: 0, ..good.clone() };
        let wrong_keys = LanWelcome { data_key: Zeroizing::new("not base64!".into()), ..good.clone() };
        for bad in [other, short, portless, wrong_keys] {
            assert_eq!(LanWelcome::from_bytes(&bad.to_bytes()).err(), Some(SyncError::BadInvite));
        }
        assert_eq!(LanWelcome::from_bytes(b"{}").err(), Some(SyncError::BadInvite));
        assert_eq!(LanWelcome::from_bytes(&vec![b' '; MAX_WELCOME + 1]).err(), Some(SyncError::BadInvite));
    }
}
