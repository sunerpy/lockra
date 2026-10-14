//! KeePass key files, as KeePass 2 reads them: an XML key file (version 1.0, Base64; version 2.0,
//! hex with a check of four bytes), 32 bytes taken as they are, 64 hex digits, or any other file
//! hashed with SHA-256. Only the 32 bytes the composite key takes are kept, never the file.

use quick_xml::events::Event;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// The key a key file brings to a database's composite key.
pub struct KeyFile(Zeroizing<[u8; 32]>);

/// A file that is not a key file: empty, or an XML key file that does not read (another
/// version, data of the wrong length, a check that fails).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("not a KeePass key file")]
pub struct KeyFileError;

impl KeyFile {
    /// The key of the file with `bytes`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KeyFileError> {
        if bytes.is_empty() {
            return Err(KeyFileError);
        }
        if let Some(xml) = xml_key(bytes) {
            return xml.map(Self);
        }
        let mut key = Zeroizing::new([0u8; 32]);
        if bytes.len() == 32 {
            key.copy_from_slice(bytes);
        } else if let Some(decoded) = (bytes.len() == 64).then(|| hex32(bytes)).flatten() {
            *key = *decoded;
        } else {
            key.copy_from_slice(&Sha256::digest(bytes));
        }
        Ok(Self(key))
    }

    pub(crate) fn key(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for KeyFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeyFile(<32 bytes>)")
    }
}

/// The key of an XML key file; `None` when the file is not one (no XML, or another root
/// element), so that it is hashed as a file like any other.
fn xml_key(bytes: &[u8]) -> Option<Result<Zeroizing<[u8; 32]>, KeyFileError>> {
    let text = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let mut reader = quick_xml::Reader::from_reader(text);
    let mut path: Vec<String> = Vec::new();
    let mut version = String::new();
    let mut data = Zeroizing::new(String::new());
    let mut hash: Option<String> = None;
    let mut buf = Vec::new();
    loop {
        let event = match reader.read_event_into(&mut buf) {
            Ok(event) => event,
            // Not XML before the root element: a file like any other. Past it: a damaged key file.
            Err(_) => return (!path.is_empty() || !version.is_empty() || !data.is_empty()).then_some(Err(KeyFileError)),
        };
        match event {
            Event::Start(start) => {
                let name = start.name().as_ref().to_owned();
                if path.is_empty() && name != "KeyFile" {
                    return None;
                }
                if path.len() > 8 {
                    return Some(Err(KeyFileError));
                }
                path.push(name);
                if path == ["KeyFile", "Key", "Data"] {
                    hash = start.attributes().flatten().find(|a| a.key.as_ref() == "Hash").map(|a| a.value.to_string());
                }
            }
            Event::Empty(start) if path.is_empty() => {
                return (start.name().as_ref() == "KeyFile").then_some(Err(KeyFileError));
            }
            Event::End(_) => {
                path.pop();
                if path.is_empty() {
                    break;
                }
            }
            Event::Text(text) => {
                let content = text.xml10_content();
                if path == ["KeyFile", "Meta", "Version"] {
                    version.push_str(&content);
                } else if path == ["KeyFile", "Key", "Data"] {
                    data.push_str(&content);
                } else if path.is_empty() && !content.trim().is_empty() {
                    return None;
                }
            }
            Event::Eof => {
                if path.is_empty() && version.is_empty() && data.is_empty() {
                    return None;
                }
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    let compact: Zeroizing<String> = Zeroizing::new(data.chars().filter(|c| !c.is_whitespace()).collect());
    Some(match version.trim() {
        "1.0" | "1.00" => data_encoding::BASE64
            .decode(compact.as_bytes())
            .map(Zeroizing::new)
            .ok()
            .and_then(|decoded| <[u8; 32]>::try_from(decoded.as_slice()).ok())
            .map(Zeroizing::new)
            .ok_or(KeyFileError),
        "2.0" | "2.00" => {
            let key = hex32(compact.as_bytes()).ok_or(KeyFileError);
            match (key, hash) {
                (Ok(key), Some(hash)) => {
                    let check = Sha256::digest(*key);
                    if hex_bytes(hash.trim()).is_some_and(|h| h.as_slice() == &check[..4]) { Ok(key) } else { Err(KeyFileError) }
                }
                (key, None) => key,
                (Err(error), _) => Err(error),
            }
        }
        _ => Err(KeyFileError),
    })
}

/// 64 hex digits as 32 bytes.
fn hex32(text: &[u8]) -> Option<Zeroizing<[u8; 32]>> {
    let bytes = hex_bytes(std::str::from_utf8(text).ok()?)?;
    <[u8; 32]>::try_from(bytes.as_slice()).ok().map(Zeroizing::new)
}

/// Hex digits (either case) as bytes.
fn hex_bytes(text: &str) -> Option<Zeroizing<Vec<u8>>> {
    data_encoding::HEXUPPER_PERMISSIVE.decode(text.as_bytes()).ok().map(Zeroizing::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    const V2: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<KeyFile>
	<Meta>
		<Version>2.0</Version>
	</Meta>
	<Key>
		<Data Hash="A65F0C2D">
			36057B1C 35037FD9 62257893 C0A22403
			EE3F8FBB 504D9981 08B821CB 00D28F89
		</Data>
	</Key>
</KeyFile>
"#;

    fn key_of(bytes: &[u8]) -> [u8; 32] {
        *KeyFile::from_bytes(bytes).unwrap().key()
    }

    #[test]
    fn xml_key_files_of_both_versions() {
        let v2 = key_of(V2.as_bytes());
        assert_eq!(v2[..4], [0x36, 0x05, 0x7b, 0x1c]);
        assert_eq!(v2[28..], [0x00, 0xd2, 0x8f, 0x89]);
        // A byte order mark, and no check: still the same key.
        let unchecked = V2.replace(" Hash=\"A65F0C2D\"", "");
        assert_eq!(key_of(format!("\u{feff}{unchecked}").as_bytes()), v2);
        let v1 = "<KeyFile><Meta><Version>1.00</Version></Meta><Key><Data>NXyYiJMHg3ls+eBmjbAjWec9lcOToJiofbhNiFMTJMw=</Data></Key></KeyFile>";
        assert_eq!(key_of(v1.as_bytes())[..3], [0x35, 0x7c, 0x98]);
    }

    #[test]
    fn a_damaged_xml_key_file_is_refused() {
        let wrong_check = V2.replace("A65F0C2D", "00000000");
        let short = V2.replace("00D28F89", "");
        let v1_short = "<KeyFile><Meta><Version>1.0</Version></Meta><Key><Data>AAAA</Data></Key></KeyFile>";
        let other_version = V2.replace("<Version>2.0</Version>", "<Version>3.0</Version>");
        let cut = &V2[..V2.len() - 40];
        for text in [wrong_check.as_str(), short.as_str(), v1_short, other_version.as_str(), cut, "<KeyFile/>", ""] {
            assert_eq!(KeyFile::from_bytes(text.as_bytes()).unwrap_err(), KeyFileError, "{text}");
        }
    }

    #[test]
    fn other_files_are_keys_by_their_bytes() {
        let raw = [7u8; 32];
        assert_eq!(key_of(&raw), raw);
        let hex = "0102030405060708090a0b0c0d0e0f101112131415161718191A1B1C1D1E1F20";
        assert_eq!(key_of(hex.as_bytes())[..3], [1, 2, 3]);
        // Anything else, XML of another root included, is hashed whole.
        for other in [&b"a photo, a document, anything"[..], b"<Not><KeyFile/></Not>", &[b'x'; 64], b"\x89PNG\r\n\x1a\n binary"] {
            assert_eq!(key_of(other), <[u8; 32]>::from(Sha256::digest(other)), "{}", String::from_utf8_lossy(other));
        }
        assert_eq!(format!("{:?}", KeyFile::from_bytes(&raw).unwrap()), "KeyFile(<32 bytes>)");
    }
}
