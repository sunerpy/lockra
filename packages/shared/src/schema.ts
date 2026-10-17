// The IPC contract: zod schemas for every value the Rust core sends and every command the webview
// sends. The Rust types (lockra-core `ui.rs`, lockra-bridge `UiCommand`) are the source of truth;
// `ipc-contract.test.ts` parses the fixtures they write (packages/shared/src/fixtures/ipc/) with
// these schemas, so the two sides cannot drift apart unnoticed.
import { z } from "zod";

// ---- settings --------------------------------------------------------------------------------

export const THEME_IDS = [
  "light",
  "dark",
  "warm",
  "graphite",
  "glass",
  "glass_night",
  "paper",
  "paper_night",
] as const;
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

export const SORT_ORDERS = ["name", "added", "recent", "manual"] as const;
export const sortOrderSchema = z.enum(SORT_ORDERS);
export type SortOrder = z.infer<typeof sortOrderSchema>;

/** Rows or a grid of cards, whatever the theme (lockra-core `CodesView`). */
export const CODES_VIEWS = ["list", "grid"] as const;
export const codesViewSchema = z.enum(CODES_VIEWS);
export type CodesView = z.infer<typeof codesViewSchema>;

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

/** How the lock screen unlocks first where Touch ID, Windows Hello or a fingerprint unlocks the
 *  vault (lockra-core `DefaultUnlock`). */
export const DEFAULT_UNLOCKS = ["biometric", "password"] as const;
export const defaultUnlockSchema = z.enum(DEFAULT_UNLOCKS);
export type DefaultUnlock = z.infer<typeof defaultUnlockSchema>;

// ---- the password generator (lockra-autofill) ---------------------------------------------------

/** A random password's options (lockra-autofill `PasswordOptions`). */
export const passwordOptionsSchema = z.object({
  kind: z.literal("password"),
  /** 4 to 128; a site's rules may lengthen or shorten it. */
  length: z.number().int(),
  lower: z.boolean(),
  upper: z.boolean(),
  digits: z.boolean(),
  symbols: z.boolean(),
  /** Leave out `I l 1 | O 0`. */
  avoid_ambiguous: z.boolean(),
  /** Characters never to use. */
  exclude: z.string(),
});
/** A passphrase's options (lockra-autofill `PassphraseOptions`), words from the EFF's list. */
export const passphraseOptionsSchema = z.object({
  kind: z.literal("passphrase"),
  /** 3 to 20. */
  words: z.number().int(),
  separator: z.string(),
  capitalize: z.boolean(),
  number: z.boolean(),
});
export const generatorOptionsSchema = z.discriminatedUnion("kind", [
  passwordOptionsSchema,
  passphraseOptionsSchema,
]);
export type GeneratorOptions = z.infer<typeof generatorOptionsSchema>;

/** The generator's defaults (lockra-autofill `GeneratorOptions::default()`). */
export function defaultGeneratorOptions(): GeneratorOptions {
  return {
    kind: "password",
    length: 20,
    lower: true,
    upper: true,
    digits: true,
    symbols: true,
    avoid_ambiguous: false,
    exclude: "",
  };
}

/** The order of the password list (lockra-core `ItemSort`); favourites always come first. */
export const ITEM_SORTS = ["name", "recent", "modified", "added"] as const;
export const itemSortSchema = z.enum(ITEM_SORTS);
export type ItemSort = z.infer<typeof itemSortSchema>;

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
  group_codes: z.boolean(),
  codes_view: codesViewSchema,
  auto_backup: autoBackupSchema,
  auto_update: z.boolean(),
  default_unlock: defaultUnlockSchema,
  /** Offer the phone's fingerprint once after a master-password unlock, until declined. */
  biometric_offer: z.boolean(),
  /** What the password generator makes unless told otherwise: the last options chosen. */
  generator: generatorOptionsSchema,
  /** The order of the password list. */
  item_sort: itemSortSchema,
  /** The browser extension's integration is on. Only the shell changes it (after writing the
   *  browsers' manifests); `settings_set` leaves it as it is. */
  browser_integration: z.boolean(),
  run_in_background: z.boolean(),
  /** The quick search's global shortcut, an accelerator (`CommandOrControl+Shift+Space`); `null`
   *  registers none. */
  quick_access_shortcut: z.string().nullable(),
});
export type Settings = z.infer<typeof settingsSchema>;

/** The defaults of lockra-core `Settings::default()` (the mock backend and the tests start here). */
export function defaultSettings(): Settings {
  return {
    theme: "glass",
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
    group_codes: true,
    codes_view: "list",
    auto_backup: { enabled: false, dir: null, keep: 10 },
    auto_update: false,
    default_unlock: "biometric",
    biometric_offer: true,
    generator: defaultGeneratorOptions(),
    item_sort: "name",
    browser_integration: false,
    run_in_background: false,
    quick_access_shortcut: null,
  };
}

// ---- entries ---------------------------------------------------------------------------------

export const PLATFORMS = ["windows", "macos", "linux", "android"] as const;
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
  "card_item",
  "identity_item",
  "note_item",
  "other_item",
  "encrypted_export",
  "too_large",
] as const;
export const rejectReasonSchema = z.enum(REJECT_REASONS);
export type RejectReason = z.infer<typeof rejectReasonSchema>;

const idSchema = z.string().min(1);
const msSchema = z.number().int().nonnegative();

/** The colours an account can be shown in; `auto` follows its name (lockra-core `AccountColor`). */
export const ACCOUNT_COLORS = [
  "auto",
  "red",
  "orange",
  "amber",
  "green",
  "teal",
  "blue",
  "indigo",
  "purple",
  "pink",
  "gray",
] as const;
export const accountColorSchema = z.enum(ACCOUNT_COLORS);
export type AccountColor = z.infer<typeof accountColorSchema>;
/** The most characters, as people count them, an account's mark holds (lockra-core `MARK_CHARS`). */
export const MARK_CHARS = 2;

export const entryViewSchema = z.object({
  id: idSchema,
  issuer: z.string(),
  account: z.string(),
  kind: otpKindSchema,
  algorithm: algorithmSchema,
  digits: z.number().int().min(6).max(8),
  group: z.string().nullable(),
  favorite: z.boolean(),
  color: accountColorSchema,
  mark: z.string().nullable(),
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
  z.object({ type: z.literal("camera") }),
]);
export type ImportSource = z.infer<typeof importSourceSchema>;

/** What differs between a login of the import and the vault's: the password, the code linked,
 *  or (a backup's login) the login itself since. */
export const CONFLICT_REASONS = ["password", "code", "changed"] as const;
export const conflictReasonSchema = z.enum(CONFLICT_REASONS);
export type ConflictReason = z.infer<typeof conflictReasonSchema>;

export const candidateStatusSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("new") }),
  z.object({ type: z.literal("exists"), entry_id: idSchema }),
  z.object({ type: z.literal("conflict"), entry_id: idSchema }),
  z.object({ type: z.literal("duplicate") }),
  z.object({ type: z.literal("unsupported"), reason: rejectReasonSchema }),
  /** A login the vault holds already. */
  z.object({ type: z.literal("login_exists"), item_id: idSchema }),
  /** A login whose vault version differs; replacing is the default. */
  z.object({ type: z.literal("login_conflict"), item_id: idSchema, reason: conflictReasonSchema }),
]);
export type CandidateStatus = z.infer<typeof candidateStatusSchema>;

export const CANDIDATE_ACTIONS = ["add", "skip", "replace"] as const;
export const candidateActionSchema = z.enum(CANDIDATE_ACTIONS);
export type CandidateAction = z.infer<typeof candidateActionSchema>;

/** A login the import found: its title, user name and site; its secrets stay in the core. */
export const loginCandidateSchema = z.object({
  title: z.string(),
  username: z.string(),
  /** The host of its first web address; empty when it has none. */
  host: z.string(),
  /** The candidate of its code, when it brings one. */
  totp: z.number().int().nonnegative().nullable(),
});
export type LoginCandidate = z.infer<typeof loginCandidateSchema>;

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
  /** A login, not an account. */
  login: loginCandidateSchema.nullable(),
});
export type CandidateView = z.infer<typeof candidateViewSchema>;

export const googleBatchViewSchema = z.object({
  id: z.number().int(),
  size: z.number().int(),
  received: z.array(z.number().int()),
  missing: z.array(z.number().int()),
});
export type GoogleBatchView = z.infer<typeof googleBatchViewSchema>;

export const AWAITING_KINDS = ["backup", "kdbx"] as const;
/** A file of the import waiting for its password: a Lockra backup, or a KeePass database, which
 *  may take a key file too (chosen through `pickKeyFile`; its name only comes here). */
export const awaitingViewSchema = z.object({
  name: z.string(),
  kind: z.enum(AWAITING_KINDS),
  key_file: z.string().nullable(),
});
export type AwaitingView = z.infer<typeof awaitingViewSchema>;

export const importViewSchema = z.object({
  candidates: z.array(candidateViewSchema),
  google_batches: z.array(googleBatchViewSchema),
  awaiting: awaitingViewSchema.nullable(),
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
  "biometric_cancelled",
  "biometric_failed",
  "biometric_unavailable",
  "entry_not_found",
  "duplicate_entry",
  "invalid_uri",
  "invalid_secret",
  "invalid_parameters",
  "no_import",
  "import_empty",
  "import_unrecognized",
  "import_unreadable",
  "import_key_file_invalid",
  "several_accounts",
  "clipboard_empty",
  "clipboard_failed",
  "camera_denied",
  "camera_unavailable",
  "export_expired",
  "export_nothing",
  "export_acknowledge_needed",
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
  "update_keychain",
  "sync_off",
  "sync_already_on",
  "sync_config_invalid",
  "sync_insecure",
  "sync_network",
  "sync_denied",
  "sync_storage_failed",
  "sync_too_large",
  "sync_space_not_found",
  "sync_wrong_credentials",
  "sync_key_invalid",
  "sync_invite_invalid",
  "sync_invite_code_wrong",
  "sync_space_password_needed",
  "sync_invite_needs_storage",
  "sync_invite_outdated",
  "sync_folder_not_chosen",
  "sync_folder_missing",
  "sync_data_corrupted",
  "sync_unsupported",
  "item_not_found",
  "item_invalid",
  "item_read_only",
  "generator_rules_unsatisfiable",
  "browser_unavailable",
  "browser_not_paired",
  "browser_pair_expired",
  "browser_manifest_failed",
  "shortcut_unavailable",
  "feature_unavailable",
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
export const INSTALL_METHODS = [
  "deb",
  "rpm",
  "appimage",
  "nsis",
  "msi",
  "app",
  "android",
  "play",
] as const;
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
 *  `StorageConfig`). The core never sends the secret back. A folder of this computer (one a cloud
 *  drive keeps in sync) has no path here: it is "the folder chosen" through `pickSyncFolder`. A
 *  Lockra relay needs its address alone: the core shows it the space's access, from the sync key. */
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
  z.object({ kind: z.literal("folder") }),
  z.object({ kind: z.literal("relay"), url: z.string() }),
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
  /** A folder of this computer that a cloud drive keeps in sync, as the system writes it. */
  z.object({ kind: z.literal("folder"), path: z.string() }),
  /** A Lockra relay, by its address. */
  z.object({ kind: z.literal("relay"), url: z.string() }),
]);
export type StorageView = z.infer<typeof storageViewSchema>;

export const syncDeviceViewSchema = z.object({
  tag: z.string(),
  name: z.string(),
  written_at_ms: msSchema.nullable(),
  this_device: z.boolean(),
  /** The Lockra version it writes with, as its snapshot says; null before 0.10, which does not say. */
  app_version: z.string().nullable(),
  /** Its Lockra keeps the passwords (0.9 and earlier do not). */
  passwords: z.boolean(),
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
  /** This device joined by the biometric check: its master password recovers the space only once
   *  it was typed here again (the next unlock with it seals its keyring). */
  keyring_unsealed: z.boolean(),
  /** The sync key was saved or written down; until then Settings › Sync reminds of it. */
  key_saved: z.boolean(),
  /** On a relay: this device's snapshot is past three quarters of what the relay keeps of one. */
  snapshot_warning: z
    .object({ bytes: z.number().int().nonnegative(), limit: z.number().int().positive() })
    .nullable(),
});
export type SyncSpaceView = z.infer<typeof syncSpaceViewSchema>;

export const syncViewSchema = z.object({
  /** The space this device belongs to; absent when sync is off and while locked. */
  space: syncSpaceViewSchema.nullable(),
});
export type SyncView = z.infer<typeof syncViewSchema>;

/** How a device joins a space (lockra-core `JoinSource`). */
export const joinSourceSchema = z.discriminatedUnion("type", [
  /** `code` opens a sealed text (`lockra-invite:2:…`); `storage` is this device's own way to the
   *  space, needed when the invitation holds the sync key alone (`sync_invite_needs_storage`). */
  z.object({
    type: z.literal("invite"),
    text: z.string(),
    code: z.string().optional(),
    storage: storageConfigSchema.optional(),
  }),
  z.object({ type: z.literal("manual"), storage: storageConfigSchema, sync_key: z.string() }),
]);
export type JoinSource = z.infer<typeof joinSourceSchema>;

/** The longest device name kept (lockra-core `MAX_DEVICE_NAME_CHARS`). */
export const MAX_DEVICE_NAME_CHARS = 64;

// ---- state and events ------------------------------------------------------------------------

/** The platform checks Lockra can ask for before "remember on this device" unlocks. */
export const BIOMETRIC_KINDS = ["touch_id", "windows_hello", "fingerprint"] as const;
export const biometricKindSchema = z.enum(BIOMETRIC_KINDS);
export type BiometricKind = z.infer<typeof biometricKindSchema>;

export const lockViewSchema = z.object({
  device_unlock: z.object({
    available: z.boolean(),
    enabled: z.boolean(),
    /** Touch ID or Windows Hello first: what the computer offers, and whether the vault asks. */
    biometric: z.object({ kind: biometricKindSchema.nullable(), enabled: z.boolean() }),
  }),
  failed_attempts: z.number().int().nonnegative(),
  retry_at_ms: msSchema.nullable(),
});
export type LockView = z.infer<typeof lockViewSchema>;

/** The group pinned at the top of the code list: the pinned accounts (收藏), a group or the
 *  accounts in no group (lockra-core `PinnedGroup`). */
export const pinnedGroupSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("favorites") }),
  z.object({ type: z.literal("group"), name: z.string() }),
  z.object({ type: z.literal("ungrouped") }),
]);
export type PinnedGroup = z.infer<typeof pinnedGroupSchema>;

// ---- logins ----------------------------------------------------------------------------------

/** How a saved URL is compared with the page. */
export const MATCHINGS = ["domain", "host", "starts_with", "regex", "exact", "never"] as const;
export const matchingSchema = z.enum(MATCHINGS);
export type Matching = z.infer<typeof matchingSchema>;

/** What the browser extension may do with a login. */
export const PROTECTIONS = ["fill", "confirm", "no_extension"] as const;
export const protectionSchema = z.enum(PROTECTIONS);
export type Protection = z.infer<typeof protectionSchema>;

/** A login's own choice for an autofill preference. */
export const TRIS = ["default", "on", "off"] as const;
export const triSchema = z.enum(TRIS);
export type Tri = z.infer<typeof triSchema>;

/** What a field of a login holds. */
export const PURPOSES = ["username", "email", "password", "custom"] as const;
export const purposeSchema = z.enum(PURPOSES);
export type Purpose = z.infer<typeof purposeSchema>;

export const SITE_RULE_KINDS = ["never_save", "never_fill", "equivalent"] as const;
export const siteRuleKindSchema = z.enum(SITE_RULE_KINDS);
export type SiteRuleKind = z.infer<typeof siteRuleKindSchema>;

export const autofillChoiceSchema = z.object({
  on_page_load: triSchema.default("default"),
  auto_submit: triSchema.default("default"),
  fill_otp: triSchema.default("default"),
});
export type AutofillChoice = z.infer<typeof autofillChoiceSchema>;

export const urlViewSchema = z.object({
  url: z.string(),
  matching: matchingSchema.default("domain"),
});
export type UrlView = z.infer<typeof urlViewSchema>;

/** A custom field: a hidden one shows its name only. */
export const fieldViewSchema = z.object({
  id: idSchema,
  label: z.string(),
  hidden: z.boolean(),
  value: z.string().nullable(),
  has_value: z.boolean(),
  page_field: z.string().nullable(),
});
export type FieldView = z.infer<typeof fieldViewSchema>;

export const conflictViewSchema = z.object({
  field: idSchema,
  purpose: purposeSchema,
  label: z.string(),
});
export type ConflictView = z.infer<typeof conflictViewSchema>;

/** A login as `items_list` shows it (lockra-core `ItemView`): no secret. The core leaves out what
 *  is at its default; the defaults here fill it in. */
export const itemViewSchema = z.object({
  id: idSchema,
  /** `login`, or the name a later Lockra gave its kind (then `read_only`). */
  kind: z.string().default("login"),
  title: z.string(),
  username: z.string().default(""),
  email: z.string().default(""),
  has_password: z.boolean().default(false),
  urls: z.array(urlViewSchema).default([]),
  has_notes: z.boolean().default(false),
  tags: z.array(z.string()).default([]),
  group: z.string().nullable().default(null),
  favorite: z.boolean().default(false),
  color: accountColorSchema.default("auto"),
  mark: z.string().nullable().default(null),
  protection: protectionSchema.default("fill"),
  autofill: autofillChoiceSchema.default({
    on_page_load: "default",
    auto_submit: "default",
    fill_otp: "default",
  }),
  totp_entry: idSchema.nullable().default(null),
  fields: z.array(fieldViewSchema).default([]),
  history_count: z.number().int().nonnegative().default(0),
  conflicts: z.array(conflictViewSchema).default([]),
  source: z.string().nullable().default(null),
  created_at_ms: msSchema,
  updated_at_ms: msSchema,
  last_used_at_ms: msSchema.nullable().default(null),
  /** Holds what a later Lockra wrote: shown, not edited. */
  read_only: z.boolean().default(false),
});
export type ItemView = z.infer<typeof itemViewSchema>;

export const siteRuleViewSchema = z.object({
  id: idSchema,
  kind: z.string(),
  domains: z.array(z.string()),
});
export type SiteRuleView = z.infer<typeof siteRuleViewSchema>;

/** What `item_copy` or `generator_copy` put on the clipboard. */
export const COPIED_PARTS = [
  "username",
  "email",
  "password",
  "totp",
  "field",
  "generated",
] as const;
export const copiedPartSchema = z.enum(COPIED_PARTS);
export type CopiedPart = z.infer<typeof copiedPartSchema>;

// ---- the browser extension ----------------------------------------------------------------------

/** A sign-in form in a frame of another site: never fill it, or ask in Lockra. */
export const FRAME_RULES = ["never", "ask"] as const;
export const frameRuleSchema = z.enum(FRAME_RULES);
export type FrameRule = z.infer<typeof frameRuleSchema>;

/** What the paired extensions may do (lockra-core `BrowserPolicy`), kept in the vault. */
export const browserPolicySchema = z.object({
  fill_otp: z.boolean(),
  copy_otp: z.boolean(),
  auto_submit: z.boolean(),
  confirm_fill: z.boolean(),
  cross_origin_frames: frameRuleSchema,
  on_page_load: z.boolean(),
  /** The port counts too: each port of an address has its own logins. */
  match_port: z.boolean(),
});
export type BrowserPolicy = z.infer<typeof browserPolicySchema>;

/** Why an extension's request gets nothing (lockra-core `Refusal`). */
export const REFUSALS = [
  "unavailable",
  "locked",
  "not_paired",
  "paused",
  "busy",
  "invalid",
  "never_fill",
  "never_save",
  "no_match",
  "cross_origin",
  "http_downgrade",
  "protected",
  "denied",
  "expired",
  "rate_limited",
  "not_found",
] as const;
export const refusalSchema = z.enum(REFUSALS);
export type Refusal = z.infer<typeof refusalSchema>;

export const pairedViewSchema = z.object({
  id: idSchema,
  browser: z.string(),
  label: z.string(),
  extension: z.string(),
  paired_at_ms: msSchema,
  last_seen_ms: msSchema.nullable(),
  /** A session is open now. */
  connected: z.boolean(),
  /** It may run in pages, as it said when it connected last; null until it has. */
  page_access: z.boolean().nullable(),
  /** The version it said it was when it connected last; null until it has, or when it named none. */
  version: z.string().nullable(),
  /** It asked too often and gets nothing until resumed. */
  paused: z.boolean(),
});
export type PairedView = z.infer<typeof pairedViewSchema>;

/** A pairing request: the user compares the code with the extension's and approves. */
export const pairRequestViewSchema = z.object({
  id: idSchema,
  code: z.string(),
  browser: z.string(),
  label: z.string(),
  extension: z.string(),
  expires_at_ms: msSchema,
});
export type PairRequestView = z.infer<typeof pairRequestViewSchema>;

export const CONFIRM_REASONS = ["protected", "cross_origin"] as const;
export const CONFIRM_ACTIONS = ["fill", "copy"] as const;

/** A fill (or a copy from the popup) waiting for the user. */
export const confirmViewSchema = z.object({
  id: idSchema,
  item_id: idSchema,
  title: z.string(),
  username: z.string(),
  frame_url: z.string(),
  /** The page, when the form is in a frame of another site. */
  top_url: z.string().nullable(),
  browser: z.string(),
  label: z.string(),
  reasons: z.array(z.enum(CONFIRM_REASONS)),
  action: z.enum(CONFIRM_ACTIONS),
  /** It needs the master password or the biometric check, not a click. */
  needs_proof: z.boolean(),
  expires_at_ms: msSchema,
});
export type ConfirmView = z.infer<typeof confirmViewSchema>;

export const ACTIVITY_EVENTS = [
  "paired",
  "unpaired",
  "filled",
  "otp_filled",
  "copied",
  "saved",
  "updated",
  "generated",
  "refused",
  "paused",
] as const;

export const activityViewSchema = z.object({
  at_ms: msSchema,
  browser: z.string(),
  label: z.string(),
  site: z.string(),
  event: z.enum(ACTIVITY_EVENTS),
  refusal: refusalSchema.nullable(),
  item_id: idSchema.nullable(),
  title: z.string().nullable(),
});
export type ActivityView = z.infer<typeof activityViewSchema>;

/** The browsers Lockra writes a native-messaging manifest for (the shell's `Browser`), in the
 *  order Settings › Browser lists them. */
export const BROWSER_IDS = ["chrome", "edge", "firefox", "chromium", "brave"] as const;
export const browserIdSchema = z.enum(BROWSER_IDS);
export type BrowserId = z.infer<typeof browserIdSchema>;

/** A browser as the desktop shell sees it (`browser_status`): its profiles on this account, and
 *  Lockra's manifest in place for it. */
export const browserStatusSchema = z.object({
  browser: browserIdSchema,
  installed: z.boolean(),
  manifest: z.boolean(),
});
export type BrowserStatus = z.infer<typeof browserStatusSchema>;

/** The folder the desktop shell keeps its copy of the extension in (`browser_extension`), for
 *  Chrome, Edge and Brave to load unpacked: where it is, the version this build carries, and
 *  whether the folder holds it now. */
export const extensionFolderSchema = z.object({
  path: z.string(),
  version: z.string(),
  ready: z.boolean(),
});
export type ExtensionFolder = z.infer<typeof extensionFolderSchema>;

/** Settings › Browser and the requests waiting for the user (lockra-core `BrowserView`). */
export const browserViewSchema = z.object({
  /** This build offers the extension here (the desktop, password management on). */
  available: z.boolean(),
  /** The integration is on (readable while locked). */
  enabled: z.boolean(),
  paired: z.array(pairedViewSchema),
  pairing: z.array(pairRequestViewSchema),
  confirms: z.array(confirmViewSchema),
  policy: browserPolicySchema,
  /** Newest first; in memory only, emptied as the vault locks. */
  activity: z.array(activityViewSchema),
});
export type BrowserView = z.infer<typeof browserViewSchema>;

/** The quick search (lockra-core `QuickAccessView`): the global shortcut the desktop shell
 *  registers (`null`: none chosen, or no quick search in this build or on this platform), and why
 *  it could not (`shortcut_unavailable`, in memory only). */
export const quickAccessViewSchema = z.object({
  shortcut: z.string().nullable(),
  error: errorCodeSchema.nullable(),
});
export type QuickAccessView = z.infer<typeof quickAccessViewSchema>;

/** What this build offers of what is still being finished (lockra-core `Features`): a release
 *  build turns password management on once it ships; the interface hides what is off. */
export const featuresSchema = z.object({
  passwords: z.boolean(),
});
export type Features = z.infer<typeof featuresSchema>;

export const uiStateSchema = z.object({
  app_version: z.string(),
  platform: platformSchema,
  features: featuresSchema,
  phase: phaseSchema,
  data_dir: z.string(),
  lock: lockViewSchema,
  entries: z.array(entryViewSchema),
  /** Goes up whenever the logins or the site rules change and as the vault opens and locks: ask
   *  for `items_list` again (`useItems`). */
  items_rev: z.number().int().nonnegative(),
  /** How many logins the vault holds; 0 unless unlocked. */
  items_count: z.number().int().nonnegative(),
  /** The site rules; empty unless unlocked. */
  site_rules: z.array(siteRuleViewSchema),
  /** The groups folded in the password list ("" for the logins in no group); this device's. */
  item_collapsed_groups: z.array(z.string()),
  /** The password generator: how many passwords its history holds (0 unless unlocked). */
  generator: z.object({ history_count: z.number().int().nonnegative() }),
  /** The browser extension. */
  browser: browserViewSchema,
  /** The quick search's global shortcut, as the desktop shell registers it. */
  quick_access: quickAccessViewSchema,
  /** The groups folded in the code list ("" for the accounts in no group); empty unless unlocked. */
  collapsed_groups: z.array(z.string()),
  /** The accounts in the order they were dragged into (`manual` order); this device's. */
  entry_order: z.array(idSchema),
  /** The groups in the order they were dragged into ("" for the accounts in no group); this
   *  device's. */
  group_order: z.array(z.string()),
  /** The group this device pins at the top of the code list; none unless unlocked. */
  pinned_group: pinnedGroupSchema.nullable(),
  settings: settingsSchema,
  import: importViewSchema.nullable(),
  backup: backupViewSchema,
  restore: restoreViewSchema.nullable(),
  auto_lock_at_ms: msSchema.nullable(),
  update: updateViewSchema,
  sync: syncViewSchema,
});
export type UiState = z.infer<typeof uiStateSchema>;

/** What an import's commit did with the logins: added, replaced in the vault (a password, a code
 *  linked, a backup's version), and left out (with the other apps' items it does not take). */
export const loginOutcomeSchema = z.object({
  added: z.number().int(),
  replaced: z.number().int(),
  skipped: z.number().int(),
});
export type LoginOutcome = z.infer<typeof loginOutcomeSchema>;

export const noticeSchema = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("copied"),
    entry_id: idSchema,
    clear_after_s: z.number().int().nullable(),
  }),
  /** A part of a login, or a generated password (`item_id` null), is on the clipboard. */
  z.object({
    type: z.literal("item_copied"),
    item_id: idSchema.nullable(),
    part: copiedPartSchema,
    clear_after_s: z.number().int().nullable(),
  }),
  z.object({ type: z.literal("clipboard_cleared") }),
  /** A login saved, or its password changed, from the browser extension. */
  z.object({
    type: z.literal("item_saved"),
    item_id: idSchema,
    title: z.string(),
    updated: z.boolean(),
  }),
  /** An extension asks to pair: bring the window up with the request. */
  z.object({
    type: z.literal("browser_pair_requested"),
    id: idSchema,
    browser: z.string(),
    code: z.string(),
  }),
  /** An extension asks to fill or copy a login that waits for the user. */
  z.object({
    type: z.literal("browser_confirm_requested"),
    id: idSchema,
    browser: z.string(),
    title: z.string(),
  }),
  /** An extension asked too often and is paused. */
  z.object({
    type: z.literal("browser_paused"),
    pairing: idSchema,
    browser: z.string(),
    label: z.string(),
  }),
  /** An extension asks Lockra to unlock. */
  z.object({ type: z.literal("browser_unlock_requested"), browser: z.string() }),
  /** The quick search was asked for (its shortcut, the tray, `lockra-desktop --palette`): open the
   *  command palette. Only while unlocked; not a toast. */
  z.object({ type: z.literal("open_palette") }),
  z.object({ type: z.literal("open_settings") }),
  z.object({ type: z.literal("open_updates") }),
  z.object({
    type: z.literal("imported"),
    added: z.number().int(),
    replaced: z.number().int(),
    skipped: z.number().int(),
    /** What became of the logins (and of the other apps' items left out). */
    logins: loginOutcomeSchema,
  }),
  z.object({ type: z.literal("file_unrecognized"), name: z.string() }),
  z.object({ type: z.literal("file_unreadable"), name: z.string() }),
  z.object({ type: z.literal("backup_written"), file_name: z.string(), automatic: z.boolean() }),
  z.object({ type: z.literal("backup_failed"), code: errorCodeSchema }),
  z.object({ type: z.literal("restored"), entries: z.number().int() }),
  z.object({ type: z.literal("auto_locked") }),
  z.object({ type: z.literal("export_expired"), session: idSchema }),
  z.object({ type: z.literal("device_unlock_turned_off") }),
  /** A sync run changed the accounts here; `devices` is empty when the run failed after it. */
  z.object({
    type: z.literal("sync_brought"),
    added: z.number().int(),
    updated: z.number().int(),
    removed: z.number().int(),
    /** Logins new, changed or deleted; counted only where password management is on. */
    logins: z.number().int(),
    devices: z.array(z.string()),
  }),
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

/** What logins export to (`export_items_file`): a KeePass database (KDBX 4) under a password of
 *  its own, or Bitwarden's plain CSV. */
export const ITEMS_EXPORT_FORMATS = ["kdbx", "csv"] as const;
export const itemsExportFormatSchema = z.enum(ITEMS_EXPORT_FORMATS);
export type ItemsExportFormat = z.infer<typeof itemsExportFormatSchema>;

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
  logins: loginOutcomeSchema,
});
export type ImportOutcome = z.infer<typeof importOutcomeSchema>;

/** The answer to `sync_key_reveal`: the space's sync key, which the interface calls the recovery
 *  key. */
export const syncKeyViewSchema = z.object({ sync_key: z.string() });
export type SyncKeyView = z.infer<typeof syncKeyViewSchema>;

/** The answer to `sync_invite`: what another device scans or pastes to join. */
export const syncInviteSchema = z.object({
  /** The invitation; for a space on a relay also the pairing link, copied and sent as it is (it
   *  opens alone, for good). */
  invite: z.string(),
  svg: z.string(),
  /** The invitation sealed for sending, which opens only with `code`, shown on the screen only;
   *  none for a space on a relay. */
  shared: z.object({ text: z.string(), code: z.string() }).nullable(),
  /** The invitation carries the storage; not for a space in a folder of this computer, where the
   *  other device says how it reaches the space. */
  includes_storage: z.boolean(),
});
export type SyncInvite = z.infer<typeof syncInviteSchema>;

/** A hidden value a login's field held before (`item_reveal`'s history). */
export const pastViewSchema = z.object({
  /** What `item_update`'s `settle` names it by. */
  id: z.string(),
  field: idSchema,
  value: z.string(),
  set_at_ms: msSchema,
  /** Written apart from the field's current value: one side of a conflict. */
  conflict: z.boolean(),
});
export type PastView = z.infer<typeof pastViewSchema>;

/** The answer to `item_reveal`: the parts asked for (a secret answer). */
export const itemRevealedSchema = z.object({
  item_id: idSchema,
  password: z.string().nullable(),
  notes: z.string().nullable(),
  fields: z.array(z.object({ id: idSchema, value: z.string() })),
  history: z.array(pastViewSchema).nullable(),
});
export type ItemRevealed = z.infer<typeof itemRevealedSchema>;

/** The answer to `generator_generate` (a secret answer). */
export const generatedViewSchema = z.object({
  password: z.string(),
  bits: z.number().int().nonnegative(),
  /** Made to a site's rules, and meets them. */
  rules_met: z.boolean(),
});
export type GeneratedView = z.infer<typeof generatedViewSchema>;

/** A password of the generator's history (`generator_history_reveal`, a secret answer). */
export const generatedPastViewSchema = z.object({
  password: z.string(),
  at_ms: msSchema,
  site: z.string().nullable(),
});
export type GeneratedPastView = z.infer<typeof generatedPastViewSchema>;

/** The answer to `password_score`: zxcvbn's score and a warning code. */
export const scoreViewSchema = z.object({
  score: z.number().int().min(0).max(4),
  warning: z.string().nullable(),
});
export type ScoreView = z.infer<typeof scoreViewSchema>;

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
  color: accountColorSchema.optional(),
  /** `""` goes back to the name's initial. */
  mark: z.string().optional(),
});
export type EntryPatch = z.infer<typeof entryPatchSchema>;

export const choiceSchema = z.object({
  id: z.number().int().nonnegative(),
  action: candidateActionSchema,
});
export type Choice = z.infer<typeof choiceSchema>;

/** What becomes of a secret in an edit: kept, set, emptied, or the generator's last password
 *  (which the webview need not send back). */
export const secretChangeSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("keep") }),
  z.object({ type: z.literal("set"), value: z.string() }),
  z.object({ type: z.literal("clear") }),
  z.object({ type: z.literal("generated") }),
]);
export type SecretChange = z.infer<typeof secretChangeSchema>;

export const urlDraftSchema = z.object({ url: z.string(), matching: matchingSchema.optional() });
export type UrlDraft = z.infer<typeof urlDraftSchema>;

export const autofillDraftSchema = z.object({
  on_page_load: triSchema.optional(),
  auto_submit: triSchema.optional(),
  fill_otp: triSchema.optional(),
});

/** A custom field added (no `id`), changed or removed. */
export const fieldChangeSchema = z.object({
  id: idSchema.optional(),
  label: z.string().optional(),
  value: secretChangeSchema.optional(),
  hidden: z.boolean().optional(),
  /** The page's field it fills; `""` fills none. */
  page_field: z.string().optional(),
  remove: z.boolean().optional(),
});
export type FieldChange = z.infer<typeof fieldChangeSchema>;

export const itemDraftSchema = z.object({
  title: z.string().optional(),
  username: z.string().optional(),
  email: z.string().optional(),
  password: secretChangeSchema.optional(),
  urls: z.array(urlDraftSchema).optional(),
  notes: z.string().optional(),
  tags: z.array(z.string()).optional(),
  group: z.string().nullable().optional(),
  favorite: z.boolean().optional(),
  color: accountColorSchema.optional(),
  mark: z.string().nullable().optional(),
  protection: protectionSchema.optional(),
  autofill: autofillDraftSchema.optional(),
  totp_entry: idSchema.nullable().optional(),
  fields: z.array(fieldChangeSchema).optional(),
});
export type ItemDraft = z.infer<typeof itemDraftSchema>;

/** What an edit changes; what it leaves out stays. */
export const itemPatchSchema = z.object({
  title: z.string().optional(),
  /** `""` clears it. */
  username: z.string().optional(),
  email: z.string().optional(),
  password: secretChangeSchema.optional(),
  urls: z.array(urlDraftSchema).optional(),
  notes: secretChangeSchema.optional(),
  tags: z.array(z.string()).optional(),
  /** `""` takes it out of its group. */
  group: z.string().optional(),
  favorite: z.boolean().optional(),
  color: accountColorSchema.optional(),
  /** `""` goes back to the title's initial. */
  mark: z.string().optional(),
  protection: protectionSchema.optional(),
  autofill: autofillDraftSchema.optional(),
  /** `null` unlinks the account. */
  totp_entry: idSchema.nullable().optional(),
  fields: z.array(fieldChangeSchema).optional(),
  /** Conflicts settled: take a value of the history (`PastView.id`), or keep the current one. */
  settle: z.array(z.object({ field: idSchema, take: z.string().optional() })).optional(),
});
export type ItemPatch = z.infer<typeof itemPatchSchema>;

export const revealPartsSchema = z.object({
  password: z.boolean().optional(),
  notes: z.boolean().optional(),
  history: z.boolean().optional(),
  fields: z.array(idSchema).optional(),
});
export type RevealParts = z.infer<typeof revealPartsSchema>;

export const copyPartSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("username") }),
  z.object({ type: z.literal("email") }),
  z.object({ type: z.literal("password") }),
  z.object({ type: z.literal("totp") }),
  z.object({ type: z.literal("field"), id: idSchema }),
]);
export type CopyPart = z.infer<typeof copyPartSchema>;

export const RESTORE_MODES = ["merge", "replace"] as const;
export const restoreModeSchema = z.enum(RESTORE_MODES);
export type RestoreMode = z.infer<typeof restoreModeSchema>;

const password = z.string();

export const uiCommandSchema = z.discriminatedUnion("command", [
  z.object({ command: z.literal("app_state") }),
  z.object({ command: z.literal("vault_create"), password }),
  z.object({ command: z.literal("vault_unlock"), password }),
  z.object({ command: z.literal("vault_unlock_device"), reason: z.string().optional() }),
  z.object({ command: z.literal("vault_lock") }),
  z.object({ command: z.literal("vault_change_password"), current: password, new: password }),
  z.object({ command: z.literal("vault_reset") }),
  z.object({ command: z.literal("device_unlock_enable") }),
  z.object({ command: z.literal("device_unlock_disable"), password }),
  z.object({ command: z.literal("device_biometric_enable"), reason: z.string().optional() }),
  z.object({ command: z.literal("device_biometric_disable"), password }),
  z.object({ command: z.literal("entry_add_uri"), uri: z.string() }),
  z.object({ command: z.literal("entry_add_manual"), draft: entryDraftSchema }),
  z.object({ command: z.literal("entry_add_clipboard") }),
  z.object({ command: z.literal("entry_update"), id: idSchema, patch: entryPatchSchema }),
  z.object({ command: z.literal("entry_delete"), id: idSchema }),
  z.object({ command: z.literal("entries_set_group"), ids: z.array(idSchema), group: z.string() }),
  z.object({ command: z.literal("entry_hotp_next"), id: idSchema }),
  z.object({ command: z.literal("entry_copy"), id: idSchema }),
  /** Without a password, the biometric check that unlocks this vault proves presence. */
  z.object({
    command: z.literal("entry_reveal"),
    id: idSchema,
    password: password.optional(),
    reason: z.string().optional(),
  }),
  z.object({ command: z.literal("view_collapse_groups"), groups: z.array(z.string()) }),
  /** The accounts in the order they were dragged into; the code order becomes manual. */
  z.object({ command: z.literal("view_order_entries"), ids: z.array(idSchema) }),
  /** The groups in the order they were dragged into ("" for the accounts in no group). */
  z.object({ command: z.literal("view_order_groups"), groups: z.array(z.string()) }),
  /** Pin a group at the top of the code list, or none. */
  z.object({ command: z.literal("view_pin_group"), pinned: pinnedGroupSchema.nullable() }),
  z.object({ command: z.literal("import_text"), text: z.string() }),
  z.object({ command: z.literal("import_clipboard") }),
  z.object({ command: z.literal("import_backup_password"), password }),
  /** Forget the key file chosen for the KeePass database waiting in the import. */
  z.object({ command: z.literal("import_key_file_clear") }),
  z.object({ command: z.literal("import_commit"), choices: z.array(choiceSchema).optional() }),
  z.object({ command: z.literal("import_cancel") }),
  z.object({
    command: z.literal("export_start"),
    target: exportTargetSchema,
    entry_ids: z.array(idSchema),
    /** Without it, the biometric check that unlocks this vault proves presence. */
    password: password.optional(),
    reason: z.string().optional(),
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
    /** This device's master password: the vault's, or the new vault's. An unlocked vault may
     *  leave it out: the biometric check that unlocks it proves presence then. */
    password: password.optional(),
    device_name: z.string(),
    /** The master password of a device in the space, when it is not `password`. */
    space_password: password.optional(),
    reason: z.string().optional(),
  }),
  /** Without a password, the biometric check that unlocks this vault proves presence. */
  z.object({
    command: z.literal("sync_invite"),
    password: password.optional(),
    reason: z.string().optional(),
  }),
  /** The sealed invitation (`shared.text`), or a relay space's own pairing link, onto the
   *  clipboard, for a device that scans nothing. */
  z.object({ command: z.literal("sync_invite_copy"), text: z.string() }),
  z.object({ command: z.literal("sync_key_acknowledge") }),
  /** The recovery key; without a password, the biometric check that unlocks this vault proves
   *  presence. */
  z.object({
    command: z.literal("sync_key_reveal"),
    password: password.optional(),
    reason: z.string().optional(),
  }),
  /** Without a password, the biometric check that unlocks this vault proves presence. */
  z.object({
    command: z.literal("sync_set_storage"),
    storage: storageConfigSchema,
    password: password.optional(),
    reason: z.string().optional(),
  }),
  z.object({ command: z.literal("sync_rename_device"), name: z.string() }),
  z.object({ command: z.literal("sync_remove_device"), tag: z.string() }),
  z.object({ command: z.literal("sync_now") }),
  z.object({ command: z.literal("sync_disable") }),
  z.object({ command: z.literal("items_list") }),
  z.object({ command: z.literal("item_add"), draft: itemDraftSchema }),
  z.object({ command: z.literal("item_update"), id: idSchema, patch: itemPatchSchema }),
  z.object({ command: z.literal("item_delete"), ids: z.array(idSchema) }),
  /** A `confirm` login needs the master password, or without it the biometric check. */
  z.object({
    command: z.literal("item_reveal"),
    id: idSchema,
    parts: revealPartsSchema,
    password: password.optional(),
    reason: z.string().optional(),
  }),
  z.object({
    command: z.literal("item_copy"),
    id: idSchema,
    part: copyPartSchema,
    password: password.optional(),
    reason: z.string().optional(),
  }),
  z.object({
    command: z.literal("generator_generate"),
    options: generatorOptionsSchema,
    /** The site it is for: its rules are kept to when Lockra knows them. */
    domain: z.string().optional(),
  }),
  z.object({ command: z.literal("generator_copy") }),
  z.object({
    command: z.literal("generator_history_reveal"),
    password: password.optional(),
    reason: z.string().optional(),
  }),
  z.object({ command: z.literal("generator_history_clear") }),
  z.object({ command: z.literal("password_score"), password }),
  z.object({
    command: z.literal("site_rule_add"),
    kind: siteRuleKindSchema,
    domains: z.array(z.string()),
  }),
  z.object({ command: z.literal("site_rule_remove"), id: idSchema }),
  /** Fold these groups of the password list, unfold the others (this device's view). */
  z.object({ command: z.literal("view_collapse_items"), groups: z.array(z.string()) }),
  /** The user compared the codes: approve with the master password or the biometric check. */
  z.object({
    command: z.literal("browser_pair_approve"),
    id: idSchema,
    password: password.optional(),
    reason: z.string().optional(),
  }),
  z.object({ command: z.literal("browser_pair_deny"), id: idSchema }),
  /** A waiting fill or copy goes ahead; proof where the login asks for it. */
  z.object({
    command: z.literal("browser_fill_confirm"),
    id: idSchema,
    password: password.optional(),
    reason: z.string().optional(),
  }),
  z.object({ command: z.literal("browser_fill_deny"), id: idSchema }),
  z.object({ command: z.literal("browser_unpair"), id: idSchema }),
  z.object({ command: z.literal("browser_resume") }),
  z.object({ command: z.literal("browser_activity_clear") }),
  /** Letting the extensions do more asks for proof. */
  z.object({
    command: z.literal("browser_set_policy"),
    policy: browserPolicySchema,
    password: password.optional(),
    reason: z.string().optional(),
  }),
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
  "browser_extension_download",
  "quick_access_done",
] as const;

/** The phone shell's own Tauri commands (lockra-bridge `PHONE_COMMANDS`). */
export const PHONE_COMMAND_NAMES = [
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
] as const;

/** Commands that answer with something other than `null`. */
export interface CommandResults {
  app_state: UiState;
  entry_add_uri: EntryAdded;
  entry_add_manual: EntryAdded;
  entry_add_clipboard: EntryAdded;
  entry_reveal: Revealed;
  import_commit: ImportOutcome;
  export_start: ExportStarted;
  export_page: ExportPage;
  sync_key_reveal: SyncKeyView;
  sync_invite: SyncInvite;
  items_list: ItemView[];
  item_add: EntryAdded;
  item_reveal: ItemRevealed;
  generator_generate: GeneratedView;
  generator_history_reveal: GeneratedPastView[];
  password_score: ScoreView;
}
export type ResultOf<C extends CommandName> = C extends keyof CommandResults
  ? CommandResults[C]
  : null;

export const RESULT_SCHEMAS: { [C in keyof CommandResults]: z.ZodType<CommandResults[C]> } = {
  app_state: uiStateSchema,
  entry_add_uri: entryAddedSchema,
  entry_add_manual: entryAddedSchema,
  entry_add_clipboard: entryAddedSchema,
  entry_reveal: revealedSchema,
  import_commit: importOutcomeSchema,
  export_start: exportStartedSchema,
  export_page: exportPageSchema,
  sync_key_reveal: syncKeyViewSchema,
  sync_invite: syncInviteSchema,
  items_list: z.array(itemViewSchema),
  item_add: entryAddedSchema,
  item_reveal: itemRevealedSchema,
  generator_generate: generatedViewSchema,
  generator_history_reveal: z.array(generatedPastViewSchema),
  password_score: scoreViewSchema,
};

export function hasResult(name: CommandName): name is keyof CommandResults {
  return Object.prototype.hasOwnProperty.call(RESULT_SCHEMAS, name);
}
