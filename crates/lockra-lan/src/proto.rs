//! The requests a device sends its hub and the hub's answers: a message's JSON header. An object's
//! bytes, and a pairing's welcome, go in the body.

use lockra_sync::{ObjectMeta, PutCondition, SyncError};
use serde::{Deserialize, Serialize};

/// What a device asks its hub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// The objects in the space's devices directory.
    List { dir: String },
    /// An object; the answer's body is its bytes.
    Get { path: String },
    /// Write this device's object, the body, under `condition`.
    Put { path: String, condition: Condition },
    /// Remove this device's object.
    Delete { path: String },
    /// This device writes under `tag` from now on.
    Register { tag: String },
    /// A device asking to pair, under the offer's key: its name and platform, for the user at the
    /// hub.
    Join { name: String, platform: String },
}

/// [`PutCondition`] on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "etag", rename_all = "snake_case")]
pub enum Condition {
    Always,
    IfAbsent,
    IfMatch(String),
}

impl From<PutCondition> for Condition {
    fn from(condition: PutCondition) -> Self {
        match condition {
            PutCondition::Always => Self::Always,
            PutCondition::IfAbsent => Self::IfAbsent,
            PutCondition::IfMatch(etag) => Self::IfMatch(etag),
        }
    }
}

impl From<Condition> for PutCondition {
    fn from(condition: Condition) -> Self {
        match condition {
            Condition::Always => Self::Always,
            Condition::IfAbsent => Self::IfAbsent,
            Condition::IfMatch(etag) => Self::IfMatch(etag),
        }
    }
}

/// An object in a listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Object {
    pub name: String,
    pub etag: Option<String>,
    pub size: u64,
}

impl From<ObjectMeta> for Object {
    fn from(meta: ObjectMeta) -> Self {
        Self { name: meta.name, etag: meta.etag, size: meta.size }
    }
}

impl From<Object> for ObjectMeta {
    fn from(object: Object) -> Self {
        Self { name: object.name, etag: object.etag, size: object.size }
    }
}

/// The hub's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum Answer {
    Listed {
        objects: Vec<Object>,
    },
    /// The body holds the object when `found`.
    Got {
        found: bool,
        etag: Option<String>,
    },
    Put {
        etag: Option<String>,
    },
    Done,
    /// The user at the hub agreed: the body is the welcome.
    Welcome,
    /// The user at the hub said no, or did not answer in time.
    Refused,
    Failed {
        error: Failure,
    },
}

/// Why the hub did not do what was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// Not the space's, or not this device's to write.
    Denied,
    /// A condition did not hold.
    Conflict,
    /// Larger than an object may be.
    Corrupted,
    /// The hub could not read or write its folder.
    Storage,
    /// The hub no longer pairs with this device; it keeps the key to say so.
    Removed,
    /// The hub serves as many connections as it may.
    Busy,
    /// Not a request of this version.
    Unsupported,
}

impl Failure {
    /// What a store's error tells a device.
    pub fn of(error: &SyncError) -> Self {
        match error {
            SyncError::Conflict => Self::Conflict,
            SyncError::Corrupted => Self::Corrupted,
            SyncError::Denied | SyncError::Misplaced => Self::Denied,
            _ => Self::Storage,
        }
    }

    /// The error a device's run sees.
    pub fn error(self) -> SyncError {
        match self {
            Self::Denied => SyncError::Denied,
            Self::Conflict => SyncError::Conflict,
            Self::Corrupted => SyncError::Corrupted,
            Self::Storage => SyncError::Storage("the hub could not use its folder".into()),
            Self::Removed => SyncError::WrongCredentials,
            Self::Busy => SyncError::Network("the hub is busy".into()),
            Self::Unsupported => SyncError::Storage("the hub does not know this request".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_answers_read_back_and_say_what_they_are() {
        let request = Request::Put { path: "lockra-sync-v1/x/devices/ab.lks".into(), condition: Condition::IfMatch("e1".into()) };
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(text, r#"{"op":"put","path":"lockra-sync-v1/x/devices/ab.lks","condition":{"kind":"if_match","etag":"e1"}}"#);
        assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
        let answer = Answer::Failed { error: Failure::Removed };
        assert_eq!(serde_json::to_string(&answer).unwrap(), r#"{"answer":"failed","error":"removed"}"#);
        assert!(serde_json::from_str::<Request>(r#"{"op":"format_disk"}"#).is_err());
        for (condition, back) in [
            (PutCondition::Always, Condition::Always),
            (PutCondition::IfAbsent, Condition::IfAbsent),
            (PutCondition::IfMatch("e".into()), Condition::IfMatch("e".into())),
        ] {
            assert_eq!(Condition::from(condition.clone()), back.clone());
            assert_eq!(PutCondition::from(back), condition);
        }
        let meta = ObjectMeta { name: "ab.lks".into(), etag: Some("e".into()), size: 3 };
        assert_eq!(ObjectMeta::from(Object::from(meta.clone())), meta);
    }

    #[test]
    fn failures_map_both_ways() {
        assert_eq!(Failure::of(&SyncError::Conflict), Failure::Conflict);
        assert_eq!(Failure::of(&SyncError::Corrupted), Failure::Corrupted);
        assert_eq!(Failure::of(&SyncError::Misplaced), Failure::Denied);
        assert_eq!(Failure::of(&SyncError::Storage("disk full".into())), Failure::Storage);
        assert_eq!(Failure::Removed.error(), SyncError::WrongCredentials);
        assert!(matches!(Failure::Busy.error(), SyncError::Network(_)));
        assert!(matches!(Failure::Unsupported.error(), SyncError::Storage(_)));
        assert_eq!(Failure::Denied.error(), SyncError::Denied);
    }
}
