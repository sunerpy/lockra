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
    /// The master password or the sync key opens none of the space's keyrings.
    #[error("the master password or the sync key is wrong")]
    WrongCredentials,
    /// Nothing of the space is where it was looked for: no device snapshot at all.
    #[error("the sync space is not at this place")]
    NoSpace,
    /// An object of another space, or a device object under another device's name.
    #[error("the object belongs to another space or device")]
    Misplaced,
    /// Another device writes under this device's name (a copied vault).
    #[error("another device writes as this one")]
    DeviceClash,
    /// The sync key text is not one Lockra wrote.
    #[error("the sync key is not valid")]
    BadSyncKey,
    /// The invitation text is not one Lockra wrote.
    #[error("the invitation is not valid")]
    BadInvite,
    /// A shared invitation's one-time code is missing or does not open it.
    #[error("the invitation code is not right")]
    BadInviteCode,
    /// The caller stopped the run before it wrote (the vault was locked meanwhile).
    #[error("the run was stopped")]
    Interrupted,
    /// The system's random number generator failed.
    #[error("random numbers are unavailable")]
    Random,
}
