//! The invitation one device of a space shows another: the storage, its credentials and the sync
//! key, as one text (and the QR code of that text). It holds everything but a master password,
//! so the device that scans it still needs the master password of a device in the space to open
//! that device's keyring.

use std::fmt;

use data_encoding::BASE64URL_NOPAD;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{StorageConfig, SyncError, SyncKey};

const INVITE_PREFIX: &str = "lockra-invite:1:";

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

    /// The invitation from its text (surrounding white space does not matter).
    pub fn from_text(text: &str) -> Result<Self, SyncError> {
        let body = text.trim().strip_prefix(INVITE_PREFIX).ok_or(SyncError::BadInvite)?;
        let json = Zeroizing::new(BASE64URL_NOPAD.decode(body.as_bytes()).map_err(|_| SyncError::BadInvite)?);
        let wire: Wire = serde_json::from_slice(&json).map_err(|_| SyncError::BadInvite)?;
        let sync_key = SyncKey::from_text(&wire.sync_key).map_err(|_| SyncError::BadInvite)?;
        Ok(Self { storage: wire.storage, sync_key })
    }
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
