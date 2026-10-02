// The IPC contract: zod schemas for every value the Rust core sends and every command the webview
// sends. The Rust types (lockra-core `ui.rs`, lockra-bridge `UiCommand`) are the source of truth;
// `ipc-contract.test.ts` parses the fixtures they write (packages/shared/src/fixtures/ipc/) with
// these schemas, so the two sides cannot drift apart unnoticed.
import { z } from "zod";

// ---- settings --------------------------------------------------------------------------------

export const THEME_IDS = ["light", "dark", "warm", "graphite"] as const;
export const themeIdSchema = z.enum(THEME_IDS);
export type ThemeId = z.infer<typeof themeIdSchema>;

export const ACCENT_IDS = [
  "default",
  "blue",
  "green",
  "yellow",
  "pink",
  "orange",
  "purple",
  "ink",
] as const;
export const accentIdSchema = z.enum(ACCENT_IDS);
export type AccentId = z.infer<typeof accentIdSchema>;

export const DENSITIES = ["default", "compact"] as const;
export const densitySchema = z.enum(DENSITIES);
export type Density = z.infer<typeof densitySchema>;

export const LOCALE_SETTINGS = ["system", "zh-cn", "en"] as const;
export const localeSettingSchema = z.enum(LOCALE_SETTINGS);
export type LocaleSetting = z.infer<typeof localeSettingSchema>;

export const SORT_ORDERS = ["name", "added", "recent"] as const;
export const sortOrderSchema = z.enum(SORT_ORDERS);
export type SortOrder = z.infer<typeof sortOrderSchema>;

/** The choices of Settings › Security (lockra-core `settings.rs`). */
export const AUTO_LOCK_CHOICES = [0, 1, 2, 5, 10, 15, 30, 60] as const;
export const CLIPBOARD_CHOICES = [0, 10, 20, 30, 60, 90] as const;
export const KEEP_CHOICES = [3, 5, 10, 20, 50] as const;
export const FONT_SIZE_MIN = 12;
export const FONT_SIZE_MAX = 18;

export const autoBackupSchema = z.object({
  enabled: z.boolean(),
  dir: z.string().nullable(),
  keep: z.number().int().min(1),
});
export type AutoBackup = z.infer<typeof autoBackupSchema>;

export const settingsSchema = z.object({
  theme: themeIdSchema,
  follow_system_theme: z.boolean(),
  accent: accentIdSchema,
  density: densitySchema,
  font_size_px: z.number().int(),
  reduce_motion: z.boolean(),
  locale: localeSettingSchema,
  auto_lock_minutes: z.number().int().nonnegative(),
  clipboard_clear_seconds: z.number().int().nonnegative(),
  hide_codes: z.boolean(),
  sort: sortOrderSchema,
  auto_backup: autoBackupSchema,
  auto_update: z.boolean(),
});
export type Settings = z.infer<typeof settingsSchema>;

/** The defaults of lockra-core `Settings::default()` (the mock backend and the tests start here). */
export function defaultSettings(): Settings {
  return {
    theme: "light",
    follow_system_theme: true,
    accent: "default",
    density: "default",
    font_size_px: 14,
    reduce_motion: false,
    locale: "system",
    auto_lock_minutes: 5,
    clipboard_clear_seconds: 30,
    hide_codes: false,
    sort: "name",
    auto_backup: { enabled: false, dir: null, keep: 10 },
    auto_update: false,
  };
}

// ---- entries ---------------------------------------------------------------------------------

export const PLATFORMS = ["windows", "macos", "linux"] as const;
export const platformSchema = z.enum(PLATFORMS);
export type Platform = z.infer<typeof platformSchema>;

export const PHASES = ["no_vault", "locked", "unlocked"] as const;
export const phaseSchema = z.enum(PHASES);
export type Phase = z.infer<typeof phaseSchema>;

export const ALGORITHMS = ["sha1", "sha256", "sha512"] as const;
export const algorithmSchema = z.enum(ALGORITHMS);
export type Algorithm = z.infer<typeof algorithmSchema>;

export const otpKindSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("totp"), period: z.number().int().min(1) }),
  z.object({ type: z.literal("hotp"), counter: z.number().int().nonnegative() }),
]);
export type OtpKind = z.infer<typeof otpKindSchema>;

export const ORIGINS = ["manual", "uri", "google", "microsoft", "backup"] as const;
export const originSchema = z.enum(ORIGINS);
export type Origin = z.infer<typeof originSchema>;

export const INCOMPATIBLE = [
  "period_not_30",
  "digits_not_6_or_8",
  "digits_not_6",
  "algorithm_not_sha1",
  "hotp_not_supported",
  "too_large",
] as const;
export const incompatibleSchema = z.enum(INCOMPATIBLE);
export type Incompatible = z.infer<typeof incompatibleSchema>;

export const REJECT_REASONS = [
  "not_otpauth",
  "md5_algorithm",
  "unknown_algorithm",
  "unknown_type",
  "unsupported_digits",
  "invalid_period",
  "invalid_counter",
  "empty_secret",
  "invalid_secret",
  "encrypted_secret",
  "unsupported_account_type",
] as const;
export const rejectReasonSchema = z.enum(REJECT_REASONS);
export type RejectReason = z.infer<typeof rejectReasonSchema>;

const idSchema = z.string().min(1);
const msSchema = z.number().int().nonnegative();

export const entryViewSchema = z.object({
  id: idSchema,
  issuer: z.string(),
  account: z.string(),
  kind: otpKindSchema,
  algorithm: algorithmSchema,
  digits: z.number().int().min(6).max(8),
  group: z.string().nullable(),
  favorite: z.boolean(),
  origin: originSchema,
  created_at_ms: msSchema,
  updated_at_ms: msSchema,
  last_used_at_ms: msSchema.nullable(),
  export: z.object({
    google: incompatibleSchema.nullable(),
    microsoft: incompatibleSchema.nullable(),
  }),
});
export type EntryView = z.infer<typeof entryViewSchema>;

// ---- import, backup, restore -----------------------------------------------------------------

export const importSourceSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("file"), name: z.string() }),
  z.object({ type: z.literal("clipboard") }),
  z.object({ type: z.literal("text") }),
]);
export type ImportSource = z.infer<typeof importSourceSchema>;

export const candidateStatusSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("new") }),
  z.object({ type: z.literal("exists"), entry_id: idSchema }),
  z.object({ type: z.literal("conflict"), entry_id: idSchema }),
  z.object({ type: z.literal("duplicate") }),
  z.object({ type: z.literal("unsupported"), reason: rejectReasonSchema }),
]);
export type CandidateStatus = z.infer<typeof candidateStatusSchema>;

export const CANDIDATE_ACTIONS = ["add", "skip", "replace"] as const;
export const candidateActionSchema = z.enum(CANDIDATE_ACTIONS);
export type CandidateAction = z.infer<typeof candidateActionSchema>;

export const candidateViewSchema = z.object({
  id: z.number().int().nonnegative(),
  source: importSourceSchema,
  origin: originSchema,
  issuer: z.string(),
  account: z.string(),
  kind: otpKindSchema.nullable(),
  algorithm: algorithmSchema.nullable(),
  digits: z.number().int().nullable(),
  line: z.number().int().nullable(),
  status: candidateStatusSchema,
  default_action: candidateActionSchema,
});
export type CandidateView = z.infer<typeof candidateViewSchema>;

export const googleBatchViewSchema = z.object({
  id: z.number().int(),
  size: z.number().int(),
  received: z.array(z.number().int()),
  missing: z.array(z.number().int()),
});
export type GoogleBatchView = z.infer<typeof googleBatchViewSchema>;

export const importViewSchema = z.object({
  candidates: z.array(candidateViewSchema),
  google_batches: z.array(googleBatchViewSchema),
  awaiting_password: z.string().nullable(),
});
export type ImportView = z.infer<typeof importViewSchema>;

export const ERROR_CODES = [
  "no_vault",
  "vault_exists",
  "locked",
  "wrong_password",
  "rate_limited",
  "password_too_short",
  "vault_corrupted",
  "vault_unsupported",
  "not_lockra",
  "keychain_unavailable",
  "keychain_failed",
  "device_key_missing",
  "device_key_stale",
  "device_unlock_off",
  "entry_not_found",
  "duplicate_entry",
  "invalid_uri",
  "invalid_secret",
  "invalid_parameters",
  "no_import",
  "import_empty",
  "import_unrecognized",
  "import_unreadable",
  "clipboard_empty",
  "clipboard_failed",
  "export_expired",
  "export_nothing",
  "no_restore",
  "backup_dir_missing",
  "backup_dir_unavailable",
  "io_failed",
  "update_unavailable",
  "update_busy",
  "update_network",
  "update_invalid",
  "update_signature",
  "update_install_failed",
  "update_cancelled",
  "sync_off",
  "sync_already_on",
  "sync_config_invalid",
  "sync_insecure",
  "sync_network",
  "sync_denied",
  "sync_storage_failed",
  "sync_space_not_found",
  "sync_wrong_credentials",
  "sync_key_invalid",
  "sync_invite_invalid",
  "sync_data_corrupted",
  "sync_unsupported",
  "internal",
] as const;
export const errorCodeSchema = z.enum(ERROR_CODES);
export type ErrorCode = z.infer<typeof errorCodeSchema>;

export const backupViewSchema = z.object({
  last_backup_ms: msSchema.nullable(),
  last_auto_file: z.string().nullable(),
  last_auto_error: z.object({ code: errorCodeSchema, at_ms: msSchema }).nullable(),
});
export type BackupView = z.infer<typeof backupViewSchema>;

export const FILE_KINDS = ["vault", "backup"] as const;
export const restoreViewSchema = z.object({
  file_name: z.string(),
  kind: z.enum(FILE_KINDS),
  created_at_ms: msSchema,
});
export type RestoreView = z.infer<typeof restoreViewSchema>;

// ---- the in-app update -----------------------------------------------------------------------

/** How this copy was installed, which is how an update installs (lockra-core `InstallMethod`). */
export const INSTALL_METHODS = ["deb", "rpm", "appimage", "nsis", "msi", "app"] as const;
export const installMethodSchema = z.enum(INSTALL_METHODS);
export type InstallMethod = z.infer<typeof installMethodSchema>;

export const updateStatusSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("idle") }),
  z.object({ state: z.literal("checking") }),
  z.object({ state: z.literal("up_to_date"), checked_at_ms: msSchema }),
  z.object({
    state: z.literal("available"),
    version: z.string(),
    notes: z.string().nullable(),
    date: z.string().nullable(),
    checked_at_ms: msSchema,
  }),
  z.object({
    state: z.literal("downloading"),
    version: z.string(),
    received: z.number().int().nonnegative(),
    total: z.number().int().nonnegative().nullable(),
  }),
  z.object({ state: z.literal("ready"), version: z.string() }),
  z.object({ state: z.literal("installing"), version: z.string() }),
  z.object({ state: z.literal("failed"), code: errorCodeSchema, at_ms: msSchema }),
]);
export type UpdateStatus = z.infer<typeof updateStatusSchema>;

export const updateViewSchema = z.object({
  /** Absent when this copy cannot update itself. */
  method: installMethodSchema.nullable(),
  status: updateStatusSchema,
});
export type UpdateView = z.infer<typeof updateViewSchema>;

// ---- multi-device sync -----------------------------------------------------------------------

/** Where a sync space is stored, credentials included: what the webview sends (lockra-sync
 *  `StorageConfig`). The core never sends the secret back. */
export const storageConfigSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("s3"),
    endpoint: z.string(),
    region: z.string(),
    bucket: z.string(),
    prefix: z.string(),
    access_key_id: z.string(),
    secret_access_key: z.string(),
    path_style: z.boolean(),
  }),
  z.object({
    kind: z.literal("webdav"),
    url: z.string(),
    prefix: z.string(),
    username: z.string(),
    password: z.string(),
  }),
]);
export type StorageConfig = z.infer<typeof storageConfigSchema>;
export type StorageKind = StorageConfig["kind"];

/** The storage as the core shows it: everything but the secret (lockra-core `StorageView`). */
export const storageViewSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("s3"),
    endpoint: z.string(),
    region: z.string(),
    bucket: z.string(),
    prefix: z.string(),
    access_key_id: z.string(),
    path_style: z.boolean(),
  }),
  z.object({
    kind: z.literal("webdav"),
    url: z.string(),
    prefix: z.string(),
    username: z.string(),
  }),
]);
export type StorageView = z.infer<typeof storageViewSchema>;

export const syncDeviceViewSchema = z.object({
  tag: z.string(),
  name: z.string(),
  written_at_ms: msSchema.nullable(),
  this_device: z.boolean(),
});
export type SyncDeviceView = z.infer<typeof syncDeviceViewSchema>;

export const syncStatusSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("idle") }),
  z.object({ state: z.literal("syncing") }),
  z.object({ state: z.literal("synced"), at_ms: msSchema }),
  z.object({ state: z.literal("failed"), code: errorCodeSchema, at_ms: msSchema }),
]);
export type SyncStatus = z.infer<typeof syncStatusSchema>;

export const syncSpaceViewSchema = z.object({
  storage: storageViewSchema,
  device_name: z.string(),
  devices: z.array(syncDeviceViewSchema),
  status: syncStatusSchema,
  last_sync_ms: msSchema.nullable(),
  rolled_back: z.array(z.string()),
  unreadable: z.array(z.string()),
  keyring_pending: z.boolean(),
});
export type SyncSpaceView = z.infer<typeof syncSpaceViewSchema>;

export const syncViewSchema = z.object({
  /** The space this device belongs to; absent when sync is off and while locked. */
  space: syncSpaceViewSchema.nullable(),
});
export type SyncView = z.infer<typeof syncViewSchema>;

/** How a device joins a space (lockra-core `JoinSource`). */
export const joinSourceSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("invite"), text: z.string() }),
  z.object({ type: z.literal("manual"), storage: storageConfigSchema, sync_key: z.string() }),
]);
export type JoinSource = z.infer<typeof joinSourceSchema>;

/** The longest device name kept (lockra-core `MAX_DEVICE_NAME_CHARS`). */
export const MAX_DEVICE_NAME_CHARS = 64;

// ---- state and events ------------------------------------------------------------------------

export const lockViewSchema = z.object({
  device_unlock: z.object({ available: z.boolean(), enabled: z.boolean() }),
  failed_attempts: z.number().int().nonnegative(),
  retry_at_ms: msSchema.nullable(),
});
export type LockView = z.infer<typeof lockViewSchema>;

export const uiStateSchema = z.object({
  app_version: z.string(),
  platform: platformSchema,
  phase: phaseSchema,
  data_dir: z.string(),
  lock: lockViewSchema,
  entries: z.array(entryViewSchema),
  settings: settingsSchema,
  import: importViewSchema.nullable(),
  backup: backupViewSchema,
  restore: restoreViewSchema.nullable(),
  auto_lock_at_ms: msSchema.nullable(),
  update: updateViewSchema,
  sync: syncViewSchema,
});
export type UiState = z.infer<typeof uiStateSchema>;

export const noticeSchema = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("copied"),
    entry_id: idSchema,
    clear_after_s: z.number().int().nullable(),
  }),
  z.object({ type: z.literal("clipboard_cleared") }),
  z.object({
    type: z.literal("imported"),
    added: z.number().int(),
    replaced: z.number().int(),
    skipped: z.number().int(),
  }),
  z.object({ type: z.literal("file_unrecognized"), name: z.string() }),
  z.object({ type: z.literal("file_unreadable"), name: z.string() }),
  z.object({ type: z.literal("backup_written"), file_name: z.string(), automatic: z.boolean() }),
  z.object({ type: z.literal("backup_failed"), code: errorCodeSchema }),
  z.object({ type: z.literal("restored"), entries: z.number().int() }),
  z.object({ type: z.literal("auto_locked") }),
  z.object({ type: z.literal("export_expired"), session: idSchema }),
  z.object({ type: z.literal("device_unlock_turned_off") }),
]);
export type Notice = z.infer<typeof noticeSchema>;

export const uiEventSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("state"), state: uiStateSchema }),
  z.object({ type: z.literal("notice"), notice: noticeSchema }),
]);
export type UiEvent = z.infer<typeof uiEventSchema>;

/** The Tauri event that carries `UiEvent`s. */
export const UI_EVENT_NAME = "lockra://event";

export const codeViewSchema = z.object({
  entry_id: idSchema,
  code: z.string(),
  next_code: z.string().nullable(),
  valid_from_ms: msSchema.nullable(),
  valid_until_ms: msSchema.nullable(),
});
export type CodeView = z.infer<typeof codeViewSchema>;

export const codesFrameSchema = z.object({ at_ms: msSchema, codes: z.array(codeViewSchema) });
export type CodesFrame = z.infer<typeof codesFrameSchema>;

// ---- command answers -------------------------------------------------------------------------

export const EXPORT_TARGETS = ["google", "microsoft"] as const;
export const exportTargetSchema = z.enum(EXPORT_TARGETS);
export type ExportTarget = z.infer<typeof exportTargetSchema>;

export const entryAddedSchema = z.object({ id: idSchema });
export type EntryAdded = z.infer<typeof entryAddedSchema>;

export const exportStartedSchema = z.object({
  session: idSchema,
  target: exportTargetSchema,
  pages: z.number().int().min(1),
  excluded: z.array(z.object({ entry_id: idSchema, reason: incompatibleSchema })),
});
export type ExportStarted = z.infer<typeof exportStartedSchema>;

export const exportPageSchema = z.object({
  session: idSchema,
  index: z.number().int().nonnegative(),
  total: z.number().int().min(1),
  svg: z.string(),
  entry_ids: z.array(idSchema),
});
export type ExportPage = z.infer<typeof exportPageSchema>;

export const revealedSchema = z.object({
  entry_id: idSchema,
  secret: z.string(),
  uri: z.string(),
  svg: z.string(),
});
export type Revealed = z.infer<typeof revealedSchema>;

export const importOutcomeSchema = z.object({
  added: z.number().int(),
  replaced: z.number().int(),
  skipped: z.number().int(),
});
export type ImportOutcome = z.infer<typeof importOutcomeSchema>;

/** The answer to `sync_create`: the new space's sync key, shown once. */
export const syncCreatedSchema = z.object({ sync_key: z.string() });
export type SyncCreated = z.infer<typeof syncCreatedSchema>;

/** The answer to `sync_invite`: what another device scans or pastes to join. */
export const syncInviteSchema = z.object({
  invite: z.string(),
  svg: z.string(),
  sync_key: z.string(),
});
export type SyncInvite = z.infer<typeof syncInviteSchema>;

export const coreErrorSchema = z.object({
  code: errorCodeSchema,
  retry_at_ms: msSchema.optional(),
});
export type CoreError = z.infer<typeof coreErrorSchema>;

// ---- commands --------------------------------------------------------------------------------

export const entryDraftSchema = z.object({
  issuer: z.string().optional(),
  account: z.string().optional(),
  secret: z.string(),
  kind: otpKindSchema,
  algorithm: algorithmSchema.optional(),
  digits: z.number().int().min(6).max(8).optional(),
  group: z.string().nullable().optional(),
});
export type EntryDraft = z.infer<typeof entryDraftSchema>;

export const entryPatchSchema = z.object({
  issuer: z.string().optional(),
  account: z.string().optional(),
  group: z.string().optional(),
  favorite: z.boolean().optional(),
});
export type EntryPatch = z.infer<typeof entryPatchSchema>;

export const choiceSchema = z.object({
  id: z.number().int().nonnegative(),
  action: candidateActionSchema,
});
export type Choice = z.infer<typeof choiceSchema>;

export const RESTORE_MODES = ["merge", "replace"] as const;
export const restoreModeSchema = z.enum(RESTORE_MODES);
export type RestoreMode = z.infer<typeof restoreModeSchema>;

const password = z.string();

export const uiCommandSchema = z.discriminatedUnion("command", [
  z.object({ command: z.literal("app_state") }),
  z.object({ command: z.literal("vault_create"), password }),
  z.object({ command: z.literal("vault_unlock"), password }),
  z.object({ command: z.literal("vault_unlock_device") }),
  z.object({ command: z.literal("vault_lock") }),
  z.object({ command: z.literal("vault_change_password"), current: password, new: password }),
  z.object({ command: z.literal("vault_reset") }),
  z.object({ command: z.literal("device_unlock_enable") }),
  z.object({ command: z.literal("device_unlock_disable"), password }),
  z.object({ command: z.literal("entry_add_uri"), uri: z.string() }),
  z.object({ command: z.literal("entry_add_manual"), draft: entryDraftSchema }),
  z.object({ command: z.literal("entry_update"), id: idSchema, patch: entryPatchSchema }),
  z.object({ command: z.literal("entry_delete"), id: idSchema }),
  z.object({ command: z.literal("entry_hotp_next"), id: idSchema }),
  z.object({ command: z.literal("entry_copy"), id: idSchema }),
  z.object({ command: z.literal("entry_reveal"), id: idSchema, password }),
  z.object({ command: z.literal("import_text"), text: z.string() }),
  z.object({ command: z.literal("import_clipboard") }),
  z.object({ command: z.literal("import_backup_password"), password }),
  z.object({ command: z.literal("import_commit"), choices: z.array(choiceSchema).optional() }),
  z.object({ command: z.literal("import_cancel") }),
  z.object({
    command: z.literal("export_start"),
    target: exportTargetSchema,
    entry_ids: z.array(idSchema),
    password,
  }),
  z.object({
    command: z.literal("export_page"),
    session: idSchema,
    index: z.number().int().nonnegative(),
  }),
  z.object({ command: z.literal("export_close"), session: idSchema }),
  z.object({ command: z.literal("secret_view_closed") }),
  z.object({ command: z.literal("backup_auto_now") }),
  z.object({ command: z.literal("restore_commit"), password, mode: restoreModeSchema }),
  z.object({ command: z.literal("restore_cancel") }),
  z.object({ command: z.literal("settings_set"), settings: settingsSchema }),
  z.object({ command: z.literal("activity") }),
  z.object({ command: z.literal("update_check") }),
  z.object({ command: z.literal("update_install") }),
  z.object({
    command: z.literal("sync_create"),
    storage: storageConfigSchema,
    password,
    device_name: z.string(),
  }),
  z.object({
    command: z.literal("sync_join"),
    source: joinSourceSchema,
    password,
    device_name: z.string(),
  }),
  z.object({ command: z.literal("sync_invite"), password }),
  z.object({ command: z.literal("sync_set_storage"), storage: storageConfigSchema, password }),
  z.object({ command: z.literal("sync_rename_device"), name: z.string() }),
  z.object({ command: z.literal("sync_remove_device"), tag: z.string() }),
  z.object({ command: z.literal("sync_now") }),
  z.object({ command: z.literal("sync_disable") }),
]);
export type UiCommand = z.infer<typeof uiCommandSchema>;
export type CommandName = UiCommand["command"];
export type CommandOf<C extends CommandName> = Extract<UiCommand, { command: C }>;

/** Every command name, in lockra-bridge `COMMANDS` order (the contract test compares the two). */
export const COMMAND_NAMES = uiCommandSchema.options.map((option) => option.shape.command.value);

/** The desktop shell's own Tauri commands (lockra-bridge `SHELL_COMMANDS`). */
export const SHELL_COMMAND_NAMES = [
  "lockra_dispatch",
  "codes_subscribe",
  "codes_unsubscribe",
  "import_pick_files",
  "backup_save",
  "backup_pick_dir",
  "restore_pick",
  "export_otpauth_file",
] as const;

/** Commands that answer with something other than `null`. */
export interface CommandResults {
  app_state: UiState;
  entry_add_uri: EntryAdded;
  entry_add_manual: EntryAdded;
  entry_reveal: Revealed;
  import_commit: ImportOutcome;
  export_start: ExportStarted;
  export_page: ExportPage;
  sync_create: SyncCreated;
  sync_invite: SyncInvite;
}
export type ResultOf<C extends CommandName> = C extends keyof CommandResults
  ? CommandResults[C]
  : null;

export const RESULT_SCHEMAS: { [C in keyof CommandResults]: z.ZodType<CommandResults[C]> } = {
  app_state: uiStateSchema,
  entry_add_uri: entryAddedSchema,
  entry_add_manual: entryAddedSchema,
  entry_reveal: revealedSchema,
  import_commit: importOutcomeSchema,
  export_start: exportStartedSchema,
  export_page: exportPageSchema,
  sync_create: syncCreatedSchema,
  sync_invite: syncInviteSchema,
};

export function hasResult(name: CommandName): name is keyof CommandResults {
  return Object.prototype.hasOwnProperty.call(RESULT_SCHEMAS, name);
}
