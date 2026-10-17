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
use lockra_core::ui::{ExportTarget, JoinSource};
use lockra_core::{
    BrowserPolicy, Choice, CopyPart, Core, CoreError, EntryDraft, EntryPatch, GeneratorOptions, ItemDraft, ItemPatch, PinnedGroup, RestoreMode, RevealParts,
    SiteRuleKind, StorageConfig,
};
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
    /// Add the one account the clipboard holds (a QR code's image, or an `otpauth` link), at once:
    /// a login's form links it.
    EntryAddClipboard,
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
    /// Show the secret, its URI and its QR code. Without a password the biometric check that
    /// unlocks this vault proves the user is there.
    EntryReveal {
        /// Which.
        id: Uuid,
        /// The master password.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The biometric prompt's words, without a password.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Fold these groups of the code list ("" for the accounts in no group), unfold the others.
    ViewCollapseGroups {
        /// The groups folded from now on.
        #[serde(default)]
        groups: Vec<String>,
    },
    /// The accounts in the order they were dragged into; the code order becomes manual.
    ViewOrderEntries {
        /// Every account, in order.
        #[serde(default)]
        ids: Vec<Uuid>,
    },
    /// The groups in the order they were dragged into ("" for the accounts in no group).
    ViewOrderGroups {
        /// Every group, in order.
        #[serde(default)]
        groups: Vec<String>,
    },
    /// Pin a group at the top of the code list, or none.
    ViewPinGroup {
        /// The pinned accounts, a group or the accounts in no group; none unpins.
        #[serde(default)]
        pinned: Option<PinnedGroup>,
    },
    /// Import pasted or typed text.
    ImportText {
        /// The text.
        text: Zeroizing<String>,
    },
    /// Import the clipboard.
    ImportClipboard,
    /// Open the file waiting in the import: a Lockra backup, or a KeePass database (with the key
    /// file chosen through the shell's `import_pick_key_file`).
    ImportBackupPassword {
        /// Its password.
        password: Zeroizing<String>,
    },
    /// Forget the key file chosen for the KeePass database waiting in the import.
    ImportKeyFileClear,
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
        /// The master password; without it the biometric check that unlocks this vault.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The biometric prompt's words, without a password.
        #[serde(default)]
        reason: Option<String>,
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
    /// password becomes the new vault's master password; an unlocked vault may prove presence by
    /// the biometric check that unlocks it instead.
    SyncJoin {
        /// How.
        source: JoinSource,
        /// This device's master password (the vault's, or the new vault's); it opens the space
        /// too unless `space_password` is given.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// This device's name in the space.
        device_name: String,
        /// The master password of a device in the space, when it is not `password`.
        #[serde(default)]
        space_password: Option<Zeroizing<String>>,
        /// The biometric prompt's words, without a password.
        #[serde(default)]
        reason: Option<String>,
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
    /// Put the sealed invitation (the text to send, `lockra-invite:2:…`) on the clipboard, for a
    /// device that scans nothing; it is cleared after the clipboard time.
    SyncInviteCopy {
        /// The sealed text `sync_invite` answered with.
        text: Zeroizing<String>,
    },
    /// The user saved or wrote down the sync key: the reminder goes.
    SyncKeyAcknowledge,
    /// The sync key (the recovery key); answers with it (a secret). Without a password the
    /// biometric check that unlocks this vault proves the user is there.
    SyncKeyReveal {
        /// The master password.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The words of the biometric prompt.
        #[serde(default)]
        reason: Option<String>,
    },
    /// New storage settings for the space (an address, new credentials).
    SyncSetStorage {
        /// Where the space is now, with the credentials.
        storage: StorageConfig,
        /// The master password; without it the biometric check that unlocks this vault.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The biometric prompt's words, without a password.
        #[serde(default)]
        reason: Option<String>,
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
    /// Every login, without its secrets.
    ItemsList,
    /// Add a login; answers with its id.
    ItemAdd {
        /// The login.
        draft: ItemDraft,
    },
    /// Change a login.
    ItemUpdate {
        /// Which.
        id: Uuid,
        /// What changes.
        patch: ItemPatch,
    },
    /// Delete logins.
    ItemDelete {
        /// Which.
        ids: Vec<Uuid>,
    },
    /// Show a login's secrets (a secret answer). A login of the `confirm` level needs the master
    /// password, or without it the biometric check that unlocks this vault.
    ItemReveal {
        /// Which.
        id: Uuid,
        /// What to show.
        parts: RevealParts,
        /// The master password.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The biometric prompt's words, without a password.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Put a part of a login on the clipboard (Rust writes it, and clears it).
    ItemCopy {
        /// Which.
        id: Uuid,
        /// What.
        part: CopyPart,
        /// The master password, for a secret of a `confirm` login.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The biometric prompt's words, without a password.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Make a password (a secret answer); the core keeps it for `generator_copy` and a login saved
    /// with it.
    GeneratorGenerate {
        /// What to make.
        options: GeneratorOptions,
        /// The site it is for, whose rules it keeps to when Lockra knows them.
        #[serde(default)]
        domain: Option<String>,
    },
    /// Put the generator's last password on the clipboard, and into its history.
    GeneratorCopy,
    /// The generator's history (a secret answer), after the master password or the biometric
    /// check.
    GeneratorHistoryReveal {
        /// The master password.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The biometric prompt's words, without a password.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Forget the generator's history.
    GeneratorHistoryClear,
    /// How strong a password is.
    PasswordScore {
        /// The password.
        password: Zeroizing<String>,
    },
    /// Add a site rule.
    SiteRuleAdd {
        /// Never save, never fill, or equivalent domains.
        kind: SiteRuleKind,
        /// The domains.
        domains: Vec<String>,
    },
    /// Remove a site rule.
    SiteRuleRemove {
        /// Which.
        id: Uuid,
    },
    /// Fold these groups of the password list ("" for the logins in no group), unfold the others.
    ViewCollapseItems {
        /// The groups folded from now on.
        #[serde(default)]
        groups: Vec<String>,
    },
    /// Approve a browser extension's pairing (the user compared the codes), with the master
    /// password or the biometric check.
    BrowserPairApprove {
        /// The request.
        id: Uuid,
        /// The master password.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The biometric prompt's words, without a password.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Refuse a pairing.
    BrowserPairDeny {
        /// The request.
        id: Uuid,
    },
    /// Let a waiting fill (or a copy from the popup) go ahead: with the master password or the
    /// biometric check where the login asks for it.
    BrowserFillConfirm {
        /// The request.
        id: Uuid,
        /// The master password.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The biometric prompt's words, without a password.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Refuse a waiting fill or copy.
    BrowserFillDeny {
        /// The request.
        id: Uuid,
    },
    /// Remove a pairing: its sessions end, the extension has to pair again.
    BrowserUnpair {
        /// The pairing.
        id: Uuid,
    },
    /// The paused extensions get answers again.
    BrowserResume,
    /// Empty the activity list.
    BrowserActivityClear,
    /// Change what the extensions may do; letting them do more asks for the master password or
    /// the biometric check.
    BrowserSetPolicy {
        /// The new policy.
        policy: BrowserPolicy,
        /// The master password.
        #[serde(default)]
        password: Option<Zeroizing<String>>,
        /// The biometric prompt's words, without a password.
        #[serde(default)]
        reason: Option<String>,
    },
}

/// Every [`UiCommand`] name, in declaration order; the TypeScript schema and the fixtures name
/// exactly this set (checked by the contract test).
pub const COMMANDS: [&str; 74] = [
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
    "entry_add_clipboard",
    "entry_update",
    "entry_delete",
    "entries_set_group",
    "entry_hotp_next",
    "entry_copy",
    "entry_reveal",
    "view_collapse_groups",
    "view_order_entries",
    "view_order_groups",
    "view_pin_group",
    "import_text",
    "import_clipboard",
    "import_backup_password",
    "import_key_file_clear",
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
    "sync_invite_copy",
    "sync_key_acknowledge",
    "sync_key_reveal",
    "sync_set_storage",
    "sync_rename_device",
    "sync_remove_device",
    "sync_now",
    "sync_disable",
    "items_list",
    "item_add",
    "item_update",
    "item_delete",
    "item_reveal",
    "item_copy",
    "generator_generate",
    "generator_copy",
    "generator_history_reveal",
    "generator_history_clear",
    "password_score",
    "site_rule_add",
    "site_rule_remove",
    "view_collapse_items",
    "browser_pair_approve",
    "browser_pair_deny",
    "browser_fill_confirm",
    "browser_fill_deny",
    "browser_unpair",
    "browser_resume",
    "browser_activity_clear",
    "browser_set_policy",
];

/// The Tauri commands of the desktop shell: the dispatcher, the code stream, the actions that open a
/// native file dialog first (`entry_add_image` among them: the one account an image's QR code
/// holds, for a login's form to link), the browser integration's switch and the extension's folder
/// (its status, its path on the clipboard, the file manager showing it), and the end of a quick
/// search (the window goes back where it was).
pub const SHELL_COMMANDS: [&str; 20] = [
    "lockra_dispatch",
    "codes_subscribe",
    "codes_unsubscribe",
    "import_pick_files",
    "import_pick_key_file",
    "entry_add_image",
    "backup_save",
    "backup_pick_dir",
    "restore_pick",
    "export_otpauth_file",
    "export_items_file",
    "sync_key_save",
    "sync_pick_folder",
    "browser_enable",
    "browser_disable",
    "browser_status",
    "browser_extension",
    "browser_extension_copy",
    "browser_extension_reveal",
    "quick_access_done",
];

/// The Tauri commands of the phone shell: the dispatcher, the code stream, and the actions that
/// open the photo picker, the file picker or the camera first; what they read stays in Rust.
pub const PHONE_COMMANDS: [&str; 13] = [
    "lockra_dispatch",
    "codes_subscribe",
    "codes_unsubscribe",
    "import_pick_files",
    "import_pick_key_file",
    "import_scan",
    "backup_save",
    "restore_pick",
    "export_otpauth_file",
    "export_items_file",
    "sync_scan_join",
    "sync_key_save",
    "update_open_release",
];

impl UiCommand {
    /// Whether the answer carries a secret (the shell turns screen-capture protection on).
    pub fn shows_secret(&self) -> bool {
        matches!(
            self,
            Self::EntryReveal { .. }
                | Self::ExportStart { .. }
                | Self::SyncInvite { .. }
                | Self::SyncKeyReveal { .. }
                | Self::ItemReveal { .. }
                | Self::GeneratorGenerate { .. }
                | Self::GeneratorHistoryReveal { .. }
        )
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
        UiCommand::EntryAddClipboard => json!({ "id": core.add_from_clipboard().await? }),
        UiCommand::EntryUpdate { id, patch } => unit(core.update_entry(id, patch))?,
        UiCommand::EntryDelete { id } => unit(core.delete_entry(id))?,
        UiCommand::EntriesSetGroup { ids, group } => unit(core.set_entries_group(&ids, &group))?,
        UiCommand::EntryHotpNext { id } => unit(core.hotp_next(id))?,
        UiCommand::EntryCopy { id } => unit(core.copy_code(id))?,
        UiCommand::EntryReveal { id, password, reason } => json!(core.reveal(id, password, reason).await?),
        UiCommand::ViewCollapseGroups { groups } => unit(core.collapse_groups(groups))?,
        UiCommand::ViewOrderEntries { ids } => unit(core.order_entries(ids))?,
        UiCommand::ViewOrderGroups { groups } => unit(core.order_groups(groups))?,
        UiCommand::ViewPinGroup { pinned } => unit(core.pin_group(pinned))?,
        UiCommand::ImportText { text } => unit(core.import_text(&text))?,
        UiCommand::ImportClipboard => unit(core.import_clipboard().await)?,
        UiCommand::ImportBackupPassword { password } => unit(core.import_backup_password(password).await)?,
        UiCommand::ImportKeyFileClear => unit(core.import_key_file_clear())?,
        UiCommand::ImportCommit { choices } => json!(core.import_commit(&choices)?),
        UiCommand::ImportCancel => {
            core.import_cancel();
            Value::Null
        }
        UiCommand::ExportStart { target, entry_ids, password, reason } => json!(core.export_start(target, &entry_ids, password, reason).await?),
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
        // The key is not shown here: Settings › Sync reminds of it, and shows it on `sync_key_reveal`.
        UiCommand::SyncCreate { storage, password, device_name } => unit(core.sync_create(storage, password, device_name).await.map(|_| ()))?,
        UiCommand::SyncJoin { source, password, device_name, space_password, reason } => {
            unit(core.sync_join(source, password, device_name, space_password, reason).await)?
        }
        UiCommand::SyncInvite { password, reason } => json!(core.sync_invite(password, reason).await?),
        UiCommand::SyncInviteCopy { text } => unit(core.sync_invite_copy(&text))?,
        UiCommand::SyncKeyAcknowledge => unit(core.sync_key_acknowledge())?,
        UiCommand::SyncKeyReveal { password, reason } => json!(core.sync_key_reveal(password, reason).await?),
        UiCommand::SyncSetStorage { storage, password, reason } => unit(core.sync_set_storage(storage, password, reason).await)?,
        UiCommand::SyncRenameDevice { name } => unit(core.sync_rename_device(&name))?,
        UiCommand::SyncRemoveDevice { tag } => unit(core.sync_remove_device(&tag).await)?,
        UiCommand::SyncNow => unit(core.sync_now())?,
        UiCommand::SyncDisable => unit(core.sync_disable())?,
        UiCommand::ItemsList => json!(core.items_list()?),
        UiCommand::ItemAdd { draft } => json!({ "id": core.item_add(draft)? }),
        UiCommand::ItemUpdate { id, patch } => unit(core.item_update(id, patch))?,
        UiCommand::ItemDelete { ids } => unit(core.item_delete(&ids))?,
        UiCommand::ItemReveal { id, parts, password, reason } => json!(core.item_reveal(id, parts, password, reason).await?),
        UiCommand::ItemCopy { id, part, password, reason } => unit(core.item_copy(id, part, password, reason).await)?,
        UiCommand::GeneratorGenerate { options, domain } => json!(core.generator_generate(&options, domain)?),
        UiCommand::GeneratorCopy => unit(core.generator_copy())?,
        UiCommand::GeneratorHistoryReveal { password, reason } => json!(core.generator_history_reveal(password, reason).await?),
        UiCommand::GeneratorHistoryClear => unit(core.generator_history_clear())?,
        UiCommand::PasswordScore { password } => json!(core.password_score(&password)?),
        UiCommand::SiteRuleAdd { kind, domains } => unit(core.site_rule_add(kind, &domains))?,
        UiCommand::SiteRuleRemove { id } => unit(core.site_rule_remove(id))?,
        UiCommand::ViewCollapseItems { groups } => unit(core.collapse_item_groups(groups))?,
        UiCommand::BrowserPairApprove { id, password, reason } => unit(core.browser_pair_approve(id, password, reason).await)?,
        UiCommand::BrowserPairDeny { id } => unit(core.browser_pair_deny(id))?,
        UiCommand::BrowserFillConfirm { id, password, reason } => unit(core.browser_confirm(id, password, reason).await)?,
        UiCommand::BrowserFillDeny { id } => unit(core.browser_confirm_deny(id))?,
        UiCommand::BrowserUnpair { id } => unit(core.browser_unpair(id))?,
        UiCommand::BrowserResume => unit(core.browser_resume())?,
        UiCommand::BrowserActivityClear => unit(core.browser_activity_clear())?,
        UiCommand::BrowserSetPolicy { policy, password, reason } => unit(core.browser_set_policy(policy, password, reason).await)?,
    })
}

fn unit(result: Result<(), CoreError>) -> Result<Value, CoreError> {
    result.map(|()| Value::Null)
}
