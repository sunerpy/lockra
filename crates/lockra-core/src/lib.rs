//! The application core of Lockra: the session state machine (no vault, locked, unlocked), the
//! entries, import, export, backup, restore and settings, behind ports for everything the platform
//! provides (keychain, clipboard, clock, the code stream). The Tauri shell only wires the real
//! ports in and forwards commands and events; the logic is tested here with the fakes.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod backup;
mod entry;
mod error;
mod export;
mod import;
pub mod ports;
mod session;
pub mod settings;
pub mod ui;

#[cfg(any(test, feature = "fakes"))]
pub mod fakes;

#[cfg(test)]
mod tests;

pub use backup::{AUTO_PREFIX, BACKUP_EXTENSION, is_auto_name};
pub use entry::{Entry, EntryDraft, EntryPatch, EntryView, ExportCompat, VaultData};
pub use error::{CoreError, CoreResult, ErrorCode};
pub use export::EXPORT_IDLE;
pub use import::{Choice, Outcome};
pub use lockra_vault::KdfCost;
pub use session::{AUTO_BACKUP_DEBOUNCE, Core, CoreConfig, FREE_ATTEMPTS, MAX_IMPORT_BYTES, MIN_PASSWORD_CHARS, Ports, RestoreMode, VAULT_FILE};
