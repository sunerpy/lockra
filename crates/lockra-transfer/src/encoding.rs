//! Base64 as it arrives from other apps: either alphabet, padded or not.

use std::sync::OnceLock;

use data_encoding::{Encoding, Specification};
use zeroize::Zeroizing;

/// Decode standard or URL-safe Base64, with or without padding, ignoring whitespace. A `+` that a
/// form decoder already turned into a space comes back as `+`. `None` when it is not Base64.
pub(crate) fn decode_base64(text: &str) -> Option<Zeroizing<Vec<u8>>> {
    let normalized = normalize_base64(text);
    lenient_base64().decode(normalized.as_bytes()).ok().map(Zeroizing::new)
}

/// Both alphabets folded into the standard one, padding and whitespace dropped.
fn normalize_base64(text: &str) -> Zeroizing<String> {
    Zeroizing::new(
        text.chars()
            .filter_map(|c| match c {
                '-' => Some('+'),
                '_' => Some('/'),
                ' ' => Some('+'),
                '=' | '\n' | '\r' | '\t' => None,
                other => Some(other),
            })
            .collect(),
    )
}

/// Base64 without padding that tolerates non-zero trailing bits.
fn lenient_base64() -> &'static Encoding {
    static ENCODING: OnceLock<Encoding> = OnceLock::new();
    ENCODING.get_or_init(|| {
        let mut spec = Specification::new();
        spec.symbols.push_str("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/");
        spec.check_trailing_bits = false;
        // A fixed, valid specification: this cannot fail.
        #[allow(clippy::expect_used)]
        spec.encoding().expect("valid Base64 specification")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_decodes() {
        for text in ["+/8=", "-_8", "+/8", " /8=", "+/\n8="] {
            assert_eq!(decode_base64(text).unwrap().as_slice(), [0xfb, 0xff], "{text:?}");
        }
        assert!(decode_base64("a%b").is_none());
        assert!(decode_base64("A").is_none());
    }
}
