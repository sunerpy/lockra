//! What a file is, judged from its first bytes (the file name is not trusted).

use crate::microsoft::{SQLITE_MAGIC, is_wal};

/// The kind of a file handed to the import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A SQLite database: Microsoft Authenticator's PhoneFactor, if it has the `accounts` table.
    Sqlite,
    /// A SQLite write-ahead log (`PhoneFactor-wal`).
    SqliteWal,
    /// A Lockra vault or backup.
    Lockra,
    /// PNG, JPEG or WebP: searched for QR codes.
    Image,
    /// UTF-8 text: `otpauth://` and `otpauth-migration://` lines.
    Text,
    /// Anything else.
    Unknown,
}

/// The kind of `bytes`.
pub fn detect(bytes: &[u8]) -> Kind {
    if bytes.starts_with(SQLITE_MAGIC) {
        Kind::Sqlite
    } else if is_wal(bytes) {
        Kind::SqliteWal
    } else if bytes.starts_with(b"LKRAVLT1") || bytes.starts_with(b"LKRABAK1") {
        Kind::Lockra
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
    {
        Kind::Image
    } else if let Ok(text) = std::str::from_utf8(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)) {
        let lower = text.to_ascii_lowercase();
        if lower.contains("otpauth://") || lower.contains("otpauth-migration://") { Kind::Text } else { Kind::Unknown }
    } else {
        Kind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_come_from_the_content() {
        assert_eq!(detect(b"SQLite format 3\0rest"), Kind::Sqlite);
        assert_eq!(detect(b"\x37\x7f\x06\x82...."), Kind::SqliteWal);
        assert_eq!(detect(b"LKRABAK1...."), Kind::Lockra);
        assert_eq!(detect(b"LKRAVLT1...."), Kind::Lockra);
        assert_eq!(detect(b"\x89PNG\r\n\x1a\n...."), Kind::Image);
        assert_eq!(detect(&[0xff, 0xd8, 0xff, 0xe0]), Kind::Image);
        assert_eq!(detect(b"RIFF\0\0\0\0WEBPVP8 "), Kind::Image);
        assert_eq!(detect(b"\xef\xbb\xbfOTPAUTH://totp/x?secret=A"), Kind::Text);
        assert_eq!(detect(b"otpauth-migration://offline?data=x"), Kind::Text);
        assert_eq!(detect(b"just some notes"), Kind::Unknown);
        assert_eq!(detect(&[0xc3, 0x28]), Kind::Unknown);
        assert_eq!(detect(b""), Kind::Unknown);
    }
}
