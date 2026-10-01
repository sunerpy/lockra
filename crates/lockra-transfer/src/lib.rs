//! Import and export (docs/formats.md): Google Authenticator's transfer QR codes, Microsoft
//! Authenticator's PhoneFactor database, `otpauth://` lists, and QR codes in both directions.
//! Lockra's own backups are `lockra-vault` containers and are handled by the core.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod detect;
mod encoding;
pub mod google;
mod item;
pub mod microsoft;
pub mod qr;
pub mod text;

pub use item::{Incompatible, Item, Origin, RejectReason, display_label};
