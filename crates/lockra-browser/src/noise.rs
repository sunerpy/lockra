//! The Noise handshakes and the encrypted channel (docs/formats.md §10). Pairing is
//! `Noise_XX_25519_AESGCM_SHA256`: neither side knows the other yet, both end up with the other's
//! static key and show a six-digit code drawn from the handshake hash, which the user compares.
//! Each session is `Noise_KK_25519_AESGCM_SHA256` between the paired static keys, with fresh
//! ephemeral keys: forward secrecy per connection. Both bind the prologue [`PROLOGUE`].
//!
//! An application message is sent as at most [`MAX_FRAGMENTS`] Noise messages of at most 65 535
//! bytes, and is at most [`MAX_MESSAGE`] bytes: eight full fragments carry 524 144 bytes, a little
//! under that. Each fragment's plaintext starts with a flag byte, 1 when more fragments follow and
//! 0 on the last.

use hkdf::Hkdf;
use sha2::Sha256;
use snow::{Builder, HandshakeState, TransportState};
use zeroize::Zeroizing;

/// The pairing handshake.
pub const PAIRING_PATTERN: &str = "Noise_XX_25519_AESGCM_SHA256";
/// The session handshake.
pub const SESSION_PATTERN: &str = "Noise_KK_25519_AESGCM_SHA256";
/// Both handshakes' prologue: a handshake of another protocol or version fails.
pub const PROLOGUE: &[u8] = b"lockra-browser/1";
/// The longest Noise message.
pub const MAX_NOISE: usize = 65_535;
/// The most fragments of one application message.
pub const MAX_FRAGMENTS: usize = 8;
/// The longest application message.
pub const MAX_MESSAGE: usize = 512 * 1024;
/// HKDF's `info` for the pairing code.
pub const PAIRING_CODE_INFO: &[u8] = b"lockra pairing code";

const TAG: usize = 16;
/// The most application bytes one fragment carries: the Noise message less the tag and the flag.
pub const FRAGMENT: usize = MAX_NOISE - TAG - 1;

/// A handshake or channel that failed: the connection cannot go on.
#[derive(Debug, thiserror::Error)]
pub enum NoiseError {
    /// Noise refused it (a bad key, a message that does not decrypt, the wrong step).
    #[error("noise: {0}")]
    Noise(#[from] snow::Error),
    /// An application message over [`MAX_MESSAGE`] or [`MAX_FRAGMENTS`].
    #[error("a message over the limit")]
    TooLong,
    /// A fragment without its flag byte, or with another flag.
    #[error("a malformed fragment")]
    Malformed,
    /// The other side's static key is not a 32-byte key.
    #[error("no usable static key")]
    NoKey,
}

/// The app's side of a handshake.
pub struct Responder {
    state: HandshakeState,
}

impl std::fmt::Debug for Responder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Responder").field("finished", &self.state.is_handshake_finished()).finish_non_exhaustive()
    }
}

impl Responder {
    /// The pairing handshake (XX) under the app's static key.
    pub fn pairing(local: &[u8; 32]) -> Result<Self, NoiseError> {
        let state = Builder::new(PAIRING_PATTERN.parse()?).prologue(PROLOGUE)?.local_private_key(local)?.build_responder()?;
        Ok(Self { state })
    }

    /// A session's handshake (KK) between the app's static key and the paired extension's.
    pub fn session(local: &[u8; 32], remote: &[u8; 32]) -> Result<Self, NoiseError> {
        let state = Builder::new(SESSION_PATTERN.parse()?).prologue(PROLOGUE)?.local_private_key(local)?.remote_public_key(remote)?.build_responder()?;
        Ok(Self { state })
    }

    /// Read the other side's handshake message; its payload.
    pub fn read(&mut self, message: &[u8]) -> Result<Zeroizing<Vec<u8>>, NoiseError> {
        let mut payload = Zeroizing::new(vec![0u8; MAX_NOISE]);
        let length = self.state.read_message(message, &mut payload)?;
        payload.truncate(length);
        Ok(payload)
    }

    /// The next handshake message, carrying `payload`.
    pub fn write(&mut self, payload: &[u8]) -> Result<Vec<u8>, NoiseError> {
        let mut message = vec![0u8; MAX_NOISE];
        let length = self.state.write_message(payload, &mut message)?;
        message.truncate(length);
        Ok(message)
    }

    /// The handshake is done.
    pub fn finished(&self) -> bool {
        self.state.is_handshake_finished()
    }

    /// The other side's static key, once the handshake has carried it.
    pub fn remote_static(&self) -> Option<[u8; 32]> {
        self.state.get_remote_static().and_then(|key| key.try_into().ok())
    }

    /// The six digits both sides show for this handshake.
    pub fn pairing_code(&self) -> String {
        pairing_code(self.state.get_handshake_hash())
    }

    /// The encrypted channel, once the handshake is done.
    pub fn into_channel(self) -> Result<Channel, NoiseError> {
        Ok(Channel { transport: self.state.into_transport_mode()?, inbox: Zeroizing::new(Vec::new()), fragments: 0 })
    }
}

/// The six-digit code of a handshake hash: HKDF-SHA256 of the hash (no salt, `info`
/// [`PAIRING_CODE_INFO`]), its first four bytes as a big-endian number, modulo a million, with
/// leading zeros.
pub fn pairing_code(handshake_hash: &[u8]) -> String {
    let mut okm = [0u8; 4];
    // Four bytes are far within what HKDF-SHA256 can expand to.
    #[allow(clippy::expect_used)]
    Hkdf::<Sha256>::new(None, handshake_hash).expand(PAIRING_CODE_INFO, &mut okm).expect("a 4-byte output");
    format!("{:06}", u32::from_be_bytes(okm) % 1_000_000)
}

/// The encrypted channel of a finished handshake.
pub struct Channel {
    transport: TransportState,
    inbox: Zeroizing<Vec<u8>>,
    fragments: usize,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel").field("fragments", &self.fragments).finish_non_exhaustive()
    }
}

impl Channel {
    /// The other side's static key.
    pub fn remote_static(&self) -> Option<[u8; 32]> {
        self.transport.get_remote_static().and_then(|key| key.try_into().ok())
    }

    /// `message` as the Noise messages to send, in order.
    pub fn seal(&mut self, message: &[u8]) -> Result<Vec<Vec<u8>>, NoiseError> {
        if message.len() > MAX_MESSAGE {
            return Err(NoiseError::TooLong);
        }
        let chunks: Vec<&[u8]> = if message.is_empty() { vec![&[][..]] } else { message.chunks(FRAGMENT).collect() };
        if chunks.len() > MAX_FRAGMENTS {
            return Err(NoiseError::TooLong);
        }
        let last = chunks.len() - 1;
        let mut out = Vec::with_capacity(chunks.len());
        for (index, chunk) in chunks.into_iter().enumerate() {
            let mut plain = Zeroizing::new(Vec::with_capacity(chunk.len() + 1));
            plain.push(u8::from(index != last));
            plain.extend_from_slice(chunk);
            let mut sealed = vec![0u8; plain.len() + TAG];
            let length = self.transport.write_message(&plain, &mut sealed)?;
            sealed.truncate(length);
            out.push(sealed);
        }
        Ok(out)
    }

    /// Take in one Noise message; the application message once its last fragment came.
    pub fn open(&mut self, sealed: &[u8]) -> Result<Option<Zeroizing<Vec<u8>>>, NoiseError> {
        let mut plain = Zeroizing::new(vec![0u8; sealed.len()]);
        let length = self.transport.read_message(sealed, &mut plain)?;
        plain.truncate(length);
        let (&flag, chunk) = plain.split_first().ok_or(NoiseError::Malformed)?;
        if flag > 1 {
            return Err(NoiseError::Malformed);
        }
        self.fragments += 1;
        if self.fragments > MAX_FRAGMENTS || self.inbox.len() + chunk.len() > MAX_MESSAGE {
            return Err(NoiseError::TooLong);
        }
        self.inbox.extend_from_slice(chunk);
        if flag == 1 {
            return Ok(None);
        }
        self.fragments = 0;
        Ok(Some(std::mem::replace(&mut self.inbox, Zeroizing::new(Vec::new()))))
    }
}

/// The extension's side of the handshakes, for the tests and the vectors the extension's own
/// implementation is checked against (tests/vectors).
#[doc(hidden)]
pub mod initiator {
    use super::{Builder, Channel, NoiseError, PAIRING_PATTERN, PROLOGUE, SESSION_PATTERN};
    use snow::HandshakeState;
    use zeroize::Zeroizing;

    /// An extension's handshake.
    pub struct Initiator {
        state: HandshakeState,
    }

    impl Initiator {
        /// Pairing, under `local`; `ephemeral` fixes the ephemeral key (vectors only).
        pub fn pairing(local: &[u8; 32], ephemeral: Option<&[u8; 32]>) -> Result<Self, NoiseError> {
            let mut builder = Builder::new(PAIRING_PATTERN.parse()?).prologue(PROLOGUE)?.local_private_key(local)?;
            if let Some(key) = ephemeral {
                builder = builder.fixed_ephemeral_key_for_testing_only(key);
            }
            Ok(Self { state: builder.build_initiator()? })
        }

        /// A session with the app's static key `remote`.
        pub fn session(local: &[u8; 32], remote: &[u8; 32], ephemeral: Option<&[u8; 32]>) -> Result<Self, NoiseError> {
            let mut builder = Builder::new(SESSION_PATTERN.parse()?).prologue(PROLOGUE)?.local_private_key(local)?.remote_public_key(remote)?;
            if let Some(key) = ephemeral {
                builder = builder.fixed_ephemeral_key_for_testing_only(key);
            }
            Ok(Self { state: builder.build_initiator()? })
        }

        /// The next handshake message.
        pub fn write(&mut self, payload: &[u8]) -> Result<Vec<u8>, NoiseError> {
            let mut message = vec![0u8; super::MAX_NOISE];
            let length = self.state.write_message(payload, &mut message)?;
            message.truncate(length);
            Ok(message)
        }

        /// Read the app's handshake message.
        pub fn read(&mut self, message: &[u8]) -> Result<Zeroizing<Vec<u8>>, NoiseError> {
            let mut payload = Zeroizing::new(vec![0u8; super::MAX_NOISE]);
            let length = self.state.read_message(message, &mut payload)?;
            payload.truncate(length);
            Ok(payload)
        }

        /// The handshake hash so far.
        pub fn hash(&self) -> Vec<u8> {
            self.state.get_handshake_hash().to_vec()
        }

        /// The app's static key, once the handshake has carried it.
        pub fn remote_static(&self) -> Option<[u8; 32]> {
            self.state.get_remote_static().and_then(|key| key.try_into().ok())
        }

        /// The encrypted channel.
        pub fn into_channel(self) -> Result<Channel, NoiseError> {
            Ok(Channel { transport: self.state.into_transport_mode()?, inbox: Zeroizing::new(Vec::new()), fragments: 0 })
        }
    }

    /// A static key pair, from the default resolver.
    pub fn keypair() -> Result<([u8; 32], [u8; 32]), NoiseError> {
        let pair = Builder::new(SESSION_PATTERN.parse()?).generate_keypair()?;
        let private: [u8; 32] = pair.private.as_slice().try_into().map_err(|_| NoiseError::NoKey)?;
        let public: [u8; 32] = pair.public.as_slice().try_into().map_err(|_| NoiseError::NoKey)?;
        Ok((private, public))
    }
}

#[cfg(test)]
mod tests {
    use super::initiator::{Initiator, keypair};
    use super::*;

    fn paired_channels() -> (Channel, Channel) {
        let (app, _) = keypair().unwrap();
        let (extension, _) = keypair().unwrap();
        let mut responder = Responder::pairing(&app).unwrap();
        let mut initiator = Initiator::pairing(&extension, None).unwrap();
        responder.read(&initiator.write(b"").unwrap()).unwrap();
        initiator.read(&responder.write(b"").unwrap()).unwrap();
        responder.read(&initiator.write(b"").unwrap()).unwrap();
        assert!(responder.finished());
        (responder.into_channel().unwrap(), initiator.into_channel().unwrap())
    }

    #[test]
    fn pairing_shows_the_same_code_on_both_sides_and_learns_both_keys() {
        let (app, app_public) = keypair().unwrap();
        let (extension, extension_public) = keypair().unwrap();
        let mut responder = Responder::pairing(&app).unwrap();
        let mut initiator = Initiator::pairing(&extension, None).unwrap();
        responder.read(&initiator.write(b"").unwrap()).unwrap();
        initiator.read(&responder.write(b"").unwrap()).unwrap();
        assert_eq!(initiator.remote_static(), Some(app_public));
        let payload = responder.read(&initiator.write(b"{\"hi\":1}").unwrap()).unwrap();
        assert_eq!(payload.as_slice(), b"{\"hi\":1}");
        assert_eq!(responder.remote_static(), Some(extension_public));
        let code = responder.pairing_code();
        assert_eq!(code, pairing_code(&initiator.hash()));
        assert!(code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn a_session_needs_the_paired_keys() {
        let (app, app_public) = keypair().unwrap();
        let (extension, extension_public) = keypair().unwrap();
        let (stranger, _) = keypair().unwrap();
        let mut responder = Responder::session(&app, &extension_public).unwrap();
        let mut initiator = Initiator::session(&extension, &app_public, None).unwrap();
        responder.read(&initiator.write(b"").unwrap()).unwrap();
        initiator.read(&responder.write(b"").unwrap()).unwrap();
        assert!(responder.finished());
        let mut app_side = responder.into_channel().unwrap();
        let mut extension_side = initiator.into_channel().unwrap();
        for sealed in extension_side.seal(b"{\"req\":1}").unwrap() {
            if let Some(message) = app_side.open(&sealed).unwrap() {
                assert_eq!(message.as_slice(), b"{\"req\":1}");
            }
        }
        // Another extension's key, or a wrong idea of the app's: the first message fails.
        let mut responder = Responder::session(&app, &extension_public).unwrap();
        let mut impostor = Initiator::session(&stranger, &app_public, None).unwrap();
        assert!(responder.read(&impostor.write(b"").unwrap()).is_err());
        let mut responder = Responder::session(&app, &extension_public).unwrap();
        let (_, other_public) = keypair().unwrap();
        let mut confused = Initiator::session(&extension, &other_public, None).unwrap();
        assert!(responder.read(&confused.write(b"").unwrap()).is_err());
    }

    #[test]
    fn a_long_message_goes_in_fragments_up_to_the_limit() {
        let (mut app, mut extension) = paired_channels();
        let long = vec![b'x'; FRAGMENT * 3 + 5];
        let sealed = app.seal(&long).unwrap();
        assert_eq!(sealed.len(), 4);
        assert!(sealed.iter().all(|s| s.len() <= MAX_NOISE));
        let mut got = None;
        for fragment in &sealed {
            got = extension.open(fragment).unwrap();
        }
        assert_eq!(got.unwrap().as_slice(), long.as_slice());
        // The empty message is one fragment.
        let empty = app.seal(b"").unwrap();
        assert_eq!(empty.len(), 1);
        assert_eq!(extension.open(&empty[0]).unwrap().unwrap().as_slice(), b"");
        // Eight full fragments are the most, a little under 512 KiB.
        const { assert!(MAX_FRAGMENTS * FRAGMENT < MAX_MESSAGE) };
        assert_eq!(app.seal(&vec![0u8; MAX_FRAGMENTS * FRAGMENT]).unwrap().len(), MAX_FRAGMENTS);
        assert!(matches!(app.seal(&vec![0u8; MAX_FRAGMENTS * FRAGMENT + 1]), Err(NoiseError::TooLong)));
        assert!(matches!(app.seal(&vec![0u8; MAX_MESSAGE + 1]), Err(NoiseError::TooLong)));
    }

    #[test]
    fn too_many_fragments_or_a_bad_flag_end_the_channel() {
        let (mut app, mut extension) = paired_channels();
        // Nine fragments that all say more follows.
        for n in 0..=MAX_FRAGMENTS {
            let mut plain = vec![1u8];
            plain.extend_from_slice(b"chunk");
            let mut sealed = vec![0u8; plain.len() + TAG];
            let length = app.transport.write_message(&plain, &mut sealed).unwrap();
            sealed.truncate(length);
            let opened = extension.open(&sealed);
            if n < MAX_FRAGMENTS {
                assert!(opened.unwrap().is_none());
            } else {
                assert!(matches!(opened, Err(NoiseError::TooLong)));
            }
        }
        let (mut app, mut extension) = paired_channels();
        let mut sealed = vec![0u8; 3 + TAG];
        let length = app.transport.write_message(&[7, b'{', b'}'], &mut sealed).unwrap();
        sealed.truncate(length);
        assert!(matches!(extension.open(&sealed), Err(NoiseError::Malformed)));
        // A message that does not decrypt (replayed, altered) fails.
        let (mut app, mut extension) = paired_channels();
        let sealed = app.seal(b"once").unwrap();
        extension.open(&sealed[0]).unwrap();
        assert!(extension.open(&sealed[0]).is_err(), "replayed");
    }

    #[test]
    fn the_pairing_code_is_hkdf_of_the_hash_modulo_a_million() {
        // HKDF-SHA256(ikm = 32 zero bytes, salt = none, info = "lockra pairing code"), first four
        // bytes big-endian: the vector the extension's implementation checks too.
        let code = pairing_code(&[0u8; 32]);
        let mut okm = [0u8; 4];
        Hkdf::<Sha256>::new(None, &[0u8; 32]).expand(PAIRING_CODE_INFO, &mut okm).unwrap();
        assert_eq!(code, format!("{:06}", u32::from_be_bytes(okm) % 1_000_000));
        assert_ne!(pairing_code(&[1u8; 32]), code);
    }
}
