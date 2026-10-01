//! The wire contract between the webview and the core, free of any Tauri dependency.
//!
//! Every command the webview may send is one variant of [`UiCommand`] (tagged by `command`,
//! fields snake_case); [`dispatch`] runs it on a [`Core`] and answers with JSON. The Tauri
//! shell's `lockra_dispatch` forwards to it unchanged. No variant takes a file path: reading or
//! writing files happens only through the shell's own commands, after the user picked the file in
//! a native dialog ([`SHELL_COMMANDS`]), so a compromised webview cannot name one.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use lockra_core::settings::Settings;
use lockra_core::ui::ExportTarget;
use lockra_core::{Choice, Core, CoreError, EntryDraft, EntryPatch, RestoreMode};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
use zeroize::Zeroizing;

/// One webview command.
#[derive(Debug, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiCommand {
    /// The state to render.
    AppState,
    /// Create the vault.
    VaultCreate {
        /// The master password.
        password: Zeroizing<String>,
    },
    /// Unlock with the master password.
    VaultUnlock {
        /// The master password.
        password: Zeroizing<String>,
    },
    /// Unlock with the keychain key.
    VaultUnlockDevice,
    /// Lock.
    VaultLock,
    /// Change the master password.
    VaultChangePassword {
        /// The current password.
        current: Zeroizing<String>,
        /// The new one.
        new: Zeroizing<String>,
    },
    /// Move a locked vault aside and start over.
    VaultReset,
    /// Turn "remember on this device" on.
    DeviceUnlockEnable,
    /// Turn it off.
    DeviceUnlockDisable {
        /// The master password.
        password: Zeroizing<String>,
    },
    /// Add from an `otpauth://` URI.
    EntryAddUri {
        /// The URI.
        uri: Zeroizing<String>,
    },
    /// Add by hand.
    EntryAddManual {
        /// The fields.
        draft: EntryDraft,
    },
    /// Rename, regroup, pin.
    EntryUpdate {
        /// Which.
        id: Uuid,
        /// What changes.
        patch: EntryPatch,
    },
    /// Delete.
    EntryDelete {
        /// Which.
        id: Uuid,
    },
    /// HOTP: next code.
    EntryHotpNext {
        /// Which.
        id: Uuid,
    },
    /// Copy the current code.
    EntryCopy {
        /// Which.
        id: Uuid,
    },
    /// Show the secret, its URI and its QR code.
    EntryReveal {
        /// Which.
        id: Uuid,
        /// The master password.
        password: Zeroizing<String>,
    },
    /// Import pasted or typed text.
    ImportText {
        /// The text.
        text: Zeroizing<String>,
    },
    /// Import the clipboard.
    ImportClipboard,
    /// Open the Lockra backup waiting in the import.
    ImportBackupPassword {
        /// Its password.
        password: Zeroizing<String>,
    },
    /// Apply the import.
    ImportCommit {
        /// Per-candidate choices; the rest take their default.
        #[serde(default)]
        choices: Vec<Choice>,
    },
    /// Drop the import.
    ImportCancel,
    /// Build an export's QR codes.
    ExportStart {
        /// Google or Microsoft.
        target: ExportTarget,
        /// Which entries.
        entry_ids: Vec<Uuid>,
        /// The master password.
        password: Zeroizing<String>,
    },
    /// One code of an export.
    ExportPage {
        /// The session.
        session: Uuid,
        /// Zero-based page.
        index: u32,
    },
    /// Close an export.
    ExportClose {
        /// The session.
        session: Uuid,
    },
    /// A secret view (export codes, a revealed secret) closed: the shell lifts screen-capture
    /// protection. Nothing changes in the core.
    SecretViewClosed,
    /// Run the automatic backup now.
    BackupAutoNow,
    /// Restore the backup opened through the shell's picker.
    RestoreCommit {
        /// Its password.
        password: Zeroizing<String>,
        /// Merge or replace (ignored before a vault exists).
        mode: RestoreMode,
    },
    /// Forget the opened backup.
    RestoreCancel,
    /// Save settings.
    SettingsSet {
        /// All of them.
        settings: Settings,
    },
    /// The user did something.
    Activity,
    /// Look for a newer release (the answer arrives in the state).
    UpdateCheck,
    /// Download, verify and install the newest release, then restart.
    UpdateInstall,
}

/// Every [`UiCommand`] name, in declaration order; the TypeScript schema and the fixtures name
/// exactly this set (checked by the contract test).
pub const COMMANDS: [&str; 32] = [
    "app_state",
    "vault_create",
    "vault_unlock",
    "vault_unlock_device",
    "vault_lock",
    "vault_change_password",
    "vault_reset",
    "device_unlock_enable",
    "device_unlock_disable",
    "entry_add_uri",
    "entry_add_manual",
    "entry_update",
    "entry_delete",
    "entry_hotp_next",
    "entry_copy",
    "entry_reveal",
    "import_text",
    "import_clipboard",
    "import_backup_password",
    "import_commit",
    "import_cancel",
    "export_start",
    "export_page",
    "export_close",
    "secret_view_closed",
    "backup_auto_now",
    "restore_commit",
    "restore_cancel",
    "settings_set",
    "activity",
    "update_check",
    "update_install",
];

/// The Tauri commands of the desktop shell: the dispatcher, the code stream, and the actions that
/// open a native file dialog first.
pub const SHELL_COMMANDS: [&str; 8] =
    ["lockra_dispatch", "codes_subscribe", "codes_unsubscribe", "import_pick_files", "backup_save", "backup_pick_dir", "restore_pick", "export_otpauth_file"];

impl UiCommand {
    /// Whether the answer carries a secret (the shell turns screen-capture protection on).
    pub fn shows_secret(&self) -> bool {
        matches!(self, Self::EntryReveal { .. } | Self::ExportStart { .. })
    }

    /// Whether the command ends every secret view (the shell turns the protection off).
    pub fn hides_secret(&self) -> bool {
        matches!(self, Self::SecretViewClosed | Self::ExportClose { .. } | Self::VaultLock)
    }
}

/// Run `command` on `core`; the answer is JSON (`null` for commands that return nothing).
pub async fn dispatch(core: &Core, command: UiCommand) -> Result<Value, CoreError> {
    Ok(match command {
        UiCommand::AppState => json!(core.state()),
        UiCommand::VaultCreate { password } => unit(core.create_vault(password).await)?,
        UiCommand::VaultUnlock { password } => unit(core.unlock(password).await)?,
        UiCommand::VaultUnlockDevice => unit(core.unlock_with_device().await)?,
        UiCommand::VaultLock => {
            core.lock_vault();
            Value::Null
        }
        UiCommand::VaultChangePassword { current, new } => unit(core.change_password(current, new).await)?,
        UiCommand::VaultReset => unit(core.reset_vault())?,
        UiCommand::DeviceUnlockEnable => unit(core.enable_device_unlock())?,
        UiCommand::DeviceUnlockDisable { password } => unit(core.disable_device_unlock(password).await)?,
        UiCommand::EntryAddUri { uri } => json!({ "id": core.add_uri(&uri)? }),
        UiCommand::EntryAddManual { draft } => json!({ "id": core.add_manual(draft)? }),
        UiCommand::EntryUpdate { id, patch } => unit(core.update_entry(id, patch))?,
        UiCommand::EntryDelete { id } => unit(core.delete_entry(id))?,
        UiCommand::EntryHotpNext { id } => unit(core.hotp_next(id))?,
        UiCommand::EntryCopy { id } => unit(core.copy_code(id))?,
        UiCommand::EntryReveal { id, password } => json!(core.reveal(id, password).await?),
        UiCommand::ImportText { text } => unit(core.import_text(&text))?,
        UiCommand::ImportClipboard => unit(core.import_clipboard().await)?,
        UiCommand::ImportBackupPassword { password } => unit(core.import_backup_password(password).await)?,
        UiCommand::ImportCommit { choices } => json!(core.import_commit(&choices)?),
        UiCommand::ImportCancel => {
            core.import_cancel();
            Value::Null
        }
        UiCommand::ExportStart { target, entry_ids, password } => json!(core.export_start(target, &entry_ids, password).await?),
        UiCommand::ExportPage { session, index } => json!(core.export_page(session, index)?),
        UiCommand::ExportClose { session } => {
            core.export_close(session);
            Value::Null
        }
        UiCommand::SecretViewClosed => Value::Null,
        UiCommand::BackupAutoNow => unit(core.backup_auto_now())?,
        UiCommand::RestoreCommit { password, mode } => unit(core.restore_commit(password, mode).await)?,
        UiCommand::RestoreCancel => {
            core.restore_cancel();
            Value::Null
        }
        UiCommand::SettingsSet { settings } => unit(core.set_settings(settings))?,
        UiCommand::Activity => {
            core.activity();
            Value::Null
        }
        UiCommand::UpdateCheck => unit(core.update_check())?,
        UiCommand::UpdateInstall => unit(core.update_install())?,
    })
}

fn unit(result: Result<(), CoreError>) -> Result<Value, CoreError> {
    result.map(|()| Value::Null)
}
