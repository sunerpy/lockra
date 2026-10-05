//! An encrypted connection between a hub and a device: the cleartext preamble, a Noise
//! `NNpsk0_25519_ChaChaPoly_SHA256` handshake under the key the preamble hints at (the preamble
//! and the hub's id are its prologue, so neither can be changed underway), then messages. A message
//! is a 32-bit length, a 32-bit header length, a JSON header and a body of raw bytes, carried in
//! Noise transport messages of at most 65 535 bytes, each after its 16-bit length.

use std::io;

use sha2::{Digest as _, Sha256};
use snow::{HandshakeState, TransportState};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::wire::{Kind, PREAMBLE_LEN, Preamble};
use crate::{Key, MAX_MESSAGE};

const PATTERN: &str = "Noise_NNpsk0_25519_ChaChaPoly_SHA256";
/// The largest Noise message.
const MAX_FRAME: usize = 65_535;
/// The plaintext a transport message carries at most: the frame less its authentication tag.
const MAX_CHUNK: usize = MAX_FRAME - 16;

/// Why a connection ended.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("connection: {0}")]
    Io(#[from] io::Error),
    #[error("not a Lockra LAN connection")]
    NotLockra,
    #[error("no key of this hub's made the hint")]
    UnknownKey,
    #[error("the handshake failed")]
    Handshake,
    #[error("a message did not decrypt or was malformed")]
    Malformed,
    #[error("a message larger than any of the protocol")]
    TooLarge,
    #[error("no random numbers")]
    Random,
}

impl From<lockra_sync::SyncError> for ChannelError {
    fn from(_: lockra_sync::SyncError) -> Self {
        Self::Random
    }
}

/// A connection after its handshake.
pub struct Channel<S> {
    stream: S,
    noise: TransportState,
    hash: [u8; 32],
}

fn handshake(key: &Key, opening: &[u8; PREAMBLE_LEN], hub_id: Uuid, initiator: bool) -> Result<HandshakeState, ChannelError> {
    let mut prologue = opening.to_vec();
    prologue.extend_from_slice(hub_id.as_bytes());
    let params = PATTERN.parse().map_err(|_| ChannelError::Handshake)?;
    let builder = snow::Builder::new(params).psk(0, key).and_then(|b| b.prologue(&prologue)).map_err(|_| ChannelError::Handshake)?;
    if initiator { builder.build_initiator() } else { builder.build_responder() }.map_err(|_| ChannelError::Handshake)
}

async fn write_frame<S: AsyncWrite + Unpin>(stream: &mut S, frame: &[u8]) -> Result<(), ChannelError> {
    let length = u16::try_from(frame.len()).map_err(|_| ChannelError::TooLarge)?;
    stream.write_all(&length.to_be_bytes()).await?;
    stream.write_all(frame).await?;
    Ok(())
}

async fn read_frame<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>, ChannelError> {
    let mut length = [0u8; 2];
    stream.read_exact(&mut length).await?;
    let mut frame = vec![0u8; usize::from(u16::from_be_bytes(length))];
    stream.read_exact(&mut frame).await?;
    Ok(frame)
}

impl<S: AsyncRead + AsyncWrite + Unpin + Send> Channel<S> {
    /// Open a connection for `kind` under `key` to the hub `hub_id`.
    pub async fn connect(mut stream: S, kind: Kind, key: &Key, hub_id: Uuid) -> Result<Self, ChannelError> {
        let opening = Preamble::new(kind, key)?.to_bytes();
        stream.write_all(&opening).await?;
        let mut state = handshake(key, &opening, hub_id, true)?;
        let mut buffer = Zeroizing::new(vec![0u8; MAX_FRAME]);
        let written = state.write_message(&[], &mut buffer).map_err(|_| ChannelError::Handshake)?;
        write_frame(&mut stream, &buffer[..written]).await?;
        stream.flush().await?;
        let reply = read_frame(&mut stream).await?;
        state.read_message(&reply, &mut buffer).map_err(|_| ChannelError::Handshake)?;
        Self::finish(stream, state)
    }

    /// Take a connection to hub `hub_id`: `choose` gives the key a preamble's hint was made with,
    /// and who uses it, or nothing (the connection ends, unanswered).
    pub async fn accept<T>(mut stream: S, hub_id: Uuid, choose: impl FnOnce(&Preamble) -> Option<(Key, T)>) -> Result<(Self, Kind, T), ChannelError> {
        let mut opening = [0u8; PREAMBLE_LEN];
        stream.read_exact(&mut opening).await?;
        let preamble = Preamble::parse(&opening).ok_or(ChannelError::NotLockra)?;
        let (key, who) = choose(&preamble).ok_or(ChannelError::UnknownKey)?;
        let mut state = handshake(&key, &opening, hub_id, false)?;
        let first = read_frame(&mut stream).await?;
        let mut buffer = Zeroizing::new(vec![0u8; MAX_FRAME]);
        state.read_message(&first, &mut buffer).map_err(|_| ChannelError::Handshake)?;
        let written = state.write_message(&[], &mut buffer).map_err(|_| ChannelError::Handshake)?;
        write_frame(&mut stream, &buffer[..written]).await?;
        stream.flush().await?;
        Ok((Self::finish(stream, state)?, preamble.kind, who))
    }

    fn finish(stream: S, state: HandshakeState) -> Result<Self, ChannelError> {
        let mut hash = [0u8; 32];
        hash.copy_from_slice(state.get_handshake_hash().get(..32).ok_or(ChannelError::Handshake)?);
        Ok(Self { stream, noise: state.into_transport_mode().map_err(|_| ChannelError::Handshake)?, hash })
    }

    /// Six digits both ends of this connection show alike, and the ends of no other connection:
    /// the user compares them before a pairing goes on.
    pub fn check_code(&self) -> String {
        let digest = Sha256::new().chain_update(b"lockra-lan-check").chain_update(self.hash).finalize();
        let mut first = [0u8; 4];
        first.copy_from_slice(&digest[..4]);
        format!("{:06}", u32::from_be_bytes(first) % 1_000_000)
    }

    /// Send a message.
    pub async fn send(&mut self, header: &[u8], body: &[u8]) -> Result<(), ChannelError> {
        let total = 4 + header.len() + body.len();
        if total > MAX_MESSAGE {
            return Err(ChannelError::TooLarge);
        }
        let mut plain = Zeroizing::new(Vec::with_capacity(4 + total));
        plain.extend_from_slice(&u32::try_from(total).map_err(|_| ChannelError::TooLarge)?.to_be_bytes());
        plain.extend_from_slice(&u32::try_from(header.len()).map_err(|_| ChannelError::TooLarge)?.to_be_bytes());
        plain.extend_from_slice(header);
        plain.extend_from_slice(body);
        let mut frame = vec![0u8; MAX_FRAME];
        for chunk in plain.chunks(MAX_CHUNK) {
            let written = self.noise.write_message(chunk, &mut frame).map_err(|_| ChannelError::Malformed)?;
            write_frame(&mut self.stream, &frame[..written]).await?;
        }
        self.stream.flush().await?;
        Ok(())
    }

    /// Receive a message: its header and its body.
    pub async fn recv(&mut self) -> Result<(Zeroizing<Vec<u8>>, Zeroizing<Vec<u8>>), ChannelError> {
        let mut plain = Zeroizing::new(Vec::new());
        let mut chunk = Zeroizing::new(vec![0u8; MAX_FRAME]);
        let mut length: Option<usize> = None;
        loop {
            let frame = read_frame(&mut self.stream).await?;
            let read = self.noise.read_message(&frame, &mut chunk).map_err(|_| ChannelError::Malformed)?;
            plain.extend_from_slice(&chunk[..read]);
            if length.is_none() && plain.len() >= 4 {
                let total = usize::try_from(u32::from_be_bytes([plain[0], plain[1], plain[2], plain[3]])).map_err(|_| ChannelError::TooLarge)?;
                if !(4..=MAX_MESSAGE).contains(&total) {
                    return Err(ChannelError::TooLarge);
                }
                length = Some(4 + total);
            }
            match length {
                Some(length) if plain.len() == length => break,
                Some(length) if plain.len() > length => return Err(ChannelError::Malformed),
                _ => {}
            }
        }
        let header_length = usize::try_from(u32::from_be_bytes([plain[4], plain[5], plain[6], plain[7]])).map_err(|_| ChannelError::Malformed)?;
        let body_start = 8usize.checked_add(header_length).filter(|end| *end <= plain.len()).ok_or(ChannelError::Malformed)?;
        Ok((Zeroizing::new(plain[8..body_start].to_vec()), Zeroizing::new(plain[body_start..].to_vec())))
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::duplex;

    use super::*;

    const HUB: Uuid = Uuid::from_u128(7);

    fn key(byte: u8) -> Key {
        Zeroizing::new([byte; 32])
    }

    async fn pair_up(
        client_key: Key,
        hub_keys: Vec<Key>,
    ) -> (Result<Channel<tokio::io::DuplexStream>, ChannelError>, Result<(Channel<tokio::io::DuplexStream>, Kind, usize), ChannelError>) {
        let (client, hub) = duplex(1 << 20);
        let connect = Channel::connect(client, Kind::Session, &client_key, HUB);
        let accept = Channel::accept(hub, HUB, |preamble| hub_keys.iter().position(|k| preamble.made_with(k)).map(|i| (hub_keys[i].clone(), i)));
        tokio::join!(connect, accept)
    }

    #[tokio::test]
    async fn the_two_ends_agree_on_a_key_and_carry_messages_of_any_size_up_to_the_limit() {
        let (client, hub) = pair_up(key(2), vec![key(1), key(2), key(3)]).await;
        let (mut client, mut hub) = (client.unwrap(), hub.unwrap());
        assert_eq!((hub.1, hub.2), (Kind::Session, 1), "the hub found the key by its hint");
        assert_eq!(client.check_code(), hub.0.check_code());
        assert_eq!(client.check_code().len(), 6);
        // Small, empty and larger than one Noise message.
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        for (header, body) in [(b"{}".to_vec(), Vec::new()), (Vec::new(), b"x".to_vec()), (b"{\"op\":\"put\"}".to_vec(), big.clone())] {
            client.send(&header, &body).await.unwrap();
            let (got_header, got_body) = hub.0.recv().await.unwrap();
            assert_eq!((got_header.as_slice(), got_body.as_slice()), (header.as_slice(), body.as_slice()));
        }
        hub.0.send(b"{\"answer\":\"ok\"}", &big).await.unwrap();
        assert_eq!(client.recv().await.unwrap().1.as_slice(), big.as_slice());
        // Larger than any message of the protocol: not sent.
        assert!(matches!(client.send(b"", &vec![0u8; MAX_MESSAGE]).await, Err(ChannelError::TooLarge)));
    }

    #[tokio::test]
    async fn a_key_the_hub_does_not_hold_ends_the_connection_unanswered() {
        let (client, hub) = pair_up(key(9), vec![key(1)]).await;
        assert!(matches!(hub, Err(ChannelError::UnknownKey)));
        assert!(matches!(client, Err(ChannelError::Io(_))), "no reply to read");
    }

    #[tokio::test]
    async fn another_hub_or_a_changed_preamble_fails_the_handshake() {
        // The right key at another hub: the prologue differs.
        let (client, hub) = duplex(1 << 16);
        let k = key(4);
        let connect = Channel::connect(client, Kind::Session, &k, Uuid::from_u128(8));
        let accept = Channel::accept(hub, HUB, |_| Some((key(4), ())));
        let (client, hub) = tokio::join!(connect, accept);
        assert!(matches!(hub, Err(ChannelError::Handshake)));
        assert!(client.is_err());
        // The kind changed underway: the hub reads a pairing, the device sent a session.
        let (mut client, mut hub) = duplex(1 << 16);
        let opening = Preamble::new(Kind::Session, &k).unwrap();
        let mut changed = opening.to_bytes();
        changed[5] = 1;
        client.write_all(&changed).await.unwrap();
        let mut state = handshake(&k, &opening.to_bytes(), HUB, true).unwrap();
        let mut buffer = vec![0u8; MAX_FRAME];
        let written = state.write_message(&[], &mut buffer).unwrap();
        write_frame(&mut client, &buffer[..written]).await.unwrap();
        let accepted = Channel::accept(&mut hub, HUB, |p| p.made_with(&k).then(|| (key(4), ()))).await;
        assert!(matches!(accepted, Err(ChannelError::Handshake)));
        // Not a preamble at all.
        let (mut client, hub) = duplex(1 << 16);
        client.write_all(&[0u8; PREAMBLE_LEN]).await.unwrap();
        assert!(matches!(Channel::accept(hub, HUB, |_| Some((key(4), ()))).await, Err(ChannelError::NotLockra)));
    }

    #[tokio::test]
    async fn a_message_claiming_more_than_the_limit_or_a_tampered_frame_ends_the_connection() {
        let (client, hub) = pair_up(key(5), vec![key(5)]).await;
        let (mut client, mut hub) = (client.unwrap(), hub.unwrap().0);
        // A frame that says the message is 4 GiB.
        let mut frame = vec![0u8; MAX_FRAME];
        let written = client.noise.write_message(&u32::MAX.to_be_bytes(), &mut frame).unwrap();
        write_frame(&mut client.stream, &frame[..written]).await.unwrap();
        assert!(matches!(hub.recv().await, Err(ChannelError::TooLarge)));
        // A frame changed on the way.
        let (client, hub) = pair_up(key(5), vec![key(5)]).await;
        let (mut client, mut hub) = (client.unwrap(), hub.unwrap().0);
        let written = client.noise.write_message(&[0, 0, 0, 4, 0, 0, 0, 0], &mut frame).unwrap();
        frame[3] ^= 1;
        write_frame(&mut client.stream, &frame[..written]).await.unwrap();
        assert!(matches!(hub.recv().await, Err(ChannelError::Malformed)));
        // A header longer than its message.
        let (client, hub) = pair_up(key(5), vec![key(5)]).await;
        let (mut client, mut hub) = (client.unwrap(), hub.unwrap().0);
        let written = client.noise.write_message(&[0, 0, 0, 4, 0, 0, 0, 9], &mut frame).unwrap();
        write_frame(&mut client.stream, &frame[..written]).await.unwrap();
        assert!(matches!(hub.recv().await, Err(ChannelError::Malformed)));
    }
}
