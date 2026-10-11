//! Which build this is, as the name of its keychain items ([`crate::per_build`]): a hash of the code
//! signature the executable carries (`LC_CODE_SIGNATURE`).
//!
//! The signature holds the hash of every page of the code and the certificate's signature over
//! them, so two builds never share it, and the same file always gives the same name: the staged
//! copy an update hands over to and the copy installed from it are the same bytes. The name only
//! has to tell builds apart; which build may read an item is the keychain's business (the item's
//! partition, which is the code's cdhash). Plain parsing, so it is tested on every platform.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use sha2::{Digest as _, Sha256};

/// The designated requirement of Lockra's macOS releases: the bundle identifier, signed with the
/// project's self-signed release certificate (`.github/macos-signing.json` pins its SHA-1). One
/// build hands its keychain items only to a process that satisfies it (keychain_handoff.rs).
pub const RELEASE_REQUIREMENT: &str = "identifier \"dev.lockra.desktop\" and certificate leaf = H\"d660014e62b11fa7743e582e5fd52c110da66855\"";

const MH_MAGIC_64: u32 = 0xfeed_facf;
const FAT_MAGIC: u32 = 0xcafe_babe;
const FAT_MAGIC_64: u32 = 0xcafe_babf;
const LC_CODE_SIGNATURE: u32 = 0x1d;
const CPU_TYPE_X86_64: u32 = 0x0100_0007;
const CPU_TYPE_ARM64: u32 = 0x0100_000c;
/// The most load commands read: far above what an executable has.
const MAX_LOAD_COMMANDS: u32 = 4096;
/// The most signature read: a few hundred kilobytes for a large executable.
const MAX_SIGNATURE: u64 = 16 * 1024 * 1024;

/// The CPU type of this process, as Mach-O names it.
fn this_cpu() -> u32 {
    if cfg!(target_arch = "aarch64") { CPU_TYPE_ARM64 } else { CPU_TYPE_X86_64 }
}

/// The build name of the executable at `path`; `None` for a file without a code signature.
pub fn build_id_of(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    build_id(&mut file, this_cpu())
}

/// The build name of the Mach-O in `file` (for a universal file, its `cpu` slice): the first 20
/// bytes of the SHA-256 of its code signature, in hex.
pub fn build_id<F: Read + Seek>(file: &mut F, cpu: u32) -> Option<String> {
    let base = slice_offset(file, cpu)?;
    let (offset, size) = signature_range(file, base)?;
    if size == 0 || size > MAX_SIGNATURE {
        return None;
    }
    file.seek(SeekFrom::Start(base.checked_add(offset)?)).ok()?;
    let mut signature = vec![0u8; usize::try_from(size).ok()?];
    file.read_exact(&mut signature).ok()?;
    let digest = Sha256::digest(&signature);
    Some(digest[..20].iter().map(|b| format!("{b:02x}")).collect())
}

fn u32_at(bytes: &[u8], at: usize, big_endian: bool) -> Option<u32> {
    let raw: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(if big_endian { u32::from_be_bytes(raw) } else { u32::from_le_bytes(raw) })
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    let raw: [u8; 8] = bytes.get(at..at + 8)?.try_into().ok()?;
    Some(u64::from_be_bytes(raw))
}

/// Where the Mach-O for `cpu` starts: 0 for a thin file, its slice's offset in a universal one.
fn slice_offset<F: Read + Seek>(file: &mut F, cpu: u32) -> Option<u64> {
    let mut head = [0u8; 8];
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_exact(&mut head).ok()?;
    let magic = u32_at(&head, 0, true)?;
    if magic != FAT_MAGIC && magic != FAT_MAGIC_64 {
        return Some(0);
    }
    let count = u32_at(&head, 4, true)?;
    if count > 64 {
        return None;
    }
    let entry = if magic == FAT_MAGIC { 20 } else { 32 };
    let mut table = vec![0u8; usize::try_from(count).ok()? * entry];
    file.read_exact(&mut table).ok()?;
    table
        .chunks(entry)
        .find(|arch| u32_at(arch, 0, true) == Some(cpu))
        .and_then(|arch| if magic == FAT_MAGIC { u32_at(arch, 8, true).map(u64::from) } else { u64_at(arch, 8) })
}

/// The code signature's offset and size, from the load commands of the Mach-O at `base`.
fn signature_range<F: Read + Seek>(file: &mut F, base: u64) -> Option<(u64, u64)> {
    let mut header = [0u8; 32];
    file.seek(SeekFrom::Start(base)).ok()?;
    file.read_exact(&mut header).ok()?;
    if u32_at(&header, 0, false)? != MH_MAGIC_64 {
        return None;
    }
    let count = u32_at(&header, 16, false)?;
    let size = u32_at(&header, 20, false)?;
    if count > MAX_LOAD_COMMANDS || size > 16 * 1024 * 1024 {
        return None;
    }
    let mut commands = vec![0u8; usize::try_from(size).ok()?];
    file.read_exact(&mut commands).ok()?;
    let mut at = 0usize;
    for _ in 0..count {
        let cmd = u32_at(&commands, at, false)?;
        let len = usize::try_from(u32_at(&commands, at + 4, false)?).ok()?;
        if len < 8 {
            return None;
        }
        if cmd == LC_CODE_SIGNATURE {
            return Some((u64::from(u32_at(&commands, at + 8, false)?), u64::from(u32_at(&commands, at + 12, false)?)));
        }
        at = at.checked_add(len)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    /// A thin 64-bit Mach-O with one unrelated load command and, when `signature` is given, an
    /// `LC_CODE_SIGNATURE` pointing at it at the end of the file.
    fn thin(signature: Option<&[u8]>) -> Vec<u8> {
        let mut commands = Vec::new();
        // LC_UUID (0x1b), 24 bytes.
        commands.extend_from_slice(&0x1bu32.to_le_bytes());
        commands.extend_from_slice(&24u32.to_le_bytes());
        commands.extend_from_slice(&[7u8; 16]);
        let ncmds = if signature.is_some() { 2u32 } else { 1u32 };
        let sizeofcmds = commands.len() as u32 + if signature.is_some() { 16 } else { 0 };
        let data_at = 32 + sizeofcmds + 64;
        if let Some(signature) = signature {
            commands.extend_from_slice(&LC_CODE_SIGNATURE.to_le_bytes());
            commands.extend_from_slice(&16u32.to_le_bytes());
            commands.extend_from_slice(&data_at.to_le_bytes());
            commands.extend_from_slice(&(signature.len() as u32).to_le_bytes());
        }
        let mut file = Vec::new();
        file.extend_from_slice(&MH_MAGIC_64.to_le_bytes());
        file.extend_from_slice(&CPU_TYPE_ARM64.to_le_bytes());
        file.extend_from_slice(&[0u8; 8]);
        file.extend_from_slice(&ncmds.to_le_bytes());
        file.extend_from_slice(&sizeofcmds.to_le_bytes());
        file.extend_from_slice(&[0u8; 8]);
        file.extend_from_slice(&commands);
        file.extend_from_slice(&[0u8; 64]);
        if let Some(signature) = signature {
            file.extend_from_slice(signature);
        }
        file
    }

    fn id(bytes: &[u8]) -> Option<String> {
        build_id(&mut Cursor::new(bytes), CPU_TYPE_ARM64)
    }

    #[test]
    fn the_name_is_the_hash_of_the_code_signature() {
        let one = id(&thin(Some(b"signature of build one"))).unwrap();
        assert_eq!(one.len(), 40);
        assert!(one.chars().all(|c| c.is_ascii_hexdigit()));
        // The same bytes always give the same name; another signature another one.
        assert_eq!(id(&thin(Some(b"signature of build one"))).as_deref(), Some(one.as_str()));
        assert_ne!(id(&thin(Some(b"signature of build two"))).unwrap(), one);
        let digest = Sha256::digest(b"signature of build one");
        assert_eq!(one, digest[..20].iter().map(|b| format!("{b:02x}")).collect::<String>());
    }

    #[test]
    fn a_universal_file_names_the_slice_of_this_cpu() {
        let arm = thin(Some(b"arm64 signature"));
        let intel = thin(Some(b"x86_64 signature"));
        let mut file = Vec::new();
        file.extend_from_slice(&FAT_MAGIC.to_be_bytes());
        file.extend_from_slice(&2u32.to_be_bytes());
        let first = 4096u32;
        let second = first + 4096;
        for (cpu, offset, len) in [(CPU_TYPE_X86_64, first, intel.len()), (CPU_TYPE_ARM64, second, arm.len())] {
            file.extend_from_slice(&cpu.to_be_bytes());
            file.extend_from_slice(&0u32.to_be_bytes());
            file.extend_from_slice(&offset.to_be_bytes());
            file.extend_from_slice(&(len as u32).to_be_bytes());
            file.extend_from_slice(&12u32.to_be_bytes());
        }
        file.resize(first as usize, 0);
        file.extend_from_slice(&intel);
        file.resize(second as usize, 0);
        file.extend_from_slice(&arm);
        assert_eq!(build_id(&mut Cursor::new(&file), CPU_TYPE_ARM64), id(&arm));
        assert_eq!(build_id(&mut Cursor::new(&file), CPU_TYPE_X86_64), id(&intel));
        assert_ne!(id(&arm), id(&intel));
        // A CPU the file has no slice for.
        assert_eq!(build_id(&mut Cursor::new(&file), 0x0200_0000), None);
    }

    #[test]
    fn a_file_without_a_signature_has_no_name() {
        assert_eq!(id(&thin(None)), None);
        assert_eq!(id(b"#!/bin/sh\necho not a mach-o\n"), None);
        assert_eq!(id(&[]), None);
        // A signature that runs past the end of the file.
        let mut cut = thin(Some(b"a signature"));
        cut.truncate(cut.len() - 3);
        assert_eq!(id(&cut), None);
    }

    /// The requirement the app checks is the certificate the release pipeline signs with.
    #[test]
    fn the_release_requirement_names_the_pinned_certificate() {
        let pin: serde_json::Value = serde_json::from_str(include_str!("../../../../.github/macos-signing.json")).unwrap();
        let sha1 = pin["certificate_sha1"].as_str().unwrap().to_ascii_lowercase();
        let identifier = pin["identifier"].as_str().unwrap();
        assert_eq!(RELEASE_REQUIREMENT, format!("identifier \"{identifier}\" and certificate leaf = H\"{sha1}\""));
        assert_eq!(identifier, crate::KEYCHAIN_SERVICE);
    }

    #[test]
    fn a_file_on_disk_is_read_like_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lockra-desktop");
        let bytes = thin(Some(b"on disk"));
        std::fs::write(&path, &bytes).unwrap();
        let expected = if cfg!(target_arch = "aarch64") || cfg!(target_arch = "x86_64") { id(&bytes) } else { None };
        // A thin file is read whatever this machine's CPU is.
        assert_eq!(build_id_of(&path), expected.or_else(|| id(&bytes)));
        assert_eq!(build_id_of(&dir.path().join("missing")), None);
    }
}
