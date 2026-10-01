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
