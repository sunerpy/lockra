//! Import and export (docs/formats.md): Google Authenticator's transfer QR codes, Microsoft
//! Authenticator's PhoneFactor database, `otpauth://` lists, and QR codes in both directions; the
//! logins of browsers' and password managers' exports and of KeePass databases, and logins
//! written as a KeePass database or Bitwarden's CSV (§11). Lockra's own backups are
//! `lockra-vault` containers and are handled by the core.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod bitwarden;
pub mod csv_logins;
pub mod detect;
mod encoding;
pub mod google;
mod item;
pub mod kdbx;
mod login;
pub mod microsoft;
pub mod onepux;
pub mod qr;
pub mod text;

pub use item::{Incompatible, Item, Origin, RejectReason, display_label};
pub use login::{App, Found, Login, LoginField, LoginUrl, MAX_EXPANDED_BYTES, UrlMatch};
