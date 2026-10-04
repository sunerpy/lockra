//! Typed errors: every failure the webview can see has a code it translates (no raw strings).

use serde::Serialize;

/// What went wrong, as a stable code for the interface's dictionaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// There is no vault yet.
    NoVault,
    /// A vault already exists.
    VaultExists,
    /// The vault is locked.
    Locked,
    /// Wrong master password (or backup password).
    WrongPassword,
    /// Too many wrong passwords in a row; `retry_at_ms` says when to try again.
    RateLimited,
    /// A new password shorter than eight characters.
    PasswordTooShort,
    /// The vault file is damaged.
    VaultCorrupted,
    /// The vault file was written by a newer Lockra.
    VaultUnsupported,
    /// The file is not a Lockra vault or backup.
    NotLockra,
    /// No usable keychain on this system (or this build).
    KeychainUnavailable,
    /// The keychain refused to store or read the device key.
    KeychainFailed,
    /// "Remember on this device" is on, but the keychain no longer has the key.
    DeviceKeyMissing,
    /// The keychain's key no longer opens the vault.
    DeviceKeyStale,
    /// "Remember on this device" is off for this vault.
    DeviceUnlockOff,
    /// Touch ID or Windows Hello was cancelled (or the password chosen instead).
    BiometricCancelled,
    /// Touch ID or Windows Hello did not recognise the user, or failed.
    BiometricFailed,
    /// No Touch ID or Windows Hello here now (no sensor, nothing enrolled, turned off).
    BiometricUnavailable,
    /// No entry with that id.
    EntryNotFound,
    /// That exact account is already in the vault.
    DuplicateEntry,
    /// An `otpauth://` URI that does not parse.
    InvalidUri,
    /// A secret that is not Base32.
    InvalidSecret,
    /// Digits, period or counter out of range.
    InvalidParameters,
    /// No import is in progress.
    NoImport,
    /// The import found nothing it can read.
    ImportEmpty,
    /// A file the import does not recognize.
    ImportUnrecognized,
    /// A file the import could not read (too large, unreadable, damaged).
    ImportUnreadable,
    /// The clipboard holds neither an image with a QR code nor text with a URI.
    ClipboardEmpty,
    /// The clipboard could not be read or written.
    ClipboardFailed,
    /// The camera may not be used: the permission was refused.
    CameraDenied,
    /// There is no camera to scan with, or it could not be opened.
    CameraUnavailable,
    /// The export session expired or was closed.
    ExportExpired,
    /// None of the chosen entries can go to that target.
    ExportNothing,
    /// No restore is in progress.
    NoRestore,
    /// Automatic backup is on but no folder is set.
    BackupDirMissing,
    /// The backup folder cannot be written.
    BackupDirUnavailable,
    /// A file could not be read or written.
    IoFailed,
    /// This copy cannot update itself (not installed from a package, or no update key).
    UpdateUnavailable,
    /// A check or an update is already running.
    UpdateBusy,
    /// The update server could not be reached, or answered with an error.
    UpdateNetwork,
    /// The update information could not be read, or has no package for this computer.
    UpdateInvalid,
    /// The downloaded package is not signed with Lockra's key (or for the version announced).
    UpdateSignature,
    /// The package could not be installed.
    UpdateInstallFailed,
    /// The administrator password prompt was cancelled.
    UpdateCancelled,
    /// Sync is not set up on this device.
    SyncOff,
    /// Sync is already set up on this device (turn it off first).
    SyncAlreadyOn,
    /// The storage's address is not one, or a required field is empty.
    SyncConfigInvalid,
    /// Plain HTTP to another computer: the storage must be reached over HTTPS.
    SyncInsecure,
    /// The storage could not be reached (offline, DNS, TLS, timeout, a server error).
    SyncNetwork,
    /// The storage refused the credentials or the request.
    SyncDenied,
    /// The storage answered with an error (a missing bucket or folder, a full disk).
    SyncStorageFailed,
    /// There is no sync space for this sync key at that storage.
    SyncSpaceNotFound,
    /// The master password or the sync key does not open the space.
    SyncWrongCredentials,
    /// The sync key is mistyped or not a Lockra sync key.
    SyncKeyInvalid,
    /// The invitation is not one Lockra made.
    SyncInviteInvalid,
    /// A shared invitation's one-time code is missing or wrong.
    SyncInviteCodeWrong,
    /// This vault's master password is right but opens nothing in the space: the space's devices
    /// use another one, which the join needs as well.
    SyncSpacePasswordNeeded,
    /// The space's data on the storage is damaged or was altered.
    SyncDataCorrupted,
    /// The space was written by a newer Lockra.
    SyncUnsupported,
    /// Anything else (a bug); details are in the log.
    Internal,
}

/// An error with its code and, for rate limiting, when to retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{code:?}")]
pub struct CoreError {
    /// What went wrong.
    pub code: ErrorCode,
    /// Unix milliseconds when a rate-limited unlock may be tried again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_at_ms: Option<u64>,
}

impl From<ErrorCode> for CoreError {
    fn from(code: ErrorCode) -> Self {
        Self { code, retry_at_ms: None }
    }
}

impl From<lockra_vault::VaultError> for CoreError {
    fn from(error: lockra_vault::VaultError) -> Self {
        use lockra_vault::VaultError as V;
        CoreError::from(match error {
            V::NotLockra => ErrorCode::NotLockra,
            V::UnsupportedVersion(_) => ErrorCode::VaultUnsupported,
            V::WrongPassword => ErrorCode::WrongPassword,
            V::WrongDeviceKey => ErrorCode::DeviceKeyStale,
            V::NoDeviceSlot => ErrorCode::DeviceUnlockOff,
            V::Corrupted => ErrorCode::VaultCorrupted,
            V::Random | V::Kdf => ErrorCode::Internal,
        })
    }
}

impl From<std::io::Error> for CoreError {
    fn from(error: std::io::Error) -> Self {
        tracing::warn!(%error, "file operation failed");
        ErrorCode::IoFailed.into()
    }
}

/// Shorthand for core results.
pub type CoreResult<T> = Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use lockra_vault::VaultError as V;

    use super::*;

    #[test]
    fn vault_errors_map_to_codes() {
        let cases = [
            (V::NotLockra, ErrorCode::NotLockra),
            (V::UnsupportedVersion(9), ErrorCode::VaultUnsupported),
            (V::WrongPassword, ErrorCode::WrongPassword),
            (V::WrongDeviceKey, ErrorCode::DeviceKeyStale),
            (V::NoDeviceSlot, ErrorCode::DeviceUnlockOff),
            (V::Corrupted, ErrorCode::VaultCorrupted),
            (V::Random, ErrorCode::Internal),
            (V::Kdf, ErrorCode::Internal),
        ];
        for (error, code) in cases {
            assert_eq!(CoreError::from(error).code, code);
        }
        assert_eq!(CoreError::from(std::io::Error::other("x")).code, ErrorCode::IoFailed);
    }

    #[test]
    fn errors_serialize_with_their_code_and_only_a_present_retry_time() {
        assert_eq!(serde_json::to_value(CoreError::from(ErrorCode::WrongPassword)).unwrap(), serde_json::json!({"code": "wrong_password"}));
        let limited = CoreError { code: ErrorCode::RateLimited, retry_at_ms: Some(5) };
        assert_eq!(serde_json::to_value(limited).unwrap(), serde_json::json!({"code": "rate_limited", "retry_at_ms": 5}));
        assert_eq!(limited.to_string(), "RateLimited");
    }
}
