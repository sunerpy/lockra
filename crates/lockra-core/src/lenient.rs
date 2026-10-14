//! Reading what a later Lockra may have written: a value of a kind this version does not know is
//! kept as it was written instead of failing the payload, and a part of the vault's local part
//! that does not read goes back to its default instead of taking the vault with it (R16).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One of the values this version knows, or the text a later version wrote for a value it added:
/// kept as written, and written back unchanged, so that a device on this version passes it on.
/// For the string forms of plain enums (`"login"`, `"never"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Lenient<T> {
    /// A value this version knows.
    Known(T),
    /// What a later version wrote.
    Unknown(String),
}

impl<T> Lenient<T> {
    /// The value, when this version knows it.
    pub fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown(_) => None,
        }
    }
}

impl<T: Copy> Lenient<T> {
    /// The value, or `fallback` for one this version does not know.
    pub fn or(&self, fallback: T) -> T {
        self.known().copied().unwrap_or(fallback)
    }
}

impl<T: Default> Default for Lenient<T> {
    fn default() -> Self {
        Self::Known(T::default())
    }
}

impl<T> From<T> for Lenient<T> {
    fn from(value: T) -> Self {
        Self::Known(value)
    }
}

impl<T: Serialize> Serialize for Lenient<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Known(value) => value.serialize(serializer),
            Self::Unknown(text) => serializer.serialize_str(text),
        }
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Lenient<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(match serde_json::from_value(serde_json::Value::String(text.clone())) {
            Ok(value) => Self::Known(value),
            Err(_) => Self::Unknown(text),
        })
    }
}

/// A part of the vault's local part read leniently: what does not read as `T` (a later version's
/// shape, a damaged value) is `T`'s default, and the rest of the vault opens. The text is parsed
/// where it lies in the payload (no copy of it, which may hold secrets, is made first), so the
/// payload must be read from bytes, as the vault and its backups are.
pub(crate) fn or_default<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(deserializer: D) -> Result<T, D::Error> {
    let raw: &'de serde_json::value::RawValue = Deserialize::deserialize(deserializer)?;
    Ok(serde_json::from_str(raw.get()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Kind {
        #[default]
        Login,
        SecureNote,
    }

    #[test]
    fn a_value_of_a_later_kind_is_kept_as_written() {
        let known: Lenient<Kind> = serde_json::from_str(r#""secure_note""#).unwrap();
        assert_eq!(known, Lenient::Known(Kind::SecureNote));
        let later: Lenient<Kind> = serde_json::from_str(r#""bank_card""#).unwrap();
        assert_eq!(later, Lenient::Unknown("bank_card".into()));
        assert_eq!(serde_json::to_string(&later).unwrap(), r#""bank_card""#, "written back unchanged");
        assert_eq!(serde_json::to_string(&known).unwrap(), r#""secure_note""#);
        assert_eq!((later.known(), later.or(Kind::Login)), (None, Kind::Login));
        assert_eq!(Lenient::<Kind>::default(), Lenient::Known(Kind::Login));
        assert_eq!(Lenient::from(Kind::SecureNote).or(Kind::Login), Kind::SecureNote);
        // Not a string at all is still an error: that is damage, not a later kind.
        assert!(serde_json::from_str::<Lenient<Kind>>("7").is_err());
    }

    #[derive(Debug, Default, PartialEq, Deserialize)]
    struct Part {
        #[serde(default)]
        names: Vec<String>,
    }

    #[derive(Debug, PartialEq, Deserialize)]
    struct Holder {
        #[serde(default, deserialize_with = "or_default")]
        part: Part,
        kept: u32,
    }

    #[test]
    fn a_part_that_does_not_read_is_its_default_and_the_rest_reads() {
        let fine: Holder = serde_json::from_slice(br#"{"part":{"names":["a"]},"kept":1}"#).unwrap();
        assert_eq!(fine, Holder { part: Part { names: vec!["a".into()] }, kept: 1 });
        let later: Holder = serde_json::from_slice(br#"{"part":{"names":"not a list"},"kept":2}"#).unwrap();
        assert_eq!(later, Holder { part: Part::default(), kept: 2 });
        let missing: Holder = serde_json::from_slice(br#"{"kept":3}"#).unwrap();
        assert_eq!(missing.part, Part::default());
        // Broken JSON is broken whatever the field: the payload does not read.
        assert!(serde_json::from_slice::<Holder>(br#"{"part":{"names":[},"kept":4}"#).is_err());
    }
}
