//! Base64 (RFC 4648 §4, padded) for the binary fields of the JSON header.

use data_encoding::BASE64;
use serde::{Deserialize, Deserializer, Serializer, de};

/// A fixed-size byte array as one Base64 string.
pub mod array {
    use super::*;

    pub fn serialize<S: Serializer, const N: usize>(bytes: &[u8; N], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&BASE64.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = BASE64.decode(text.as_bytes()).map_err(de::Error::custom)?;
        <[u8; N]>::try_from(bytes.as_slice()).map_err(|_| de::Error::invalid_length(bytes.len(), &"a fixed-size field"))
    }
}

/// A byte vector as one Base64 string.
pub mod vec {
    use super::*;

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&BASE64.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        BASE64.decode(text.as_bytes()).map_err(de::Error::custom)
    }
}
