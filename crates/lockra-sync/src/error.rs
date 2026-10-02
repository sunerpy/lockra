//! What can go wrong between a device and its sync space.

/// A sync failure. The storage errors come from the [`crate::RemoteStore`]; the others from the
/// objects themselves.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyncError {
    /// The storage could not be reached (DNS, TLS, timeout, a server error).
    #[error("the storage could not be reached: {0}")]
    Network(String),
    /// The storage refused the credentials or the request (401, 403).
    #[error("the storage refused the request")]
    Denied,
    /// A conditional write lost against another writer (412).
    #[error("the object was changed by another writer")]
    Conflict,
    /// Anything else the storage answered.
    #[error("the storage failed: {0}")]
    Storage(String),
    /// Not a Lockra sync object.
    #[error("not a Lockra sync object")]
    NotLockra,
    /// A damaged or altered object: framing, header or authentication tag.
    #[error("the object is damaged or was altered")]
    Corrupted,
    /// Written by a newer Lockra.
    #[error("the object comes from a newer Lockra (format {0})")]
    Unsupported(u32),
    /// The master password or the sync key does not open the keyring.
    #[error("the master password or the sync key is wrong")]
    WrongCredentials,
    /// An object of another space, or a device object under another device's name.
    #[error("the object belongs to another space or device")]
    Misplaced,
    /// Another device writes under this device's name (a copied vault).
    #[error("another device writes as this one")]
    DeviceClash,
    /// The sync key text is not one Lockra wrote.
    #[error("the sync key is not valid")]
    BadSyncKey,
    /// The system's random number generator failed.
    #[error("random numbers are unavailable")]
    Random,
}
