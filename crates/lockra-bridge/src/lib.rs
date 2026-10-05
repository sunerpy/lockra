//! The wire contract between the webview and the core, free of any Tauri dependency.
//!
//! Every command the webview may send is one variant of [`UiCommand`] (tagged by `command`,
//! fields snake_case); [`dispatch`] runs it on a [`Core`] and answers with JSON. The Tauri
//! shell's `lockra_dispatch` forwards to it unchanged. No variant takes a file path: reading or
//! writing files happens only through the shell's own commands, after the user picked the file in
//! a native dialog ([`SHELL_COMMANDS`]; on the phone the photo picker or the camera,
//! [`PHONE_COMMANDS`]), so a compromised webview cannot name one.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use lockra_core::settings::Settings;
use lockra_core::ui::{ExportTarget, JoinSource, StorageSource};
use lockra_core::{Choice, Core, CoreError, EntryDraft, EntryPatch, RestoreMode, StorageConfig};
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
    VaultUnlockDevice {
        /// The words of the Touch ID or Windows Hello prompt, in the interface's language.
        #[serde(default)]
        reason: Option<String>,
    },
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
    /// Ask for Touch ID or Windows Hello before "remember on this device" unlocks (one check now).
    DeviceBiometricEnable {
        /// The words of the prompt.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Stop asking for it.
    DeviceBiometricDisable {
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
    /// Put several entries in one group ("" takes them out of theirs), in one change.
    EntriesSetGroup {
        /// Which.
        ids: Vec<Uuid>,
        /// The group.
        group: String,
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
    /// Fold these groups of the code list ("" for the accounts in no group), unfold the others.
    ViewCollapseGroups {
        /// The groups folded from now on.
        #[serde(default)]
        groups: Vec<String>,
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
    /// Set up sync on a new space at storage of the user's own; answers with the sync key.
    SyncCreate {
        /// Where the space goes, with the credentials.
        storage: StorageConfig,
        /// The master password.
        password: Zeroizing<String>,
        /// This device's name in the space.
        device_name: String,
    },
    /// Join a space: an invitation, or the storage and the sync key. With no vault yet, the
    /// password becomes the new vault's master password.
    SyncJoin {
        /// How.
        source: JoinSource,
        /// This device's master password (the vault's, or the new vault's); it opens the space
        /// too unless `space_password` is given.
        password: Zeroizing<String>,
        /// This device's name in the space.
        device_name: String,
        /// The master password of a device in the space, when it is not `password`.
        #[serde(default)]
        space_password: Option<Zeroizing<String>>,
    },
    /// The invitation for another device; answers with it (a secret). Without a password the
    /// biometric check that unlocks this vault proves the user is there.
    SyncInvite {
        /// The master password.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The words of the biometric prompt.
        #[serde(default)]
        reason: Option<String>,
    },
    /// The user saved or wrote down the sync key: the reminder goes.
    SyncKeyAcknowledge,
    /// New storage settings for the space (an address, new credentials).
    SyncSetStorage {
        /// Where the space is now, with the credentials.
        storage: StorageConfig,
        /// The master password.
        password: Zeroizing<String>,
    },
    /// Rename this device in its space.
    SyncRenameDevice {
        /// The new name.
        name: String,
    },
    /// Remove another device from the space.
    SyncRemoveDevice {
        /// Its tag.
        tag: String,
    },
    /// Sync now.
    SyncNow,
    /// Turn sync off on this device.
    SyncDisable,
    /// Make this computer the space's LAN hub; without a space, one on the LAN alone (the
    /// password required then). With a space, the biometric check may stand for the password.
    SyncLanEnable {
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The words of the biometric prompt.
        #[serde(default)]
        reason: Option<String>,
        /// This device's name in a new space.
        #[serde(default)]
        device_name: String,
    },
    /// An offer for a device to pair with this hub; answers with it (a secret for two minutes).
    SyncLanOffer {
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        #[serde(default)]
        reason: Option<String>,
    },
    /// The user's answer to the device asking to pair.
    SyncLanAnswer { approve: bool },
    /// Take a paired device off this hub.
    SyncLanRemovePeer { peer_id: Uuid },
    /// Stop syncing over the LAN on this device.
    SyncLanDisable,
    /// Pair this device with the hub whose offer is `text` (pasted); the state shows the code to
    /// compare meanwhile. With no vault yet, the password becomes the new vault's master password.
    SyncLanJoin { text: Zeroizing<String>, password: Zeroizing<String>, device_name: String },
    /// Give a space on the LAN alone a storage of the user's own.
    SyncAddStorage {
        source: StorageSource,
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Take the storage of the user's own off the space; the LAN goes on.
    SyncRemoveStorage,
}

/// Every [`UiCommand`] name, in declaration order; the TypeScript schema and the fixtures name
/// exactly this set (checked by the contract test).
pub const COMMANDS: [&str; 53] = [
    "app_state",
    "vault_create",
    "vault_unlock",
    "vault_unlock_device",
    "vault_lock",
    "vault_change_password",
    "vault_reset",
    "device_unlock_enable",
    "device_unlock_disable",
    "device_biometric_enable",
    "device_biometric_disable",
    "entry_add_uri",
    "entry_add_manual",
    "entry_update",
    "entry_delete",
    "entries_set_group",
    "entry_hotp_next",
    "entry_copy",
    "entry_reveal",
    "view_collapse_groups",
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
    "sync_create",
    "sync_join",
    "sync_invite",
    "sync_key_acknowledge",
    "sync_set_storage",
    "sync_rename_device",
    "sync_remove_device",
    "sync_now",
    "sync_disable",
    "sync_lan_enable",
    "sync_lan_offer",
    "sync_lan_answer",
    "sync_lan_remove_peer",
    "sync_lan_disable",
    "sync_lan_join",
    "sync_add_storage",
    "sync_remove_storage",
];

/// The Tauri commands of the desktop shell: the dispatcher, the code stream, the actions that open
/// a native file dialog first, and the system tray's words.
pub const SHELL_COMMANDS: [&str; 10] = [
    "lockra_dispatch",
    "codes_subscribe",
    "codes_unsubscribe",
    "import_pick_files",
    "backup_save",
    "backup_pick_dir",
    "restore_pick",
    "export_otpauth_file",
    "sync_key_save",
    "tray_set",
];

/// The Tauri commands of the phone shell: the dispatcher, the code stream, and the actions that
/// open the photo picker, the file picker or the camera first; what they read stays in Rust.
pub const PHONE_COMMANDS: [&str; 12] = [
    "lockra_dispatch",
    "codes_subscribe",
    "codes_unsubscribe",
    "import_pick_files",
    "import_scan",
    "backup_save",
    "restore_pick",
    "export_otpauth_file",
    "sync_scan_join",
    "sync_scan_pair",
    "sync_key_save",
    "update_open_release",
];

impl UiCommand {
    /// Whether the answer carries a secret (the shell turns screen-capture protection on).
    pub fn shows_secret(&self) -> bool {
        matches!(self, Self::EntryReveal { .. } | Self::ExportStart { .. } | Self::SyncCreate { .. } | Self::SyncInvite { .. } | Self::SyncLanOffer { .. })
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
        UiCommand::VaultUnlockDevice { reason } => unit(core.unlock_with_device(reason).await)?,
        UiCommand::VaultLock => {
            core.lock_vault();
            Value::Null
        }
        UiCommand::VaultChangePassword { current, new } => unit(core.change_password(current, new).await)?,
        UiCommand::VaultReset => unit(core.reset_vault())?,
        UiCommand::DeviceUnlockEnable => unit(core.enable_device_unlock())?,
        UiCommand::DeviceUnlockDisable { password } => unit(core.disable_device_unlock(password).await)?,
        UiCommand::DeviceBiometricEnable { reason } => unit(core.enable_device_biometric(reason).await)?,
        UiCommand::DeviceBiometricDisable { password } => unit(core.disable_device_biometric(password).await)?,
        UiCommand::EntryAddUri { uri } => json!({ "id": core.add_uri(&uri)? }),
        UiCommand::EntryAddManual { draft } => json!({ "id": core.add_manual(draft)? }),
        UiCommand::EntryUpdate { id, patch } => unit(core.update_entry(id, patch))?,
        UiCommand::EntryDelete { id } => unit(core.delete_entry(id))?,
        UiCommand::EntriesSetGroup { ids, group } => unit(core.set_entries_group(&ids, &group))?,
        UiCommand::EntryHotpNext { id } => unit(core.hotp_next(id))?,
        UiCommand::EntryCopy { id } => unit(core.copy_code(id))?,
        UiCommand::EntryReveal { id, password } => json!(core.reveal(id, password).await?),
        UiCommand::ViewCollapseGroups { groups } => unit(core.collapse_groups(groups))?,
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
        UiCommand::SyncCreate { storage, password, device_name } => json!(core.sync_create(storage, password, device_name).await?),
        UiCommand::SyncJoin { source, password, device_name, space_password } => unit(core.sync_join(source, password, device_name, space_password).await)?,
        UiCommand::SyncInvite { password, reason } => json!(core.sync_invite(password, reason).await?),
        UiCommand::SyncKeyAcknowledge => unit(core.sync_key_acknowledge())?,
        UiCommand::SyncSetStorage { storage, password } => unit(core.sync_set_storage(storage, password).await)?,
        UiCommand::SyncRenameDevice { name } => unit(core.sync_rename_device(&name))?,
        UiCommand::SyncRemoveDevice { tag } => unit(core.sync_remove_device(&tag).await)?,
        UiCommand::SyncNow => unit(core.sync_now())?,
        UiCommand::SyncDisable => unit(core.sync_disable())?,
        UiCommand::SyncLanEnable { password, reason, device_name } => unit(core.sync_lan_enable(password, reason, device_name).await)?,
        UiCommand::SyncLanOffer { password, reason } => json!(core.sync_lan_offer(password, reason).await?),
        UiCommand::SyncLanAnswer { approve } => unit(core.sync_lan_answer(approve))?,
        UiCommand::SyncLanRemovePeer { peer_id } => unit(core.sync_lan_remove_peer(peer_id).await)?,
        UiCommand::SyncLanDisable => unit(core.sync_lan_disable())?,
        UiCommand::SyncLanJoin { text, password, device_name } => unit(core.sync_lan_join(text, password, device_name).await)?,
        UiCommand::SyncAddStorage { source, password, reason } => unit(core.sync_add_storage(source, password, reason).await)?,
        UiCommand::SyncRemoveStorage => unit(core.sync_remove_storage())?,
    })
}

fn unit(result: Result<(), CoreError>) -> Result<Value, CoreError> {
    result.map(|()| Value::Null)
}
