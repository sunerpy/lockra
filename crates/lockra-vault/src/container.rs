//! The file format (docs/formats.md): `magic (8) | header length (u32 LE) | header (JSON) | payload`.
//!
//! The payload is XChaCha20-Poly1305 under a random data key (DEK), and its associated data is
//! every byte before it. So no header field can change without the payload being encrypted again:
//! every write is a whole-file rewrite with a fresh payload nonce. Each slot wraps the DEK under its
//! own key, bound to the vault id and the slot kind.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use data_encoding::BASE64;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::b64;
use crate::kdf::{KdfCost, KdfParams};

/// First eight bytes of a vault file.
pub const VAULT_MAGIC: &[u8; 8] = b"LKRAVLT1";
/// First eight bytes of a backup file.
pub const BACKUP_MAGIC: &[u8; 8] = b"LKRABAK1";
/// The longest header a reader accepts.
pub const MAX_HEADER_LEN: usize = 64 * 1024;

const FORMAT: u32 = 1;
const PREFIX_LEN: usize = 12;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const SLOT_AAD: &[u8] = b"lockra/slot/v1";

/// Everything that can go wrong opening or writing a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VaultError {
    /// The magic is not Lockra's.
    #[error("not a Lockra file")]
    NotLockra,
    /// A header `format` this build does not know.
    #[error("format version {0} is newer than this build reads")]
    UnsupportedVersion(u32),
    /// The password slot does not open with this password.
    #[error("wrong password")]
    WrongPassword,
    /// The device slot does not open with this key (the keychain entry is stale).
    #[error("the device key does not open this vault")]
    WrongDeviceKey,
    /// The file has no device slot.
    #[error("the vault has no device slot")]
    NoDeviceSlot,
    /// Truncated, tampered with, or written by something that is not Lockra.
    #[error("the file is damaged")]
    Corrupted,
    /// The operating system's random source failed.
    #[error("no randomness available")]
    Random,
    /// Argon2 refused the inputs (a password longer than 4 GiB).
    #[error("key derivation failed")]
    Kdf,
}

/// Vault or backup: decided by the magic, repeated in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    /// The live vault.
    Vault,
    /// A backup: one password slot, never a device slot.
    Backup,
}

impl FileKind {
    fn magic(self) -> &'static [u8; 8] {
        match self {
            Self::Vault => VAULT_MAGIC,
            Self::Backup => BACKUP_MAGIC,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SlotKind {
    Password,
    Device,
}

impl SlotKind {
    fn tag(self) -> u8 {
        match self {
            Self::Password => 1,
            Self::Device => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Slot {
    kind: SlotKind,
    #[serde(with = "b64::array")]
    nonce: [u8; NONCE_LEN],
    #[serde(with = "b64::vec")]
    wrapped_dek: Vec<u8>,
    /// A device slot's check before its key is used. In the header, so the payload's
    /// authentication covers it: a file without it no longer opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    check: Option<DeviceCheck>,
}

/// What the device must confirm before the keychain's key opens the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCheck {
    /// The user, by Touch ID or Windows Hello.
    Biometric,
    /// A check a newer Lockra wrote, kept as it is: this version cannot make it, so the master
    /// password opens the vault instead.
    Other(String),
}

impl Serialize for DeviceCheck {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Biometric => "biometric",
            Self::Other(name) => name,
        })
    }
}

impl<'de> Deserialize<'de> for DeviceCheck {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(if name == "biometric" { Self::Biometric } else { Self::Other(name) })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Header {
    format: u32,
    kind: FileKind,
    vault_id: Uuid,
    created_at_ms: u64,
    kdf: KdfParams,
    slots: Vec<Slot>,
    #[serde(with = "b64::array")]
    payload_nonce: [u8; NONCE_LEN],
}

impl Header {
    fn slot(&self, kind: SlotKind) -> Option<&Slot> {
        self.slots.iter().find(|s| s.kind == kind)
    }
}

#[derive(Deserialize)]
struct FormatProbe {
    format: u32,
}

/// What a file says about itself before any key is involved (the restore screen shows it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderInfo {
    /// Vault or backup.
    pub kind: FileKind,
    /// The vault the file belongs to.
    pub vault_id: Uuid,
    /// When the vault was created, Unix milliseconds.
    pub created_at_ms: u64,
    /// Whether it opens with a device key.
    pub has_device_slot: bool,
    /// What its device slot asks for first.
    pub device_check: Option<DeviceCheck>,
}

/// The random key a device slot is wrapped under; the OS keychain holds it.
pub struct DeviceKey(Zeroizing<[u8; KEY_LEN]>);

impl DeviceKey {
    /// Base64 text, the form the keychain stores.
    pub fn to_text(&self) -> Zeroizing<String> {
        Zeroizing::new(BASE64.encode(self.0.as_ref()))
    }

    /// Read a key back from the keychain's text.
    pub fn from_text(text: &str) -> Result<Self, VaultError> {
        let bytes = Zeroizing::new(BASE64.decode(text.trim().as_bytes()).map_err(|_| VaultError::WrongDeviceKey)?);
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        if bytes.len() != KEY_LEN {
            return Err(VaultError::WrongDeviceKey);
        }
        key.copy_from_slice(&bytes);
        Ok(Self(key))
    }
}

impl fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceKey(<redacted>)")
    }
}

/// What became of the device slot when the master password changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceSlot {
    /// There was none.
    Absent,
    /// Re-wrapped under the new data key; the keychain entry still opens the vault.
    Kept,
    /// The keychain key was unavailable or stale, so the slot was removed.
    Dropped,
}

/// The keys an unlocked vault holds: enough to write the container again. A clone duplicates the
/// data key in memory (zeroized on drop); the core clones to run the KDF outside its lock.
#[derive(Clone)]
pub struct Sealed {
    vault_id: Uuid,
    created_at_ms: u64,
    kdf: KdfParams,
    dek: Zeroizing<[u8; KEY_LEN]>,
    password_slot: Slot,
    device_slot: Option<Slot>,
}

impl fmt::Debug for Sealed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sealed").field("vault_id", &self.vault_id).field("device_slot", &self.device_slot.is_some()).finish_non_exhaustive()
    }
}

/// A container opened with one of its keys.
#[derive(Debug)]
pub struct Opened {
    /// The keys, for writing it again.
    pub sealed: Sealed,
    /// Vault or backup.
    pub kind: FileKind,
    /// The decrypted payload.
    pub payload: Zeroizing<Vec<u8>>,
}

impl Sealed {
    /// A new vault: fresh id, salt and data key, one password slot.
    pub fn create(password: &[u8], cost: KdfCost, now_ms: u64) -> Result<Self, VaultError> {
        Self::create_for(Uuid::new_v4(), password, cost, now_ms)
    }

    /// Keys for a file of an existing vault under a password of its own (a backup with a separate
    /// password): fresh salt and data key, the vault id kept.
    pub fn create_for(vault_id: Uuid, password: &[u8], cost: KdfCost, now_ms: u64) -> Result<Self, VaultError> {
        let kdf = KdfParams::fresh(cost)?;
        let kek = kdf.derive(password)?;
        let dek = random_key()?;
        let password_slot = wrap(&kek, &dek, vault_id, SlotKind::Password)?;
        Ok(Self { vault_id, created_at_ms: now_ms, kdf, dek, password_slot, device_slot: None })
    }

    /// The vault this belongs to.
    pub fn vault_id(&self) -> Uuid {
        self.vault_id
    }

    /// When the vault was created, Unix milliseconds.
    pub fn created_at_ms(&self) -> u64 {
        self.created_at_ms
    }

    /// Whether a vault written now carries a device slot.
    pub fn has_device_slot(&self) -> bool {
        self.device_slot.is_some()
    }

    /// Encrypt `payload` into a complete file. A vault keeps its device slot; a backup reuses the
    /// password slot and the data key as they are and never carries a device slot, so it opens with
    /// the master password of the moment it was written.
    pub fn seal(&self, kind: FileKind, payload: &[u8]) -> Result<Vec<u8>, VaultError> {
        let mut slots = vec![self.password_slot.clone()];
        if kind == FileKind::Vault
            && let Some(device) = &self.device_slot
        {
            slots.push(device.clone());
        }
        let header = Header {
            format: FORMAT,
            kind,
            vault_id: self.vault_id,
            created_at_ms: self.created_at_ms,
            kdf: self.kdf.clone(),
            slots,
            payload_nonce: random_nonce()?,
        };
        let header_bytes = serde_json::to_vec(&header).map_err(|_| VaultError::Corrupted)?;
        let header_len = u32::try_from(header_bytes.len()).map_err(|_| VaultError::Corrupted)?;
        let mut file = Vec::with_capacity(PREFIX_LEN + header_bytes.len() + payload.len() + TAG_LEN);
        file.extend_from_slice(kind.magic());
        file.extend_from_slice(&header_len.to_le_bytes());
        file.extend_from_slice(&header_bytes);
        let ciphertext =
            cipher(&self.dek).encrypt(&XNonce::from(header.payload_nonce), Payload { msg: payload, aad: &file }).map_err(|_| VaultError::Corrupted)?;
        file.extend_from_slice(&ciphertext);
        Ok(file)
    }

    /// Open with the master password (or a backup's own password).
    pub fn open_with_password(bytes: &[u8], password: &[u8]) -> Result<Opened, VaultError> {
        let parsed = parse(bytes)?;
        let slot = parsed.header.slot(SlotKind::Password).ok_or(VaultError::Corrupted)?;
        let kek = parsed.header.kdf.derive(password)?;
        let dek = unwrap(&kek, slot, parsed.header.vault_id).ok_or(VaultError::WrongPassword)?;
        finish(parsed, dek)
    }

    /// Open with the device key from the keychain.
    pub fn open_with_device_key(bytes: &[u8], key: &DeviceKey) -> Result<Opened, VaultError> {
        let parsed = parse(bytes)?;
        let slot = parsed.header.slot(SlotKind::Device).ok_or(VaultError::NoDeviceSlot)?;
        let dek = unwrap(&key.0, slot, parsed.header.vault_id).ok_or(VaultError::WrongDeviceKey)?;
        finish(parsed, dek)
    }

    /// `Ok` when `password` is the master password: an export or a reveal asks for it again.
    pub fn verify_password(&self, password: &[u8]) -> Result<(), VaultError> {
        self.password_kek(password).map(drop)
    }

    /// What the device slot asks for before its key is used.
    pub fn device_check(&self) -> Option<&DeviceCheck> {
        self.device_slot.as_ref().and_then(|slot| slot.check.as_ref())
    }

    /// Make the device slot ask for `check` first (or for nothing); the file written next says so.
    pub fn set_device_check(&mut self, check: Option<DeviceCheck>) -> Result<(), VaultError> {
        let slot = self.device_slot.as_mut().ok_or(VaultError::NoDeviceSlot)?;
        slot.check = check;
        Ok(())
    }

    /// Add a device slot under a fresh random key (returned for the keychain). The data key stays,
    /// so backups written before still open with the master password.
    pub fn enable_device(&mut self) -> Result<DeviceKey, VaultError> {
        let key = DeviceKey(random_key()?);
        self.device_slot = Some(wrap(&key.0, &self.dek, self.vault_id, SlotKind::Device)?);
        Ok(key)
    }

    /// Remove the device slot and rotate the data key: a copy of the old keychain key, together
    /// with an old copy of the file, then opens nothing written from here on.
    pub fn disable_device(&mut self, password: &[u8]) -> Result<(), VaultError> {
        let kek = self.password_kek(password)?;
        let dek = random_key()?;
        self.password_slot = wrap(&kek, &dek, self.vault_id, SlotKind::Password)?;
        self.device_slot = None;
        self.dek = dek;
        Ok(())
    }

    /// A new master password: new salt, new data key. The device slot is re-wrapped when
    /// `device_key` (read back from the keychain) still opens it, and removed otherwise.
    pub fn change_password(&mut self, current: &[u8], new: &[u8], cost: KdfCost, device_key: Option<&DeviceKey>) -> Result<DeviceSlot, VaultError> {
        self.password_kek(current)?;
        let kdf = KdfParams::fresh(cost)?;
        let kek = kdf.derive(new)?;
        let dek = random_key()?;
        let password_slot = wrap(&kek, &dek, self.vault_id, SlotKind::Password)?;
        let (device_slot, outcome) = match (&self.device_slot, device_key) {
            (None, _) => (None, DeviceSlot::Absent),
            (Some(slot), Some(key)) if unwrap(&key.0, slot, self.vault_id).is_some() => {
                let rewrapped = Slot { check: slot.check.clone(), ..wrap(&key.0, &dek, self.vault_id, SlotKind::Device)? };
                (Some(rewrapped), DeviceSlot::Kept)
            }
            (Some(_), _) => (None, DeviceSlot::Dropped),
        };
        self.kdf = kdf;
        self.dek = dek;
        self.password_slot = password_slot;
        self.device_slot = device_slot;
        Ok(outcome)
    }

    fn password_kek(&self, password: &[u8]) -> Result<Zeroizing<[u8; KEY_LEN]>, VaultError> {
        let kek = self.kdf.derive(password)?;
        unwrap(&kek, &self.password_slot, self.vault_id).ok_or(VaultError::WrongPassword)?;
        Ok(kek)
    }
}

/// Kind, vault id, creation time and slots of a file, without opening it.
pub fn read_header(bytes: &[u8]) -> Result<HeaderInfo, VaultError> {
    let parsed = parse(bytes)?;
    Ok(HeaderInfo {
        kind: parsed.header.kind,
        vault_id: parsed.header.vault_id,
        created_at_ms: parsed.header.created_at_ms,
        has_device_slot: parsed.header.slot(SlotKind::Device).is_some(),
        device_check: parsed.header.slot(SlotKind::Device).and_then(|slot| slot.check.clone()),
    })
}

struct Parsed<'a> {
    header: Header,
    associated: &'a [u8],
    ciphertext: &'a [u8],
}

fn parse(bytes: &[u8]) -> Result<Parsed<'_>, VaultError> {
    let magic = bytes.get(..8).ok_or(VaultError::NotLockra)?;
    let kind = if magic == VAULT_MAGIC {
        FileKind::Vault
    } else if magic == BACKUP_MAGIC {
        FileKind::Backup
    } else {
        return Err(VaultError::NotLockra);
    };
    let length: [u8; 4] = bytes.get(8..PREFIX_LEN).and_then(|b| b.try_into().ok()).ok_or(VaultError::Corrupted)?;
    let header_len = usize::try_from(u32::from_le_bytes(length)).map_err(|_| VaultError::Corrupted)?;
    if header_len == 0 || header_len > MAX_HEADER_LEN {
        return Err(VaultError::Corrupted);
    }
    let header_bytes = bytes.get(PREFIX_LEN..PREFIX_LEN + header_len).ok_or(VaultError::Corrupted)?;
    let probe: FormatProbe = serde_json::from_slice(header_bytes).map_err(|_| VaultError::Corrupted)?;
    if probe.format != FORMAT {
        return Err(VaultError::UnsupportedVersion(probe.format));
    }
    let header: Header = serde_json::from_slice(header_bytes).map_err(|_| VaultError::Corrupted)?;
    let ciphertext = &bytes[PREFIX_LEN + header_len..];
    if header.kind != kind || ciphertext.len() < TAG_LEN {
        return Err(VaultError::Corrupted);
    }
    Ok(Parsed { header, associated: &bytes[..PREFIX_LEN + header_len], ciphertext })
}

fn finish(parsed: Parsed<'_>, dek: Zeroizing<[u8; KEY_LEN]>) -> Result<Opened, VaultError> {
    let payload = cipher(&dek)
        .decrypt(&XNonce::from(parsed.header.payload_nonce), Payload { msg: parsed.ciphertext, aad: parsed.associated })
        .map_err(|_| VaultError::Corrupted)?;
    let Header { kind, vault_id, created_at_ms, kdf, slots, .. } = parsed.header;
    let password_slot = slots.iter().find(|s| s.kind == SlotKind::Password).cloned().ok_or(VaultError::Corrupted)?;
    let device_slot = slots.into_iter().find(|s| s.kind == SlotKind::Device);
    Ok(Opened { sealed: Sealed { vault_id, created_at_ms, kdf, dek, password_slot, device_slot }, kind, payload: Zeroizing::new(payload) })
}

fn cipher(key: &[u8; KEY_LEN]) -> XChaCha20Poly1305 {
    // A 32-byte slice is exactly the key size, so this cannot fail.
    #[allow(clippy::expect_used)]
    XChaCha20Poly1305::new_from_slice(key).expect("32-byte key")
}

fn slot_aad(vault_id: Uuid, kind: SlotKind) -> Vec<u8> {
    let mut aad = Vec::with_capacity(SLOT_AAD.len() + 17);
    aad.extend_from_slice(SLOT_AAD);
    aad.extend_from_slice(vault_id.as_bytes());
    aad.push(kind.tag());
    aad
}

fn wrap(kek: &[u8; KEY_LEN], dek: &[u8; KEY_LEN], vault_id: Uuid, kind: SlotKind) -> Result<Slot, VaultError> {
    let nonce = random_nonce()?;
    let wrapped_dek = cipher(kek).encrypt(&XNonce::from(nonce), Payload { msg: dek, aad: &slot_aad(vault_id, kind) }).map_err(|_| VaultError::Corrupted)?;
    Ok(Slot { kind, nonce, wrapped_dek, check: None })
}

fn unwrap(kek: &[u8; KEY_LEN], slot: &Slot, vault_id: Uuid) -> Option<Zeroizing<[u8; KEY_LEN]>> {
    let plain = Zeroizing::new(cipher(kek).decrypt(&XNonce::from(slot.nonce), Payload { msg: &slot.wrapped_dek, aad: &slot_aad(vault_id, slot.kind) }).ok()?);
    let mut dek = Zeroizing::new([0u8; KEY_LEN]);
    if plain.len() != KEY_LEN {
        return None;
    }
    dek.copy_from_slice(&plain);
    Some(dek)
}

fn random_key() -> Result<Zeroizing<[u8; KEY_LEN]>, VaultError> {
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    getrandom::fill(key.as_mut()).map_err(|_| VaultError::Random)?;
    Ok(key)
}

fn random_nonce() -> Result<[u8; NONCE_LEN], VaultError> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|_| VaultError::Random)?;
    Ok(nonce)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COST: KdfCost = KdfCost::FAST_INSECURE;
    const NOW: u64 = 1_790_000_000_000;

    fn vault(password: &[u8]) -> Sealed {
        Sealed::create(password, COST, NOW).unwrap()
    }

    /// The file with its header replaced by `edit(header JSON)`, the length prefix fixed up.
    fn with_header(file: &[u8], edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let len = u32::from_le_bytes(file[8..12].try_into().unwrap()) as usize;
        let mut header: serde_json::Value = serde_json::from_slice(&file[12..12 + len]).unwrap();
        edit(&mut header);
        let bytes = serde_json::to_vec(&header).unwrap();
        let mut out = file[..8].to_vec();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&bytes);
        out.extend_from_slice(&file[12 + len..]);
        out
    }

    fn flip_base64_char(value: &mut serde_json::Value) {
        let text = value.as_str().unwrap().to_owned();
        let mut chars: Vec<char> = text.chars().collect();
        chars[2] = if chars[2] == 'A' { 'B' } else { 'A' };
        *value = serde_json::Value::String(chars.into_iter().collect());
    }

    #[test]
    fn round_trip_with_the_password() {
        let sealed = vault(b"correct horse");
        let file = sealed.seal(FileKind::Vault, b"{\"entries\":[]}").unwrap();
        assert_eq!(&file[..8], VAULT_MAGIC);
        let opened = Sealed::open_with_password(&file, b"correct horse").unwrap();
        assert_eq!(opened.kind, FileKind::Vault);
        assert_eq!(opened.payload.as_slice(), b"{\"entries\":[]}");
        assert_eq!(opened.sealed.vault_id(), sealed.vault_id());
        assert_eq!(opened.sealed.created_at_ms(), NOW);
        assert!(!opened.sealed.has_device_slot());
    }

    #[test]
    fn every_write_uses_a_fresh_nonce() {
        let sealed = vault(b"pw");
        let a = sealed.seal(FileKind::Vault, b"same").unwrap();
        let b = sealed.seal(FileKind::Vault, b"same").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn wrong_password_is_reported_as_such() {
        let file = vault(b"right").seal(FileKind::Vault, b"x").unwrap();
        assert_eq!(Sealed::open_with_password(&file, b"wrong").unwrap_err(), VaultError::WrongPassword);
    }

    #[test]
    fn tampering_is_detected() {
        let file = vault(b"pw").seal(FileKind::Vault, b"payload").unwrap();
        // A header field outside the slots: the payload no longer authenticates.
        let edited = with_header(&file, |h| h["created_at_ms"] = serde_json::json!(NOW + 1));
        assert_eq!(Sealed::open_with_password(&edited, b"pw").unwrap_err(), VaultError::Corrupted);
        // The salt: the password slot no longer opens.
        let edited = with_header(&file, |h| flip_base64_char(&mut h["kdf"]["salt"]));
        assert_eq!(Sealed::open_with_password(&edited, b"pw").unwrap_err(), VaultError::WrongPassword);
        // The payload itself.
        let mut edited = file.clone();
        let last = edited.len() - 1;
        edited[last] ^= 1;
        assert_eq!(Sealed::open_with_password(&edited, b"pw").unwrap_err(), VaultError::Corrupted);
        // The magic says vault, the header says backup.
        let edited = with_header(&file, |h| h["kind"] = serde_json::json!("backup"));
        assert_eq!(Sealed::open_with_password(&edited, b"pw").unwrap_err(), VaultError::Corrupted);
    }

    #[test]
    fn tampering_with_any_slot_is_detected() {
        let mut sealed = vault(b"pw");
        let key = sealed.enable_device().unwrap();
        let file = sealed.seal(FileKind::Vault, b"payload").unwrap();
        for index in 0..2 {
            for field in ["nonce", "wrapped_dek"] {
                let edited = with_header(&file, |h| flip_base64_char(&mut h["slots"][index][field]));
                assert!(Sealed::open_with_password(&edited, b"pw").is_err(), "slot {index} {field}, password");
                assert!(Sealed::open_with_device_key(&edited, &key).is_err(), "slot {index} {field}, device key");
            }
        }
    }

    #[test]
    fn truncated_and_foreign_files_are_refused() {
        let file = vault(b"pw").seal(FileKind::Vault, b"payload").unwrap();
        assert_eq!(Sealed::open_with_password(&file[..7], b"pw").unwrap_err(), VaultError::NotLockra);
        for cut in [8, 11, 12, 40, file.len() - 7 - TAG_LEN - 1, file.len() - 10, file.len() - 1] {
            assert_eq!(Sealed::open_with_password(&file[..cut], b"pw").unwrap_err(), VaultError::Corrupted, "cut at {cut}");
        }
        assert_eq!(Sealed::open_with_password(b"SQLite format 3\0........", b"pw").unwrap_err(), VaultError::NotLockra);
        let mut huge = file.clone();
        huge[8..12].copy_from_slice(&((MAX_HEADER_LEN + 1) as u32).to_le_bytes());
        assert_eq!(read_header(&huge).unwrap_err(), VaultError::Corrupted);
    }

    #[test]
    fn a_newer_format_is_named() {
        let file = vault(b"pw").seal(FileKind::Vault, b"payload").unwrap();
        let edited = with_header(&file, |h| h["format"] = serde_json::json!(2));
        assert_eq!(Sealed::open_with_password(&edited, b"pw").unwrap_err(), VaultError::UnsupportedVersion(2));
        assert_eq!(read_header(&edited).unwrap_err(), VaultError::UnsupportedVersion(2));
    }

    #[test]
    fn device_slot_opens_and_stays_bound() {
        let mut sealed = vault(b"pw");
        let key = sealed.enable_device().unwrap();
        let file = sealed.seal(FileKind::Vault, b"payload").unwrap();
        assert!(read_header(&file).unwrap().has_device_slot);
        assert_eq!(Sealed::open_with_device_key(&file, &key).unwrap().payload.as_slice(), b"payload");
        assert_eq!(Sealed::open_with_password(&file, b"pw").unwrap().payload.as_slice(), b"payload");
        let other = vault(b"pw").enable_device().unwrap();
        assert_eq!(Sealed::open_with_device_key(&file, &other).unwrap_err(), VaultError::WrongDeviceKey);
        let plain = vault(b"pw").seal(FileKind::Vault, b"x").unwrap();
        assert_eq!(Sealed::open_with_device_key(&plain, &key).unwrap_err(), VaultError::NoDeviceSlot);
    }

    #[test]
    fn disabling_the_device_rotates_the_data_key() {
        let mut sealed = vault(b"pw");
        let key = sealed.enable_device().unwrap();
        let before = sealed.seal(FileKind::Vault, b"old").unwrap();
        assert_eq!(sealed.disable_device(b"nope").unwrap_err(), VaultError::WrongPassword);
        assert!(sealed.has_device_slot(), "a failed attempt changes nothing");
        sealed.disable_device(b"pw").unwrap();
        let after = sealed.seal(FileKind::Vault, b"new").unwrap();
        assert_eq!(Sealed::open_with_device_key(&after, &key).unwrap_err(), VaultError::NoDeviceSlot);
        assert_eq!(Sealed::open_with_password(&after, b"pw").unwrap().payload.as_slice(), b"new");
        // The old device key opens the old file, and the data key it yields is not the new one.
        let old = Sealed::open_with_device_key(&before, &key).unwrap();
        assert_ne!(*old.sealed.dek, *sealed.dek);
    }

    #[test]
    fn changing_the_password_rotates_and_keeps_a_live_device_slot() {
        let mut sealed = vault(b"old");
        let key = sealed.enable_device().unwrap();
        let backup_before = sealed.seal(FileKind::Backup, b"v1").unwrap();
        assert_eq!(sealed.change_password(b"wrong", b"new", COST, Some(&key)).unwrap_err(), VaultError::WrongPassword);
        assert_eq!(sealed.change_password(b"old", b"new", COST, Some(&key)).unwrap(), DeviceSlot::Kept);
        let file = sealed.seal(FileKind::Vault, b"v2").unwrap();
        assert_eq!(Sealed::open_with_password(&file, b"old").unwrap_err(), VaultError::WrongPassword);
        assert_eq!(Sealed::open_with_password(&file, b"new").unwrap().payload.as_slice(), b"v2");
        assert_eq!(Sealed::open_with_device_key(&file, &key).unwrap().payload.as_slice(), b"v2");
        // The backup written before still opens with the password of its moment, and only that.
        assert_eq!(Sealed::open_with_password(&backup_before, b"old").unwrap().payload.as_slice(), b"v1");
        assert_eq!(Sealed::open_with_password(&backup_before, b"new").unwrap_err(), VaultError::WrongPassword);
    }

    #[test]
    fn changing_the_password_drops_an_unusable_device_slot() {
        let mut sealed = vault(b"old");
        sealed.enable_device().unwrap();
        assert_eq!(sealed.change_password(b"old", b"new", COST, None).unwrap(), DeviceSlot::Dropped);
        assert!(!sealed.has_device_slot());
        let mut sealed = vault(b"old");
        sealed.enable_device().unwrap();
        let stale = vault(b"x").enable_device().unwrap();
        assert_eq!(sealed.change_password(b"old", b"new", COST, Some(&stale)).unwrap(), DeviceSlot::Dropped);
        let mut sealed = vault(b"old");
        assert_eq!(sealed.change_password(b"old", b"new", COST, None).unwrap(), DeviceSlot::Absent);
    }

    #[test]
    fn a_backup_written_after_a_device_unlock_opens_with_the_password() {
        let mut sealed = vault(b"pw");
        let key = sealed.enable_device().unwrap();
        let file = sealed.seal(FileKind::Vault, b"data").unwrap();
        let unlocked = Sealed::open_with_device_key(&file, &key).unwrap();
        let backup = unlocked.sealed.seal(FileKind::Backup, &unlocked.payload).unwrap();
        assert_eq!(&backup[..8], BACKUP_MAGIC);
        let info = read_header(&backup).unwrap();
        assert_eq!((info.kind, info.vault_id, info.has_device_slot), (FileKind::Backup, sealed.vault_id(), false));
        assert_eq!(Sealed::open_with_device_key(&backup, &key).unwrap_err(), VaultError::NoDeviceSlot);
        let restored = Sealed::open_with_password(&backup, b"pw").unwrap();
        assert_eq!((restored.kind, restored.payload.as_slice()), (FileKind::Backup, b"data".as_slice()));
    }

    #[test]
    fn a_backup_can_have_a_password_of_its_own() {
        let sealed = vault(b"master");
        let own = Sealed::create_for(sealed.vault_id(), b"backup-only", COST, NOW).unwrap();
        let backup = own.seal(FileKind::Backup, b"data").unwrap();
        assert_eq!(Sealed::open_with_password(&backup, b"master").unwrap_err(), VaultError::WrongPassword);
        assert_eq!(Sealed::open_with_password(&backup, b"backup-only").unwrap().sealed.vault_id(), sealed.vault_id());
    }

    #[test]
    fn verify_password_checks_without_changing_anything() {
        let sealed = vault(b"pw");
        assert_eq!(sealed.verify_password(b"pw"), Ok(()));
        assert_eq!(sealed.verify_password(b"pW"), Err(VaultError::WrongPassword));
    }

    #[test]
    fn device_key_text_round_trips_and_rejects_garbage() {
        let key = vault(b"pw").enable_device().unwrap();
        let back = DeviceKey::from_text(&key.to_text()).unwrap();
        assert_eq!(*back.0, *key.0);
        assert_eq!(DeviceKey::from_text("not base64!").unwrap_err(), VaultError::WrongDeviceKey);
        assert_eq!(DeviceKey::from_text("AAAA").unwrap_err(), VaultError::WrongDeviceKey);
        assert_eq!(format!("{key:?}"), "DeviceKey(<redacted>)");
        assert!(!format!("{:?}", vault(b"pw")).contains("dek"));
    }
}

#[cfg(test)]
mod device_check_tests {
    use super::*;

    const COST: KdfCost = KdfCost::FAST_INSECURE;

    fn header_json(file: &[u8]) -> serde_json::Value {
        let len = u32::from_le_bytes(file[8..12].try_into().unwrap()) as usize;
        serde_json::from_slice(&file[12..12 + len]).unwrap()
    }

    fn rewritten(file: &[u8], header: &serde_json::Value) -> Vec<u8> {
        let len = u32::from_le_bytes(file[8..12].try_into().unwrap()) as usize;
        let bytes = serde_json::to_vec(header).unwrap();
        let mut out = file[..8].to_vec();
        out.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&bytes);
        out.extend_from_slice(&file[12 + len..]);
        out
    }

    #[test]
    fn a_device_slot_asks_for_a_biometric_check_that_the_header_shows_and_authenticates() {
        let mut sealed = Sealed::create(b"pw", COST, 1).unwrap();
        assert_eq!(sealed.set_device_check(Some(DeviceCheck::Biometric)).unwrap_err(), VaultError::NoDeviceSlot, "only with a device slot");
        let key = sealed.enable_device().unwrap();
        assert_eq!(sealed.device_check(), None, "a new device slot asks for nothing");
        sealed.set_device_check(Some(DeviceCheck::Biometric)).unwrap();
        let file = sealed.seal(FileKind::Vault, b"{}").unwrap();
        assert_eq!(read_header(&file).unwrap().device_check, Some(DeviceCheck::Biometric));
        let opened = Sealed::open_with_device_key(&file, &key).unwrap();
        assert_eq!(opened.sealed.device_check(), Some(&DeviceCheck::Biometric));
        // Taking the check out of the header breaks the file for every key: it cannot be skipped.
        let mut header = header_json(&file);
        for slot in header["slots"].as_array_mut().unwrap() {
            slot.as_object_mut().unwrap().remove("check");
        }
        let edited = rewritten(&file, &header);
        assert_eq!(read_header(&edited).unwrap().device_check, None);
        assert_eq!(Sealed::open_with_device_key(&edited, &key).unwrap_err(), VaultError::Corrupted);
        assert_eq!(Sealed::open_with_password(&edited, b"pw").unwrap_err(), VaultError::Corrupted);
    }

    #[test]
    fn the_check_follows_the_device_slot_and_never_reaches_a_backup() {
        let mut sealed = Sealed::create(b"pw", COST, 1).unwrap();
        let key = sealed.enable_device().unwrap();
        sealed.set_device_check(Some(DeviceCheck::Biometric)).unwrap();
        assert_eq!(sealed.change_password(b"pw", b"new pw", COST, Some(&key)).unwrap(), DeviceSlot::Kept);
        assert_eq!(sealed.device_check(), Some(&DeviceCheck::Biometric), "a new master password keeps it");
        let backup = sealed.seal(FileKind::Backup, b"{}").unwrap();
        assert_eq!(read_header(&backup).unwrap().device_check, None);
        sealed.set_device_check(None).unwrap();
        assert_eq!(sealed.device_check(), None);
        sealed.set_device_check(Some(DeviceCheck::Biometric)).unwrap();
        sealed.disable_device(b"new pw").unwrap();
        assert_eq!(sealed.device_check(), None, "gone with the slot");
        assert!(!sealed.has_device_slot());
    }

    #[test]
    fn a_check_from_a_newer_lockra_is_kept_and_named() {
        let mut sealed = Sealed::create(b"pw", COST, 1).unwrap();
        let key = sealed.enable_device().unwrap();
        sealed.set_device_check(Some(DeviceCheck::Biometric)).unwrap();
        let file = sealed.seal(FileKind::Vault, b"{}").unwrap();
        // A newer check, written by a newer Lockra under the same key: the header reads, the
        // check is named, and sealing again keeps it as it was.
        let mut header = header_json(&file);
        for slot in header["slots"].as_array_mut().unwrap() {
            if slot["kind"] == "device" {
                slot["check"] = serde_json::json!("security_key");
            }
        }
        let newer = sealed_with(&sealed, &header);
        assert_eq!(read_header(&newer).unwrap().device_check, Some(DeviceCheck::Other("security_key".into())));
        let opened = Sealed::open_with_device_key(&newer, &key).unwrap();
        let again = opened.sealed.seal(FileKind::Vault, b"{}").unwrap();
        assert_eq!(read_header(&again).unwrap().device_check, Some(DeviceCheck::Other("security_key".into())));
    }

    /// `header` sealed as `sealed` would seal it: the payload encrypted with the header as AAD.
    fn sealed_with(sealed: &Sealed, header: &serde_json::Value) -> Vec<u8> {
        let header: Header = serde_json::from_value(header.clone()).unwrap();
        let bytes = serde_json::to_vec(&header).unwrap();
        let mut file = VAULT_MAGIC.to_vec();
        file.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
        file.extend_from_slice(&bytes);
        let ciphertext = cipher(&sealed.dek).encrypt(&XNonce::from(header.payload_nonce), Payload { msg: b"{}".as_slice(), aad: &file }).unwrap();
        file.extend_from_slice(&ciphertext);
        file
    }
}
