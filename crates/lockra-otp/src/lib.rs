//! HOTP (RFC 4226) and TOTP (RFC 6238) codes, lenient Base32 (RFC 4648 §6) and `otpauth://` URIs
//! (Google's Key Uri Format, plus what the apps in the wild actually write).
//!
//! Pure logic: no clock and no I/O. Callers pass the time in, so every function is deterministic.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod base32;
mod code;
pub mod uri;

pub use code::{Algorithm, Digits, InvalidDigits, InvalidPeriod, Period, TotpWindow, hotp, totp, totp_window};
pub use uri::{OtpAuth, OtpKind, UriError};
