//! KeePass databases, KDBX 4 (docs/formats.md §11), read by Lockra itself within limits: the key
//! derivation's parameters are checked against [`MAX_ARGON2_MEMORY`] and the others before any
//! work (a file from someone else must not make the device spend minutes or gigabytes), and the
//! payload expands to at most [`MAX_EXPANDED_BYTES`]. The ciphers are KeePass's three (AES-256,
//! Twofish, ChaCha20), the key derivations AES-KDF, Argon2d and Argon2id; KDBX 3.1 and earlier
//! are not read.
//!
//! The composite key is SHA-256 of the components' keys: SHA-256 of the password (left out when
//! the password is empty and a key file is given), then the key file's ([`KeyFile`]).

mod keyfile;
mod xml;

use std::io::Read;

use aes::Aes256;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockModeDecrypt, KeyInit, KeyIvInit, StreamCipher};
use flate2::read::GzDecoder;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};
use zeroize::Zeroizing;

pub use keyfile::{KeyFile, KeyFileError};

use crate::login::{Found, MAX_EXPANDED_BYTES};

/// A KeePass 2 database's first bytes: the KDBX signature and KeePass 2's own.
pub const MAGIC: [u8; 8] = [0x03, 0xd9, 0xa2, 0x9a, 0x67, 0xfb, 0x4b, 0xb5];
/// The most memory Argon2 may take (1 GiB).
pub const MAX_ARGON2_MEMORY: u64 = 1 << 30;
/// The most passes Argon2 may make.
pub const MAX_ARGON2_ITERATIONS: u64 = 4096;
/// The most lanes Argon2 may run (a thread each).
pub const MAX_ARGON2_PARALLELISM: u32 = 64;
/// The most memory times passes (64 GiB): a minute's work at worst on one core.
pub const MAX_ARGON2_WORK: u64 = 1 << 36;
/// The most rounds the AES-KDF may make (some 2^30: seconds with AES instructions).
pub const MAX_AES_ROUNDS: u64 = 1 << 30;

const CIPHER_AES256: [u8; 16] = *b"\x31\xc1\xf2\xe6\xbf\x71\x43\x50\xbe\x58\x05\x21\x6a\xfc\x5a\xff";
const CIPHER_TWOFISH: [u8; 16] = *b"\xad\x68\xf2\x9f\x57\x6f\x4b\xb9\xa3\x6a\xd4\x7a\xf9\x65\x34\x6c";
const CIPHER_CHACHA20: [u8; 16] = *b"\xd6\x03\x8a\x2b\x8b\x6f\x4c\xb5\xa5\x24\x33\x9a\x31\xdb\xb5\x9a";
const KDF_AES: [u8; 16] = *b"\xc9\xd9\xf3\x9a\x62\x8a\x44\x60\xbf\x74\x0d\x08\xc1\x8a\x4f\xea";
const KDF_AES_KDBX4: [u8; 16] = *b"\x7c\x02\xbb\x82\x79\xa7\x4a\xc0\x92\x7d\x11\x4a\x00\x64\x82\x38";
const KDF_ARGON2D: [u8; 16] = *b"\xef\x63\x6d\xdf\x8c\x29\x44\x4b\x91\xf7\xa9\xa4\x03\xe3\x0a\x0c";
const KDF_ARGON2ID: [u8; 16] = *b"\x9e\x29\x8b\x19\x56\xdb\x47\x73\xb2\x3d\xfc\x3e\xc6\xf0\xa1\xe6";
/// The Salsa20 nonce of KeePass's inner stream.
const SALSA20_NONCE: [u8; 8] = [0xe8, 0x30, 0x09, 0x4b, 0x97, 0x20, 0x5d, 0x2a];

/// Why a KeePass database did not open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KdbxError {
    /// Not a KDBX 4 database Lockra reads: an older version, an unknown cipher, compression or
    /// key derivation.
    #[error("not a KDBX 4 database Lockra reads")]
    Unsupported,
    /// Its key derivation asks for more than Lockra spends on opening a file.
    #[error("the database's key derivation is past Lockra's limits")]
    TooCostly,
    /// The password, with the key file if one was given, does not open it.
    #[error("the key does not open the database")]
    WrongKey,
    /// It opened, and what it holds is damaged.
    #[error("the database is damaged")]
    Damaged,
    /// Its payload expands past [`MAX_EXPANDED_BYTES`].
    #[error("the database is too large")]
    TooLarge,
}

/// The cipher of the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cipher {
    Aes256,
    Twofish,
    ChaCha20,
}

/// The key derivation, its parameters checked.
enum Kdf {
    Aes {
        seed: Zeroizing<Vec<u8>>,
        rounds: u64,
    },
    Argon2 {
        variant: argon2::Variant,
        version: argon2::Version,
        memory_kib: u32,
        iterations: u32,
        lanes: u32,
        salt: Vec<u8>,
        secret: Zeroizing<Vec<u8>>,
        ad: Vec<u8>,
    },
}

/// The outer header.
struct Header<'a> {
    cipher: Cipher,
    gzip: bool,
    master_seed: &'a [u8],
    iv: &'a [u8],
    kdf: Kdf,
    /// The header's bytes, through its end field.
    bytes: &'a [u8],
}

/// Check what opening `bytes` would take, without a key: a KDBX 4 database whose ciphers Lockra
/// knows and whose key derivation is within its limits.
pub fn check(bytes: &[u8]) -> Result<(), KdbxError> {
    header(bytes).map(|_| ())
}

/// Open `bytes` with `password` (and the key of `key_file`, when the database takes one) and
/// list its entries as logins, the recycle bin's left out. The key derivation runs here: call it
/// off the async runtime.
pub fn read(bytes: &[u8], password: &str, key_file: Option<&KeyFile>) -> Result<Vec<Found>, KdbxError> {
    read_within(bytes, password, key_file, MAX_EXPANDED_BYTES)
}

/// [`read`], the payload expanding to at most `max_expanded` bytes.
pub(crate) fn read_within(bytes: &[u8], password: &str, key_file: Option<&KeyFile>, max_expanded: usize) -> Result<Vec<Found>, KdbxError> {
    let header = header(bytes)?;
    let rest = &bytes[header.bytes.len()..];
    let (sha, rest) = rest.split_at_checked(32).ok_or(KdbxError::Damaged)?;
    let (mac, blocks) = rest.split_at_checked(32).ok_or(KdbxError::Damaged)?;
    if Sha256::digest(header.bytes).as_slice() != sha {
        return Err(KdbxError::Damaged);
    }
    let transformed = transform(&composite(password, key_file), &header.kdf)?;
    let master_key = Zeroizing::new(<[u8; 32]>::from(Sha256::new().chain_update(header.master_seed).chain_update(*transformed).finalize()));
    let hmac_base = Zeroizing::new(<[u8; 64]>::from(Sha512::new().chain_update(header.master_seed).chain_update(*transformed).chain_update([1u8]).finalize()));
    if !verify(&block_key(u64::MAX, &hmac_base), &[header.bytes], mac) {
        return Err(KdbxError::WrongKey);
    }
    let mut payload = read_blocks(blocks, &hmac_base)?;
    decrypt(header.cipher, &master_key, header.iv, &mut payload)?;
    let payload = if header.gzip { gunzip(&payload, max_expanded)? } else { payload };
    if payload.len() > max_expanded {
        return Err(KdbxError::TooLarge);
    }
    let (mut stream, xml) = inner_header(&payload)?;
    xml::logins(xml, &mut stream)
}

fn header(bytes: &[u8]) -> Result<Header<'_>, KdbxError> {
    if bytes.len() < 12 || bytes[..8] != MAGIC {
        return Err(KdbxError::Unsupported);
    }
    let major = u16::from_le_bytes([bytes[10], bytes[11]]);
    if major != 4 {
        return Err(KdbxError::Unsupported);
    }
    let mut pos = 12;
    let (mut cipher, mut gzip, mut master_seed, mut iv, mut kdf) = (None, None, None, None, None);
    loop {
        let id = *bytes.get(pos).ok_or(KdbxError::Damaged)?;
        let length = bytes.get(pos + 1..pos + 5).ok_or(KdbxError::Damaged)?;
        let length = usize::try_from(u32::from_le_bytes([length[0], length[1], length[2], length[3]])).map_err(|_| KdbxError::Damaged)?;
        let data = bytes.get(pos + 5..pos.saturating_add(5).saturating_add(length)).ok_or(KdbxError::Damaged)?;
        pos += 5 + length;
        match id {
            0 => break,
            2 => {
                cipher = Some(match <[u8; 16]>::try_from(data).map_err(|_| KdbxError::Damaged)? {
                    CIPHER_AES256 => Cipher::Aes256,
                    CIPHER_TWOFISH => Cipher::Twofish,
                    CIPHER_CHACHA20 => Cipher::ChaCha20,
                    _ => return Err(KdbxError::Unsupported),
                });
            }
            3 => {
                gzip = Some(match <[u8; 4]>::try_from(data).map(u32::from_le_bytes).map_err(|_| KdbxError::Damaged)? {
                    0 => false,
                    1 => true,
                    _ => return Err(KdbxError::Unsupported),
                });
            }
            4 => master_seed = Some(data).filter(|seed| seed.len() == 32),
            7 => iv = Some(data),
            11 => kdf = Some(kdf_of(&Dictionary::parse(data)?)?),
            // A comment, the public custom data, and what later versions add.
            _ => {}
        }
    }
    let cipher = cipher.ok_or(KdbxError::Damaged)?;
    let iv = iv.ok_or(KdbxError::Damaged)?;
    let iv_len = if cipher == Cipher::ChaCha20 { 12 } else { 16 };
    if iv.len() != iv_len {
        return Err(KdbxError::Damaged);
    }
    Ok(Header {
        cipher,
        gzip: gzip.ok_or(KdbxError::Damaged)?,
        master_seed: master_seed.ok_or(KdbxError::Damaged)?,
        iv,
        kdf: kdf.ok_or(KdbxError::Damaged)?,
        bytes: &bytes[..pos],
    })
}

/// KeePass's typed dictionary (the key derivation's parameters).
struct Dictionary<'a>(Vec<(&'a [u8], u8, &'a [u8])>);

impl<'a> Dictionary<'a> {
    fn parse(data: &'a [u8]) -> Result<Self, KdbxError> {
        let version = data.get(..2).map(|v| u16::from_le_bytes([v[0], v[1]])).ok_or(KdbxError::Damaged)?;
        // The high byte is the critical version: a later one this version cannot read.
        if version & 0xff00 > 0x0100 {
            return Err(KdbxError::Unsupported);
        }
        let mut pos = 2;
        let mut entries = Vec::new();
        loop {
            let kind = *data.get(pos).ok_or(KdbxError::Damaged)?;
            pos += 1;
            if kind == 0 {
                return Ok(Self(entries));
            }
            let mut field = || -> Result<&'a [u8], KdbxError> {
                let length = data.get(pos..pos + 4).ok_or(KdbxError::Damaged)?;
                let length = usize::try_from(i32::from_le_bytes([length[0], length[1], length[2], length[3]])).map_err(|_| KdbxError::Damaged)?;
                let value = data.get(pos + 4..pos + 4 + length).ok_or(KdbxError::Damaged)?;
                pos += 4 + length;
                Ok(value)
            };
            let name = field()?;
            let value = field()?;
            entries.push((name, kind, value));
        }
    }

    fn bytes(&self, name: &str) -> Option<&'a [u8]> {
        self.0.iter().find(|(n, kind, _)| *n == name.as_bytes() && *kind == 0x42).map(|(_, _, v)| *v)
    }

    /// An unsigned number of 32 (`0x04`) or 64 bits (`0x05`).
    fn number(&self, name: &str) -> Result<Option<u64>, KdbxError> {
        let Some((_, kind, value)) = self.0.iter().find(|(n, _, _)| *n == name.as_bytes()) else { return Ok(None) };
        match (*kind, value.len()) {
            (0x04, 4) => Ok(Some(u64::from(u32::from_le_bytes([value[0], value[1], value[2], value[3]])))),
            (0x05, 8) => Ok(Some(u64::from_le_bytes([value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7]]))),
            _ => Err(KdbxError::Damaged),
        }
    }
}

fn kdf_of(dictionary: &Dictionary<'_>) -> Result<Kdf, KdbxError> {
    let id = dictionary.bytes("$UUID").and_then(|id| <[u8; 16]>::try_from(id).ok()).ok_or(KdbxError::Damaged)?;
    match id {
        KDF_AES | KDF_AES_KDBX4 => {
            let rounds = dictionary.number("R")?.ok_or(KdbxError::Damaged)?;
            let seed = dictionary.bytes("S").filter(|s| s.len() == 32).ok_or(KdbxError::Damaged)?;
            if rounds > MAX_AES_ROUNDS {
                return Err(KdbxError::TooCostly);
            }
            Ok(Kdf::Aes { seed: Zeroizing::new(seed.to_vec()), rounds })
        }
        KDF_ARGON2D | KDF_ARGON2ID => {
            let variant = if id == KDF_ARGON2D { argon2::Variant::Argon2d } else { argon2::Variant::Argon2id };
            let version = match dictionary.number("V")?.ok_or(KdbxError::Damaged)? {
                0x10 => argon2::Version::Version10,
                0x13 => argon2::Version::Version13,
                _ => return Err(KdbxError::Unsupported),
            };
            let memory = dictionary.number("M")?.ok_or(KdbxError::Damaged)?;
            let iterations = dictionary.number("I")?.ok_or(KdbxError::Damaged)?;
            let lanes = dictionary.number("P")?.ok_or(KdbxError::Damaged)?;
            let salt = dictionary.bytes("S").filter(|s| s.len() >= 8).ok_or(KdbxError::Damaged)?;
            if memory > MAX_ARGON2_MEMORY || iterations > MAX_ARGON2_ITERATIONS || lanes > u64::from(MAX_ARGON2_PARALLELISM) {
                return Err(KdbxError::TooCostly);
            }
            if memory.saturating_mul(iterations) > MAX_ARGON2_WORK {
                return Err(KdbxError::TooCostly);
            }
            // Argon2's own bounds: a pass at least, a lane at least, eight blocks of a KiB a lane.
            let memory_kib = u32::try_from(memory / 1024).map_err(|_| KdbxError::TooCostly)?;
            let (iterations, lanes) = (u32::try_from(iterations).map_err(|_| KdbxError::TooCostly)?, u32::try_from(lanes).map_err(|_| KdbxError::TooCostly)?);
            if iterations == 0 || lanes == 0 || memory_kib < 8 * lanes {
                return Err(KdbxError::Damaged);
            }
            let secret = Zeroizing::new(dictionary.bytes("K").unwrap_or_default().to_vec());
            let ad = dictionary.bytes("A").unwrap_or_default().to_vec();
            Ok(Kdf::Argon2 { variant, version, memory_kib, iterations, lanes, salt: salt.to_vec(), secret, ad })
        }
        _ => Err(KdbxError::Unsupported),
    }
}

/// SHA-256 of the key's components.
fn composite(password: &str, key_file: Option<&KeyFile>) -> Zeroizing<[u8; 32]> {
    let mut hash = Sha256::new();
    // An empty password with a key file is no password at all; alone, it is the empty one.
    if !password.is_empty() || key_file.is_none() {
        hash.update(Sha256::digest(password.as_bytes()));
    }
    if let Some(key_file) = key_file {
        hash.update(key_file.key());
    }
    Zeroizing::new(hash.finalize().into())
}

/// The composite key through the key derivation.
fn transform(composite: &[u8; 32], kdf: &Kdf) -> Result<Zeroizing<[u8; 32]>, KdbxError> {
    match kdf {
        Kdf::Aes { seed, rounds } => {
            let cipher = Aes256::new_from_slice(seed).map_err(|_| KdbxError::Damaged)?;
            let mut blocks = Zeroizing::new(*composite);
            let (first, second) = blocks.split_at_mut(16);
            let (mut first, mut second) =
                (aes::Block::try_from(&*first).map_err(|_| KdbxError::Damaged)?, aes::Block::try_from(&*second).map_err(|_| KdbxError::Damaged)?);
            for _ in 0..*rounds {
                aes::cipher::BlockCipherEncrypt::encrypt_block(&cipher, &mut first);
                aes::cipher::BlockCipherEncrypt::encrypt_block(&cipher, &mut second);
            }
            let digest = Sha256::new().chain_update(first).chain_update(second).finalize();
            Ok(Zeroizing::new(digest.into()))
        }
        Kdf::Argon2 { variant, version, memory_kib, iterations, lanes, salt, secret, ad } => {
            let config = argon2::Config {
                ad,
                hash_length: 32,
                lanes: *lanes,
                mem_cost: *memory_kib,
                secret,
                thread_mode: argon2::ThreadMode::Parallel,
                time_cost: *iterations,
                variant: *variant,
                version: *version,
            };
            let hash = Zeroizing::new(argon2::hash_raw(composite, salt, &config).map_err(|_| KdbxError::Damaged)?);
            <[u8; 32]>::try_from(hash.as_slice()).map(Zeroizing::new).map_err(|_| KdbxError::Damaged)
        }
    }
}

/// The HMAC key of block `index`.
fn block_key(index: u64, hmac_base: &[u8; 64]) -> Zeroizing<[u8; 64]> {
    Zeroizing::new(Sha512::new().chain_update(index.to_le_bytes()).chain_update(hmac_base).finalize().into())
}

fn verify(key: &[u8; 64], parts: &[&[u8]], expected: &[u8]) -> bool {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else { return false };
    for part in parts {
        mac.update(part);
    }
    mac.verify_slice(expected).is_ok()
}

/// The payload from its HMAC-checked blocks, up to the empty last one.
fn read_blocks(mut blocks: &[u8], hmac_base: &[u8; 64]) -> Result<Zeroizing<Vec<u8>>, KdbxError> {
    let mut payload = Zeroizing::new(Vec::with_capacity(blocks.len()));
    for index in 0u64.. {
        let (mac, rest) = blocks.split_at_checked(32).ok_or(KdbxError::Damaged)?;
        let (size, rest) = rest.split_at_checked(4).ok_or(KdbxError::Damaged)?;
        let length = usize::try_from(u32::from_le_bytes([size[0], size[1], size[2], size[3]])).map_err(|_| KdbxError::Damaged)?;
        let (data, rest) = rest.split_at_checked(length).ok_or(KdbxError::Damaged)?;
        if !verify(&block_key(index, hmac_base), &[&index.to_le_bytes(), size, data], mac) {
            return Err(KdbxError::Damaged);
        }
        if length == 0 {
            break;
        }
        payload.extend_from_slice(data);
        blocks = rest;
    }
    Ok(payload)
}

fn decrypt(cipher: Cipher, key: &[u8; 32], iv: &[u8], payload: &mut Zeroizing<Vec<u8>>) -> Result<(), KdbxError> {
    let length = match cipher {
        Cipher::Aes256 => cbc::Decryptor::<Aes256>::new_from_slices(key, iv)
            .map_err(|_| KdbxError::Damaged)?
            .decrypt_padded::<Pkcs7>(payload.as_mut_slice())
            .map_err(|_| KdbxError::Damaged)?
            .len(),
        Cipher::Twofish => cbc::Decryptor::<twofish::Twofish>::new_from_slices(key, iv)
            .map_err(|_| KdbxError::Damaged)?
            .decrypt_padded::<Pkcs7>(payload.as_mut_slice())
            .map_err(|_| KdbxError::Damaged)?
            .len(),
        Cipher::ChaCha20 => {
            let mut stream = chacha20::ChaCha20::new_from_slices(key, iv).map_err(|_| KdbxError::Damaged)?;
            stream.try_apply_keystream(payload.as_mut_slice()).map_err(|_| KdbxError::Damaged)?;
            payload.len()
        }
    };
    payload.truncate(length);
    Ok(())
}

/// The payload's gzip, expanded to at most `max_expanded` bytes.
fn gunzip(compressed: &[u8], max_expanded: usize) -> Result<Zeroizing<Vec<u8>>, KdbxError> {
    let mut out = Zeroizing::new(Vec::new());
    let limit = u64::try_from(max_expanded).unwrap_or(u64::MAX).saturating_add(1);
    GzDecoder::new(compressed).take(limit).read_to_end(&mut out).map_err(|_| KdbxError::Damaged)?;
    if out.len() > max_expanded {
        return Err(KdbxError::TooLarge);
    }
    Ok(out)
}

/// The stream that masks the protected values.
pub(crate) enum InnerStream {
    /// Values kept as they are (only Base64).
    Plain,
    Salsa20(Box<salsa20::Salsa20>),
    ChaCha20(Box<chacha20::ChaCha20>),
}

impl InnerStream {
    fn apply(&mut self, bytes: &mut [u8]) -> Result<(), KdbxError> {
        match self {
            Self::Plain => Ok(()),
            Self::Salsa20(stream) => stream.try_apply_keystream(bytes).map_err(|_| KdbxError::Damaged),
            Self::ChaCha20(stream) => stream.try_apply_keystream(bytes).map_err(|_| KdbxError::Damaged),
        }
    }
}

/// The inner header's stream, and the XML after it (the attachments are skipped).
fn inner_header(payload: &[u8]) -> Result<(InnerStream, &[u8]), KdbxError> {
    let mut pos = 0;
    let (mut id, mut key) = (None, None);
    loop {
        let field = *payload.get(pos).ok_or(KdbxError::Damaged)?;
        let length = payload.get(pos + 1..pos + 5).ok_or(KdbxError::Damaged)?;
        let length = usize::try_from(u32::from_le_bytes([length[0], length[1], length[2], length[3]])).map_err(|_| KdbxError::Damaged)?;
        let data = payload.get(pos + 5..pos.saturating_add(5).saturating_add(length)).ok_or(KdbxError::Damaged)?;
        pos += 5 + length;
        match field {
            0 => break,
            1 => id = Some(<[u8; 4]>::try_from(data).map(u32::from_le_bytes).map_err(|_| KdbxError::Damaged)?),
            2 => key = Some(data),
            _ => {}
        }
    }
    let key = key.ok_or(KdbxError::Damaged)?;
    let stream = match id.ok_or(KdbxError::Damaged)? {
        0 => InnerStream::Plain,
        2 => {
            let key: [u8; 32] = Sha256::digest(key).into();
            InnerStream::Salsa20(Box::new(salsa20::Salsa20::new(&key.into(), &SALSA20_NONCE.into())))
        }
        3 => {
            let hash = Zeroizing::new(<[u8; 64]>::from(Sha512::digest(key)));
            InnerStream::ChaCha20(Box::new(chacha20::ChaCha20::new_from_slices(&hash[..32], &hash[32..44]).map_err(|_| KdbxError::Damaged)?))
        }
        _ => return Err(KdbxError::Unsupported),
    };
    Ok((stream, &payload[pos..]))
}

#[cfg(test)]
mod tests;
