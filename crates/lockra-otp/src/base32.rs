//! Base32 (RFC 4648 §6) the way people paste secrets into an authenticator.

use zeroize::Zeroizing;

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Why a secret could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Base32Error {
    /// Nothing but separators, or no character at all.
    #[error("the secret is empty")]
    Empty,
    /// Fewer than eight bits: not one whole byte.
    #[error("the secret is shorter than one byte")]
    TooShort,
    /// A character outside the Base32 alphabet (`0`, `1`, `8`, `9`, punctuation, …).
    #[error("{0:?} is not a Base32 character")]
    InvalidCharacter(char),
}

/// Decode a secret. Letters in either case are accepted, spaces, tabs, line breaks, `-` and `=`
/// padding are skipped wherever they are, and the bits left after the last whole byte are dropped:
/// services print secrets whose length is not a multiple of eight characters, and the strict
/// decoders that refuse those lose accounts that every phone app accepts.
pub fn decode(input: &str) -> Result<Zeroizing<Vec<u8>>, Base32Error> {
    let mut out = Zeroizing::new(Vec::with_capacity(input.len() * 5 / 8 + 1));
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    let mut symbols = 0usize;
    for c in input.chars() {
        let value = match c {
            'A'..='Z' => u32::from(c) - u32::from('A'),
            'a'..='z' => u32::from(c) - u32::from('a'),
            '2'..='7' => u32::from(c) - u32::from('2') + 26,
            ' ' | '\t' | '\r' | '\n' | '-' | '=' => continue,
            other => return Err(Base32Error::InvalidCharacter(other)),
        };
        symbols += 1;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    match (symbols, out.is_empty()) {
        (0, _) => Err(Base32Error::Empty),
        (_, true) => Err(Base32Error::TooShort),
        _ => Ok(out),
    }
}

/// Uppercase Base32 without padding: the form `otpauth://` URIs carry and people type.
pub fn encode(bytes: &[u8]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::with_capacity((bytes.len() * 8).div_ceil(5)));
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(ALPHABET[((buffer >> bits) & 31) as usize]));
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(char::from(ALPHABET[((buffer << (5 - bits)) & 31) as usize]));
    }
    out
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn rfc4648_vectors() {
        // RFC 4648 §10, padding removed.
        let vectors = [("", ""), ("f", "MY"), ("fo", "MZXQ"), ("foo", "MZXW6"), ("foob", "MZXW6YQ"), ("fooba", "MZXW6YTB"), ("foobar", "MZXW6YTBOI")];
        for (plain, coded) in vectors {
            assert_eq!(encode(plain.as_bytes()).as_str(), coded);
            if !plain.is_empty() {
                assert_eq!(decode(coded).unwrap().as_slice(), plain.as_bytes());
            }
        }
    }

    #[test]
    fn lenient_input_is_accepted() {
        let expected = b"Hello!\xde\xad\xbe\xef".as_slice();
        assert_eq!(decode("JBSWY3DPEHPK3PXP").unwrap().as_slice(), expected);
        assert_eq!(decode("jbsw y3dp ehpk 3pxp").unwrap().as_slice(), expected);
        assert_eq!(decode("JBSW-Y3DP-EHPK-3PXP\n").unwrap().as_slice(), expected);
        assert_eq!(decode("MZXW6===").unwrap().as_slice(), b"foo");
        // Three symbols (15 bits): one whole byte, the rest dropped, as phone apps do.
        assert_eq!(decode("MZX").unwrap().as_slice(), b"f");
    }

    #[test]
    fn invalid_input_is_refused() {
        assert_eq!(decode("").unwrap_err(), Base32Error::Empty);
        assert_eq!(decode("  - = ").unwrap_err(), Base32Error::Empty);
        assert_eq!(decode("M").unwrap_err(), Base32Error::TooShort);
        assert_eq!(decode("MZX0").unwrap_err(), Base32Error::InvalidCharacter('0'));
        assert_eq!(decode("MZXW6!").unwrap_err(), Base32Error::InvalidCharacter('!'));
        assert_eq!(decode("ÄBC").unwrap_err(), Base32Error::InvalidCharacter('Ä'));
    }

    proptest! {
        #[test]
        fn decode_inverts_encode(bytes in proptest::collection::vec(any::<u8>(), 1..80)) {
            let decoded = decode(&encode(&bytes)).unwrap();
            prop_assert_eq!(decoded.as_slice(), bytes.as_slice());
        }
    }
}
