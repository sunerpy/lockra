// An in-memory stand-in for the Rust core: the browser preview (`pnpm dev` outside Tauri) and the
// page tests run on it. It follows the core's rules closely enough for the UI to be exercised
// (phases, rate limiting, previews, export sessions, backups) but computes stand-in codes from a
// hash, not HMAC: it never holds a real account. Loaded under `import.meta.env.DEV` only; the
// release bundle check fails if it ships.
import {
  type Backend,
  type EventListener,
  type FrameListener,
  type ImportPickKind,
  LockraError,
  type SaveSyncKey,
  type ScanJoin,
  type ScanPair,
  type TrayLabels,
  type ScanTexts,
  type Unsubscribe,
} from "./backend";
import {
  type BiometricKind,
  type CandidateAction,
  type CandidateView,
  type Choice,
  type CodeView,
  type CodesFrame,
  type CommandName,
  type CommandOf,
  type EntryView,
  type ErrorCode,
  type ExportPage,
  type ExportStarted,
  type ExportTarget,
  type GoogleBatchView,
  type ImportOutcome,
  type Incompatible,
  type InstallMethod,
  type JoinSource,
  type LanOffer,
  type LanView,
  MARK_CHARS,
  type Notice,
  type OtpKind,
  type Platform,
  type ResultOf,
  type Revealed,
  type Settings,
  type StorageConfig,
  type StorageSource,
  type StorageView,
  type SyncInvite,
  type SyncSpaceView,
  type TransportView,
  type UiCommand,
  type UiState,
  MAX_DEVICE_NAME_CHARS,
  defaultSettings,
} from "./schema";
import { MIN_PASSWORD } from "./password";

export interface MockEntry {
  view: EntryView;
  /** A stand-in secret (Base32 letters); only ever shown by `entry_reveal`. */
  secret: string;
}

/** A camera scan: the QR code's text, `null` when left without one, or how it failed. */
export type MockScan = string | null | { error: ErrorCode };

export interface MockOptions {
  phase?: UiState["phase"];
  entries?: MockEntry[];
  password?: string;
  settings?: Partial<Settings>;
  platform?: Platform;
  keychainAvailable?: boolean;
  deviceUnlock?: boolean;
  /** What the computer offers before "remember on this device" unlocks (none by default). */
  biometric?: BiometricKind | null;
  /** The vault asks for it. */
  biometricUnlock?: boolean;
  /** How the next biometric checks answer (they pass by default). */
  biometricAnswer?: ErrorCode | null;
  /** Text the clipboard import reads. */
  clipboard?: string;
  /** What a camera scan reads (`null`, the default: the scan is left), or the error it ends in. */
  scan?: MockScan;
  now?: () => number;
  /** How this copy installs an update; `null`: it cannot update itself (the default). */
  updateMethod?: InstallMethod | null;
  /** The newer release an update check finds; `null` (the default): nothing newer. */
  release?: MockRelease | null;
  /** Make the update fail at this step with `code`. */
  updateFailure?: { step: "check" | "download" | "install"; code: ErrorCode };
  /** The phone's browser opens a release's page (the default), or no app can. */
  releasePageOpens?: boolean;
  /** The sync space this device belongs to (shown once unlocked). */
  sync?: SyncSpaceView | null;
  /** The build syncs over the LAN (a desktop): the hub's server and pairing, in memory. */
  lan?: boolean;
  /** The groups folded in the code list (shown once unlocked). */
  collapsedGroups?: string[];
}

/** A release the mock's update check announces. */
export interface MockRelease {
  version: string;
  notes: string | null;
  date: string | null;
  /** The package size, for the download progress. */
  size: number;
}

export const MOCK_PASSWORD = "correct horse battery";
/** The storage secret the mock accepts (any other is refused, like wrong credentials). */
export const MOCK_STORAGE_SECRET = "storage secret";
/** The one-time code of the mock's sealed invitation. */
export const MOCK_INVITE_CODE = "7K2QM-XW4FD";
/** The check code of every pairing the mock makes. */
export const MOCK_PAIR_CODE = "246813";
/** The sync key of the mock's spaces. */
export const MOCK_SYNC_KEY =
  "LKS1-MFRG-GZDF-MZTW-Q2LK-NNWG-23TP-OBYX-E43U-OR3W-C6DZ-PI2D-AMBR-GQ2D-ARQA";
const FREE_ATTEMPTS = 3;
const EXPORT_PER_CODE = 10;

let idCounter = 0;
function nextId(): string {
  idCounter += 1;
  return `00000000-0000-4000-8000-${idCounter.toString(16).padStart(12, "0")}`;
}

/** FNV-1a: a stable stand-in for HMAC, so codes look real and change per window. */
function hash(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h;
}

export function fakeCode(secret: string, counter: number, digits: number): string {
  const modulus = 10 ** digits;
  return String((hash(`${secret}:${counter}`) * 2654435761) % modulus).padStart(digits, "0");
}

function exportCompat(
  kind: OtpKind,
  algorithm: EntryView["algorithm"],
  digits: number,
): EntryView["export"] {
  const google: Incompatible | null =
    kind.type === "totp" && kind.period !== 30
      ? "period_not_30"
      : digits !== 6 && digits !== 8
        ? "digits_not_6_or_8"
        : null;
  const microsoft: Incompatible | null =
    kind.type === "hotp"
      ? "hotp_not_supported"
      : kind.period !== 30
        ? "period_not_30"
        : algorithm !== "sha1"
          ? "algorithm_not_sha1"
          : digits !== 6
            ? "digits_not_6"
            : null;
  return { google, microsoft };
}

export function mockEntry(
  issuer: string,
  account: string,
  options: Partial<
    Pick<
      EntryView,
      | "kind"
      | "algorithm"
      | "digits"
      | "group"
      | "favorite"
      | "origin"
      | "last_used_at_ms"
      | "color"
      | "mark"
    >
  > & { secret?: string; at?: number } = {},
): MockEntry {
  const kind = options.kind ?? { type: "totp", period: 30 };
  const algorithm = options.algorithm ?? "sha1";
  const digits = options.digits ?? 6;
  const at = options.at ?? Date.UTC(2026, 8, 1);
  return {
    secret:
      options.secret ??
      `${issuer}${account}`
        .toUpperCase()
        .replace(/[^A-Z2-7]/g, "A")
        .padEnd(16, "Q")
        .slice(0, 32),
    view: {
      id: nextId(),
      issuer,
      account,
      kind,
      algorithm,
      digits,
      group: options.group ?? null,
      favorite: options.favorite ?? false,
      color: options.color ?? "auto",
      mark: options.mark ?? null,
      origin: options.origin ?? "uri",
      created_at_ms: at,
      updated_at_ms: at,
      last_used_at_ms: options.last_used_at_ms ?? null,
      export: exportCompat(kind, algorithm, digits),
    },
  };
}

/** A mark as the core keeps it: trimmed, at most `MARK_CHARS` characters as people count them;
 *  `null` when nothing is left. */
export function cleanMark(mark: string): string | null {
  const segments = Array.from(
    new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(mark.trim()),
  );
  const kept = segments
    .slice(0, MARK_CHARS)
    .map((s) => s.segment)
    .join("")
    .trim();
  return kept === "" ? null : kept;
}

/** A believable vault for the preview. */
export function sampleEntries(): MockEntry[] {
  return [
    mockEntry("GitHub", "octocat", { favorite: true, group: "工作" }),
    mockEntry("Google", "alex@gmail.com", { origin: "google" }),
    mockEntry("Microsoft", "alex@outlook.com", { digits: 8, origin: "microsoft" }),
    mockEntry("AWS", "root@acme-corp", { group: "工作", last_used_at_ms: Date.UTC(2026, 8, 29) }),
    mockEntry("Cloudflare", "ops@acme.dev", { group: "工作" }),
    mockEntry("Proton", "alex@proton.me", { algorithm: "sha256" }),
    mockEntry("Bank", "6222 •••• 1234", {
      kind: { type: "hotp", counter: 12 },
      origin: "manual",
      color: "amber",
      mark: "银行",
    }),
    mockEntry("Game", "player-one", { kind: { type: "totp", period: 60 }, digits: 7 }),
  ];
}

interface MockCandidate {
  view: CandidateView;
  secret: string;
}

interface MockExport {
  target: ExportTarget;
  pages: { svg: string; entryIds: string[] }[];
}

function placeholderSvg(seed: string): string {
  const cells: string[] = [];
  for (let y = 0; y < 21; y += 1) {
    for (let x = 0; x < 21; x += 1) {
      const finder = (x < 7 && y < 7) || (x > 13 && y < 7) || (x < 7 && y > 13);
      const on = finder
        ? x % 6 === 0 || y % 6 === 0 || (x % 7 > 1 && x % 7 < 5 && y % 7 > 1 && y % 7 < 5)
        : hash(`${seed}:${x}:${y}`) % 2 === 0;
      if (on) cells.push(`<rect x="${x + 2}" y="${y + 2}" width="1" height="1"/>`);
    }
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 25" width="320" height="320"><rect width="25" height="25" fill="#ffffff"/><g fill="#000000">${cells.join("")}</g></svg>`;
}

/** The mock's stand-in for a device tag. */
function mockTag(seed: string): string {
  return Array.from({ length: 4 }, (_, i) =>
    hash(`${seed}:${i}`).toString(16).padStart(8, "0"),
  ).join("");
}

/** A space the mock can start with (the showcase and the page tests): S3, two devices. */
export function mockSyncSpace(overrides: Partial<SyncSpaceView> = {}): SyncSpaceView {
  return {
    storage: {
      kind: "s3",
      endpoint: "https://s3.eu-central-1.amazonaws.com",
      region: "eu-central-1",
      bucket: "my-lockra",
      prefix: "lockra/",
      access_key_id: "AKIAIOSFODNN7EXAMPLE",
      path_style: false,
    },
    device_name: "Desktop",
    devices: [
      {
        tag: mockTag("Desktop"),
        name: "Desktop",
        written_at_ms: Date.UTC(2026, 9, 2, 8),
        this_device: true,
      },
      {
        tag: mockTag("Pixel 8"),
        name: "Pixel 8",
        written_at_ms: Date.UTC(2026, 9, 2, 7),
        this_device: false,
      },
    ],
    status: { state: "synced", at_ms: Date.UTC(2026, 9, 2, 8) },
    last_sync_ms: Date.UTC(2026, 9, 2, 8),
    rolled_back: [],
    unreadable: [],
    keyring_pending: false,
    key_saved: true,
    transports: [
      {
        kind: "cloud",
        status: { state: "synced", at_ms: Date.UTC(2026, 9, 2, 8) },
        last_ok_ms: Date.UTC(2026, 9, 2, 8),
      },
    ],
    lan: null,
    ...overrides,
  };
}

/** The core's checks of a storage configuration (lockra-sync `StorageConfig::validate`). */
function checkStorage(storage: StorageConfig): void {
  const [address, required] =
    storage.kind === "s3"
      ? [
          storage.endpoint,
          [storage.region, storage.bucket, storage.access_key_id, storage.secret_access_key],
        ]
      : [storage.url, [storage.username, storage.password]];
  if (required.some((value) => value.trim() === "")) throw new LockraError("sync_config_invalid");
  let url: URL;
  try {
    url = new URL(address.trim());
  } catch {
    throw new LockraError("sync_config_invalid");
  }
  const loopback = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  if (url.protocol === "http:" && !loopback) throw new LockraError("sync_insecure");
  if (url.protocol !== "https:" && url.protocol !== "http:")
    throw new LockraError("sync_config_invalid");
  const secret = storage.kind === "s3" ? storage.secret_access_key : storage.password;
  if (secret !== MOCK_STORAGE_SECRET) throw new LockraError("sync_denied");
}

/** An invitation the mock reads: v1 as it is, v2 with the mock's code. */
function checkInvite(text: string, code: string | undefined): void {
  const trimmed = text.trim();
  if (trimmed.startsWith("lockra-invite:2:")) {
    const typed = (code ?? "").toUpperCase().replace(/[\s-]/g, "");
    if (typed !== MOCK_INVITE_CODE.replace("-", ""))
      throw new LockraError("sync_invite_code_wrong");
  } else if (!trimmed.startsWith("lockra-invite:1:")) throw new LockraError("sync_invite_invalid");
}

function storageView(storage: StorageConfig): StorageView {
  return storage.kind === "s3"
    ? {
        kind: "s3",
        endpoint: storage.endpoint,
        region: storage.region,
        bucket: storage.bucket,
        prefix: storage.prefix,
        access_key_id: storage.access_key_id,
        path_style: storage.path_style,
      }
    : { kind: "webdav", url: storage.url, prefix: storage.prefix, username: storage.username };
}

function parseOtpauth(
  line: string,
): { issuer: string; account: string; secret: string; kind: OtpKind; digits: number } | undefined {
  const match = /^otpauth:\/\/(totp|hotp)\/([^?]*)\?(.*)$/i.exec(line.trim());
  if (!match) return undefined;
  const [, type = "totp", rawLabel = "", query = ""] = match;
  const params = new URLSearchParams(query);
  const secret = params.get("secret") ?? "";
  if (!/^[A-Za-z2-7 =-]+$/.test(secret)) return undefined;
  const label = decodeURIComponent(rawLabel);
  const [labelIssuer, labelAccount] = label.includes(":") ? label.split(/:(.*)/s) : ["", label];
  const kind: OtpKind =
    type.toLowerCase() === "hotp"
      ? { type: "hotp", counter: Number(params.get("counter") ?? 0) }
      : { type: "totp", period: Number(params.get("period") ?? 30) };
  return {
    issuer: (params.get("issuer") ?? labelIssuer ?? "").trim(),
    account: (labelAccount ?? "").trim(),
    secret: secret.toUpperCase().replace(/[ =-]/g, ""),
    kind,
    digits: Number(params.get("digits") ?? 6),
  };
}

export class MockBackend implements Backend {
  private state: UiState;
  /** Entries kept aside while locked (the real core keeps them encrypted). */
  private lockedEntries: EntryView[] = [];
  private password: string;
  private secrets = new Map<string, string>();
  private readonly listeners = new Set<EventListener>();
  private readonly frameListeners = new Set<FrameListener>();
  private frameTimer: ReturnType<typeof setTimeout> | undefined;
  private candidates: MockCandidate[] = [];
  private batches: GoogleBatchView[] = [];
  private awaiting: string | null = null;
  private exports = new Map<string, MockExport>();
  private restoreEntries: MockEntry[] | null = null;
  private clipboard: string | undefined;
  private scan: MockScan;
  private readonly now: () => number;
  private release: MockRelease | null;
  private updateFailure: MockOptions["updateFailure"];
  private releasePageOpens: boolean;
  /** What the last run found, and whether its package is downloaded (the core's `Pending`). */
  private pending: { release: MockRelease; downloaded: boolean } | null = null;
  /** The sync space, kept here while locked (the core keeps it in the vault). */
  private space: SyncSpaceView | null;
  /** The build syncs over the LAN (`MockOptions.lan`). */
  private readonly lanEnabled: boolean;
  /** This device asking a hub to pair, until the test answers for the hub (`lanWelcome`). */
  private joining: { hub_name: string; code: string; finish: (welcomed: boolean) => void } | null =
    null;
  /** The folded groups, kept here while locked (the core keeps them in the vault). */
  private collapsed: string[];
  private biometricAnswer: ErrorCode | null;
  /** Every command dispatched, for tests. */
  readonly calls: UiCommand[] = [];

  constructor(options: MockOptions = {}) {
    this.now = options.now ?? (() => Date.now());
    this.password = options.password ?? MOCK_PASSWORD;
    this.clipboard = options.clipboard;
    this.scan = options.scan ?? null;
    this.release = options.release ?? null;
    this.updateFailure = options.updateFailure;
    this.releasePageOpens = options.releasePageOpens ?? true;
    this.space = options.sync ?? null;
    this.lanEnabled = options.lan ?? false;
    this.collapsed = options.collapsedGroups ?? [];
    this.biometricAnswer = options.biometricAnswer ?? null;
    const entries = options.entries ?? [];
    for (const entry of entries) this.secrets.set(entry.view.id, entry.secret);
    const phase = options.phase ?? (entries.length > 0 ? "unlocked" : "no_vault");
    this.state = {
      app_version: "0.1.0",
      platform: options.platform ?? "linux",
      phase,
      data_dir: "/home/user/.local/share/dev.lockra.desktop",
      lock: {
        device_unlock: {
          available: options.keychainAvailable ?? true,
          enabled: options.deviceUnlock ?? false,
          biometric: { kind: options.biometric ?? null, enabled: options.biometricUnlock ?? false },
        },
        failed_attempts: 0,
        retry_at_ms: null,
      },
      entries: phase === "unlocked" ? entries.map((e) => e.view) : [],
      collapsed_groups: phase === "unlocked" ? [...this.collapsed] : [],
      settings: { ...defaultSettings(), ...options.settings },
      import: null,
      backup: { last_backup_ms: null, last_auto_file: null, last_auto_error: null },
      restore: null,
      auto_lock_at_ms: null,
      update: { method: options.updateMethod ?? null, status: { state: "idle" } },
      sync: { space: phase === "unlocked" ? this.space : null, joining: null },
    };
    this.lockedEntries = phase === "unlocked" ? [] : entries.map((e) => e.view);
    this.refreshAutoLock();
  }

  async getState(): Promise<UiState> {
    return structuredClone(this.state);
  }

  on(listener: EventListener): Unsubscribe {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async subscribeCodes(onFrame: FrameListener): Promise<Unsubscribe> {
    this.frameListeners.add(onFrame);
    onFrame(this.frame());
    this.scheduleFrame();
    return () => {
      this.frameListeners.delete(onFrame);
      if (this.frameListeners.size === 0 && this.frameTimer !== undefined) {
        clearTimeout(this.frameTimer);
        this.frameTimer = undefined;
      }
    };
  }

  dispatch<C extends CommandName>(command: CommandOf<C>): Promise<ResultOf<C>>;
  async dispatch(command: UiCommand): Promise<unknown> {
    this.calls.push(command);
    const result = this.run(command);
    return result;
  }

  async scanImport(_texts: ScanTexts): Promise<boolean> {
    this.requireUnlocked();
    if (this.scan === null) return false;
    if (typeof this.scan !== "string") throw new LockraError(this.scan.error);
    this.importText(this.scan, { type: "camera" });
    return true;
  }

  async scanJoin(_texts: ScanTexts, join: ScanJoin): Promise<boolean> {
    if (this.scan === null) return false;
    if (typeof this.scan !== "string") throw new LockraError(this.scan.error);
    this.syncJoin(
      { type: "invite", text: this.scan },
      join.password,
      join.deviceName,
      join.spacePassword,
    );
    return true;
  }

  /** The tray's words while it shows (Windows and macOS have one), for tests. */
  tray: TrayLabels | null = null;

  async setTray(labels: TrayLabels | null): Promise<boolean> {
    if (this.state.platform === "linux") return false;
    this.tray = labels;
    return true;
  }

  async scanPair(_texts: ScanTexts, pair: ScanPair): Promise<boolean> {
    if (this.scan === null) return false;
    if (typeof this.scan !== "string") throw new LockraError(this.scan.error);
    await this.lanJoin(this.scan, pair.password, pair.deviceName);
    return true;
  }

  async pickImportFiles(_kind?: ImportPickKind): Promise<boolean> {
    this.requireUnlocked();
    const source = { type: "file" as const, name: "google-export-1.jpg" };
    const accounts: [string, string][] = [
      ["Dropbox", "alex@example.com"],
      ["GitHub", "octocat"],
      ["Slack", "alex@acme"],
    ];
    for (const [issuer, account] of accounts) {
      this.addCandidate(
        source,
        "google",
        issuer,
        account,
        `${issuer}${account}`
          .toUpperCase()
          .replace(/[^A-Z2-7]/g, "A")
          .padEnd(16, "Q")
          .slice(0, 32),
        { type: "totp", period: 30 },
        6,
      );
    }
    this.batches = [{ id: 412_337, size: 2, received: [0], missing: [1] }];
    this.publishImport();
    return true;
  }

  async saveBackup(separatePassword?: string): Promise<string | null> {
    this.requireUnlocked();
    if (separatePassword !== undefined && separatePassword.length < MIN_PASSWORD)
      throw new LockraError("password_too_short");
    const name = "lockra-backup.lockrabackup";
    this.state.backup.last_backup_ms = this.now();
    this.notice({ type: "backup_written", file_name: name, automatic: false });
    this.publish();
    return name;
  }

  async saveSyncKey(args: SaveSyncKey): Promise<boolean> {
    this.requireSpace();
    if (args.template.split("{{sync_key}}").length !== 2) throw new LockraError("internal");
    if (this.cancelNextSave) {
      this.cancelNextSave = false;
      return false;
    }
    this.confirmPresence(args.password, args.reason);
    this.savedSyncKeys.push({
      fileName: args.fileName,
      text: args.template.replace("{{sync_key}}", MOCK_SYNC_KEY),
    });
    this.setSpace({ ...this.requireSpace(), key_saved: true });
    return true;
  }

  async pickBackupDir(): Promise<string | null> {
    this.requireUnlocked();
    const dir = "/home/user/OneDrive/Lockra";
    this.state.settings = {
      ...this.state.settings,
      auto_backup: { ...this.state.settings.auto_backup, dir },
    };
    this.publish();
    return dir;
  }

  async pickRestoreFile(): Promise<boolean> {
    if (this.state.phase === "locked") throw new LockraError("locked");
    this.restoreEntries = sampleEntries().slice(0, 3);
    this.state.restore = {
      file_name: "lockra-auto-20260928-091500.lockrabackup",
      kind: "backup",
      created_at_ms: Date.UTC(2026, 5, 1),
    };
    this.publish();
    return true;
  }

  async exportOtpauthFile(entryIds: readonly string[], password: string): Promise<string | null> {
    this.requireUnlocked();
    this.checkPassword(password);
    if (!entryIds.some((id) => this.state.entries.some((e) => e.id === id)))
      throw new LockraError("export_nothing");
    return "lockra-export.txt";
  }

  /** Test hook: what the clipboard holds for `import_clipboard`. */
  setClipboard(text: string | undefined): void {
    this.clipboard = text;
  }

  async openRelease(): Promise<string | null> {
    if (this.releasePageOpens) return null;
    const status = this.state.update.status;
    const releases = "https://github.com/sunerpy/lockra/releases";
    return status.state === "available"
      ? `${releases}/tag/v${status.version}`
      : `${releases}/latest`;
  }

  /** Test hook: what the next camera scans read. */
  setScan(scan: MockScan): void {
    this.scan = scan;
  }

  /** Test hook: fire a notice as the core would. */
  emitNotice(notice: Notice): void {
    this.notice(notice);
  }

  /** Test hook: the idle time ran out, as the core locks then. */
  autoLock(): void {
    this.lock();
    this.notice({ type: "auto_locked" });
  }

  private run(command: UiCommand): unknown {
    switch (command.command) {
      case "app_state":
        return structuredClone(this.state);
      case "vault_create":
        if (this.state.phase !== "no_vault") throw new LockraError("vault_exists");
        this.checkLength(command.password);
        this.password = command.password;
        this.enterUnlocked([]);
        return null;
      case "vault_unlock":
        return this.unlock(command.password);
      case "vault_unlock_device":
        if (this.state.phase !== "locked") return null;
        if (!this.state.lock.device_unlock.available) throw new LockraError("keychain_unavailable");
        if (!this.state.lock.device_unlock.enabled) throw new LockraError("device_unlock_off");
        if (this.state.lock.device_unlock.biometric.enabled) this.checkUser(command.reason);
        this.enterUnlocked(this.lockedEntries);
        return null;
      case "vault_lock":
        this.lock();
        return null;
      case "vault_change_password":
        this.requireUnlocked();
        this.checkLength(command.new);
        this.checkPassword(command.current);
        this.password = command.new;
        this.publish();
        return null;
      case "vault_reset":
        if (this.state.phase !== "locked") throw new LockraError("no_vault");
        this.lockedEntries = [];
        this.secrets.clear();
        this.space = null;
        this.state.phase = "no_vault";
        this.state.lock = {
          ...this.state.lock,
          failed_attempts: 0,
          retry_at_ms: null,
          device_unlock: { ...this.state.lock.device_unlock, enabled: false },
        };
        this.publish();
        return null;
      case "device_unlock_enable":
        this.requireUnlocked();
        if (!this.state.lock.device_unlock.available) throw new LockraError("keychain_unavailable");
        this.state.lock.device_unlock.enabled = true;
        this.publish();
        return null;
      case "device_unlock_disable":
        this.requireUnlocked();
        if (!this.state.lock.device_unlock.enabled) throw new LockraError("device_unlock_off");
        this.checkPassword(command.password);
        this.state.lock.device_unlock.enabled = false;
        this.state.lock.device_unlock.biometric.enabled = false;
        this.publish();
        return null;
      case "device_biometric_enable":
        this.requireUnlocked();
        // As the core: "remember on this device" comes with it, so the key needs the keychain.
        if (!this.state.lock.device_unlock.enabled && !this.state.lock.device_unlock.available)
          throw new LockraError("keychain_unavailable");
        this.checkUser(command.reason);
        this.state.lock.device_unlock.enabled = true;
        this.state.lock.device_unlock.biometric.enabled = true;
        this.publish();
        return null;
      case "device_biometric_disable":
        this.requireUnlocked();
        this.checkPassword(command.password);
        this.state.lock.device_unlock.biometric.enabled = false;
        this.publish();
        return null;
      case "entry_add_uri": {
        this.requireUnlocked();
        const parsed = parseOtpauth(command.uri);
        if (!parsed) throw new LockraError("invalid_uri");
        return {
          id: this.addEntry(
            parsed.issuer,
            parsed.account,
            parsed.secret,
            parsed.kind,
            "sha1",
            parsed.digits,
            "uri",
            null,
          ),
        };
      }
      case "entry_add_manual": {
        this.requireUnlocked();
        const secret = command.draft.secret.toUpperCase().replace(/[\s=-]/g, "");
        if (!/^[A-Z2-7]{2,}$/.test(secret)) throw new LockraError("invalid_secret");
        const group = command.draft.group?.trim() ? command.draft.group.trim() : null;
        return {
          id: this.addEntry(
            command.draft.issuer?.trim() ?? "",
            command.draft.account?.trim() ?? "",
            secret,
            command.draft.kind,
            command.draft.algorithm ?? "sha1",
            command.draft.digits ?? 6,
            "manual",
            group,
          ),
        };
      }
      case "entry_update": {
        const entry = this.entry(command.id);
        const { issuer, account, group, favorite, color, mark } = command.patch;
        if (issuer !== undefined) entry.issuer = issuer.trim();
        if (account !== undefined) entry.account = account.trim();
        if (group !== undefined) entry.group = group.trim() === "" ? null : group.trim();
        if (favorite !== undefined) entry.favorite = favorite;
        if (color !== undefined) entry.color = color;
        if (mark !== undefined) entry.mark = cleanMark(mark);
        entry.updated_at_ms = this.now();
        this.changed();
        return null;
      }
      case "entry_delete":
        this.entry(command.id);
        this.state.entries = this.state.entries.filter((e) => e.id !== command.id);
        this.secrets.delete(command.id);
        this.changed();
        return null;
      case "entries_set_group": {
        // Every account first: none changes when one is missing.
        const targets = command.ids.map((id) => this.entry(id));
        const group = command.group.trim() === "" ? null : command.group.trim();
        for (const entry of targets)
          if (entry.group !== group) {
            entry.group = group;
            entry.updated_at_ms = this.now();
          }
        this.changed();
        return null;
      }
      case "entry_hotp_next": {
        const entry = this.entry(command.id);
        if (entry.kind.type !== "hotp") throw new LockraError("invalid_parameters");
        entry.kind = { type: "hotp", counter: entry.kind.counter + 1 };
        this.changed();
        return null;
      }
      case "entry_copy": {
        const entry = this.entry(command.id);
        entry.last_used_at_ms = this.now();
        const clear = this.state.settings.clipboard_clear_seconds;
        this.changed();
        this.notice({
          type: "copied",
          entry_id: entry.id,
          clear_after_s: clear > 0 ? clear : null,
        });
        return null;
      }
      case "entry_reveal": {
        const entry = this.entry(command.id);
        this.checkPassword(command.password);
        const secret = this.secrets.get(entry.id) ?? "";
        const revealed: Revealed = {
          entry_id: entry.id,
          secret: secret.match(/.{1,4}/g)?.join(" ") ?? secret,
          uri: `otpauth://${entry.kind.type}/${encodeURIComponent(entry.issuer)}:${encodeURIComponent(entry.account)}?secret=${secret}`,
          svg: placeholderSvg(entry.id),
        };
        return revealed;
      }
      case "view_collapse_groups": {
        this.requireUnlocked();
        // As the core: the groups that exist, once each, and "" for the accounts in no group.
        const existing = new Set(this.state.entries.map((e) => e.group ?? ""));
        const folded = [...new Set(command.groups.map((g) => g.trim()))].filter((g) =>
          g === "" ? true : existing.has(g),
        );
        // oxlint-disable-next-line unicorn/no-array-sort
        this.collapsed = folded.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
        this.state.collapsed_groups = [...this.collapsed];
        this.publish();
        return null;
      }
      case "import_text":
        this.requireUnlocked();
        return this.importText(command.text, { type: "text" });
      case "import_clipboard":
        this.requireUnlocked();
        if (!this.clipboard?.toLowerCase().includes("otpauth"))
          throw new LockraError("clipboard_empty");
        return this.importText(this.clipboard, { type: "clipboard" });
      case "import_backup_password":
        this.requireUnlocked();
        if (this.awaiting === null) throw new LockraError("no_import");
        this.checkPassword(command.password);
        this.awaiting = null;
        for (const entry of sampleEntries().slice(0, 2)) {
          this.addCandidate(
            { type: "file", name: "other.lockrabackup" },
            "backup",
            entry.view.issuer,
            entry.view.account,
            entry.secret,
            entry.view.kind,
            entry.view.digits,
          );
        }
        this.publishImport();
        return null;
      case "import_commit":
        return this.commit(command.choices ?? []);
      case "import_cancel":
        this.clearImport();
        this.publish();
        return null;
      case "export_start":
        return this.exportStart(command.target, command.entry_ids, command.password);
      case "export_page": {
        this.requireUnlocked();
        const session = this.exports.get(command.session);
        const page = session?.pages[command.index];
        if (!session || !page) throw new LockraError("export_expired");
        const answer: ExportPage = {
          session: command.session,
          index: command.index,
          total: session.pages.length,
          svg: page.svg,
          entry_ids: page.entryIds,
        };
        return answer;
      }
      case "export_close":
        this.exports.delete(command.session);
        return null;
      case "secret_view_closed":
        return null;
      case "backup_auto_now": {
        this.requireUnlocked();
        if (this.state.settings.auto_backup.dir === null)
          throw new LockraError("backup_dir_missing");
        const name = "lockra-auto-20261001-081500.lockrabackup";
        this.state.backup = {
          last_backup_ms: this.now(),
          last_auto_file: name,
          last_auto_error: null,
        };
        this.notice({ type: "backup_written", file_name: name, automatic: true });
        this.publish();
        return null;
      }
      case "restore_commit":
        return this.restore(command.password, command.mode);
      case "restore_cancel":
        this.state.restore = null;
        this.restoreEntries = null;
        this.publish();
        return null;
      case "settings_set": {
        const settings = command.settings;
        if (settings.auto_backup.enabled && settings.auto_backup.dir === null)
          throw new LockraError("backup_dir_missing");
        const updateTurnedOn = settings.auto_update && !this.state.settings.auto_update;
        this.state.settings = {
          ...settings,
          font_size_px: Math.min(18, Math.max(12, settings.font_size_px)),
        };
        this.refreshAutoLock();
        this.publish();
        // Turned on: look and download now, like the core (a busy updater skips it).
        if (updateTurnedOn && this.state.update.method !== null && !this.updateBusy())
          this.runUpdate("auto");
        return null;
      }
      case "activity":
        this.refreshAutoLock();
        return null;
      case "update_check":
        return this.runUpdate("check");
      case "update_install":
        return this.runUpdate("install");
      case "sync_create":
        return this.syncCreate(command.storage, command.password, command.device_name);
      case "sync_join":
        return this.syncJoin(
          command.source,
          command.password,
          command.device_name,
          command.space_password,
        );
      case "sync_invite":
        return this.syncInvite(command.password, command.reason);
      case "sync_key_acknowledge":
        this.setSpace({ ...this.requireSpace(), key_saved: true });
        return null;
      case "sync_set_storage": {
        const space = this.requireSpace();
        this.checkPassword(command.password);
        checkStorage(command.storage);
        this.setSpace({ ...space, storage: storageView(command.storage) });
        return null;
      }
      case "sync_rename_device": {
        const space = this.requireSpace();
        const name = this.deviceName(command.name);
        this.setSpace({
          ...space,
          device_name: name,
          devices: space.devices.map((d) => (d.this_device ? { ...d, name } : d)),
        });
        return null;
      }
      case "sync_remove_device": {
        const space = this.requireSpace();
        const device = space.devices.find((d) => d.tag === command.tag);
        if (device?.this_device) throw new LockraError("internal");
        this.setSpace({
          ...space,
          devices: space.devices.filter((d) => d.tag !== command.tag),
          rolled_back: space.rolled_back.filter((t) => t !== command.tag),
          unreadable: space.unreadable.filter((t) => t !== command.tag),
        });
        return null;
      }
      case "sync_now":
        this.syncRun(this.requireSpace());
        return null;
      case "sync_disable":
        this.requireSpace();
        this.setSpace(null);
        return null;
      case "sync_lan_enable":
        return this.lanEnable(command.password, command.reason, command.device_name);
      case "sync_lan_offer":
        return this.lanOffer(command.password, command.reason);
      case "sync_lan_answer":
        return this.lanAnswer(command.approve);
      case "sync_lan_remove_peer":
        return this.lanRemovePeer(command.peer_id);
      case "sync_lan_disable":
        return this.lanDisable();
      case "sync_lan_join":
        return this.lanJoin(command.text, command.password, command.device_name);
      case "sync_add_storage":
        return this.addStorage(command.source, command.password, command.reason);
      case "sync_remove_storage":
        return this.removeStorage();
    }
  }

  private requireSpace(): SyncSpaceView {
    this.requireUnlocked();
    if (this.space === null) throw new LockraError("sync_off");
    return this.space;
  }

  private setSpace(space: SyncSpaceView | null): void {
    this.space = space;
    this.state.sync = { space, joining: this.joiningView() };
    this.publish();
  }

  private joiningView(): { hub_name: string; code: string } | null {
    return this.joining === null
      ? null
      : { hub_name: this.joining.hub_name, code: this.joining.code };
  }

  private requireLan(): void {
    if (!this.lanEnabled) throw new LockraError("sync_lan_unavailable");
  }

  private hubOf(space: SyncSpaceView): Extract<LanView, { role: "hub" }> {
    if (space.lan?.role !== "hub") throw new LockraError("sync_off");
    return space.lan;
  }

  /** The hub with the space's devices: a new space on the LAN alone, or the LAN beside it. */
  private lanEnable(
    password: string | undefined,
    reason: string | undefined,
    deviceName: string | undefined,
  ): null {
    this.requireLan();
    this.requireUnlocked();
    const lan: LanView = {
      role: "hub",
      serving: true,
      port: 47_100,
      peers: [],
      request: null,
      offer_until_ms: null,
    };
    const row: TransportView = { kind: "lan", status: { state: "idle" }, last_ok_ms: null };
    if (this.space === null) {
      if (password === undefined) throw new LockraError("wrong_password");
      this.checkPassword(password);
      const name = this.deviceName(deviceName ?? "");
      this.syncRun({
        storage: null,
        device_name: name,
        devices: [{ tag: mockTag(name), name, written_at_ms: null, this_device: true }],
        status: { state: "idle" },
        last_sync_ms: null,
        rolled_back: [],
        unreadable: [],
        keyring_pending: false,
        key_saved: false,
        transports: [row],
        lan,
      });
      return null;
    }
    if (this.space.lan !== null) throw new LockraError("sync_already_on");
    this.confirmPresence(password, reason);
    this.syncRun({ ...this.space, lan, transports: [row, ...this.space.transports] });
    return null;
  }

  private lanOffer(password: string | undefined, reason: string | undefined): LanOffer {
    this.requireLan();
    const space = this.requireSpace();
    this.hubOf(space);
    this.confirmPresence(password, reason);
    const expires_at_ms = this.now() + 120_000;
    this.setSpace({ ...space, lan: { ...this.hubOf(space), offer_until_ms: expires_at_ms } });
    return {
      text: `lockra-pair:1:${btoa(`mock-offer:${this.now()}`)}`,
      svg: placeholderSvg("pair"),
      expires_at_ms,
    };
  }

  /** A device asks to pair with this hub (a test's, or the showcase's). */
  lanAsk(name: string, platform: string): void {
    const space = this.requireSpace();
    this.setSpace({
      ...space,
      lan: { ...this.hubOf(space), request: { name, platform, code: MOCK_PAIR_CODE } },
    });
  }

  private lanAnswer(approve: boolean): null {
    this.requireLan();
    const space = this.requireSpace();
    const hub = this.hubOf(space);
    if (hub.request === null) throw new LockraError("sync_pairing_expired");
    const { name, platform } = hub.request;
    if (!approve) {
      this.setSpace({ ...space, lan: { ...hub, request: null } });
      return null;
    }
    if (hub.peers.length >= 32) {
      this.setSpace({ ...space, lan: { ...hub, request: null } });
      throw new LockraError("sync_lan_full");
    }
    const tag = mockTag(`${name}:${this.now()}`);
    this.syncRun({
      ...space,
      lan: {
        ...hub,
        request: null,
        offer_until_ms: null,
        peers: [...hub.peers, { peer_id: nextId(), name, platform, tag }],
      },
      devices: [...space.devices, { tag, name, written_at_ms: this.now(), this_device: false }],
    });
    return null;
  }

  private lanRemovePeer(peerId: string): null {
    this.requireLan();
    const space = this.requireSpace();
    const hub = this.hubOf(space);
    const peer = hub.peers.find((p) => p.peer_id === peerId);
    if (peer === undefined) throw new LockraError("internal");
    this.setSpace({
      ...space,
      lan: { ...hub, peers: hub.peers.filter((p) => p.peer_id !== peerId) },
      devices: space.devices.filter((d) => d.tag !== peer.tag),
    });
    return null;
  }

  private lanDisable(): null {
    const space = this.requireSpace();
    if (space.lan === null) throw new LockraError("sync_off");
    if (space.storage === null) {
      this.setSpace(null);
      return null;
    }
    this.setSpace({
      ...space,
      lan: null,
      transports: space.transports.filter((t) => t.kind !== "lan"),
    });
    return null;
  }

  /** Pairing with a hub: the code shows until the test answers for the hub (`lanWelcome`). */
  private async lanJoin(text: string, password: string, deviceName: string): Promise<null> {
    this.requireLan();
    if (!text.trim().startsWith("lockra-pair:1:")) throw new LockraError("sync_pairing_invalid");
    if (this.state.phase === "locked") throw new LockraError("locked");
    if (this.state.phase === "unlocked" && this.space?.lan)
      throw new LockraError("sync_already_on");
    if (this.state.phase === "no_vault") this.checkLength(password);
    else this.checkPassword(password);
    const welcomed = await new Promise<boolean>((finish) => {
      this.joining = { hub_name: "Desktop", code: MOCK_PAIR_CODE, finish };
      this.state.sync = { ...this.state.sync, joining: this.joiningView() };
      this.publish();
    });
    this.joining = null;
    this.state.sync = { ...this.state.sync, joining: null };
    this.publish();
    if (!welcomed) throw new LockraError("sync_pairing_refused");
    if (this.state.phase === "no_vault") {
      // A new device: the vault comes from the space.
      this.password = password;
      const entries = sampleEntries();
      for (const entry of entries) this.secrets.set(entry.view.id, entry.secret);
      this.enterUnlocked(entries.map((e) => e.view));
    }
    const name = this.deviceName(deviceName);
    const lan: LanView = { role: "client", hub_name: "Desktop" };
    const row: TransportView = { kind: "lan", status: { state: "idle" }, last_ok_ms: null };
    if (this.space !== null) {
      this.syncRun({ ...this.space, lan, transports: [row, ...this.space.transports] });
      return null;
    }
    this.syncRun({
      storage: null,
      device_name: name,
      devices: [
        { tag: mockTag(`${name}:${this.now()}`), name, written_at_ms: null, this_device: true },
        { tag: mockTag("Desktop"), name: "Desktop", written_at_ms: this.now(), this_device: false },
      ],
      status: { state: "idle" },
      last_sync_ms: null,
      rolled_back: [],
      unreadable: [],
      keyring_pending: false,
      key_saved: true,
      transports: [row],
      lan,
    });
    return null;
  }

  /** The hub's answer to this device's pairing request (a test's). */
  lanWelcome(approve: boolean): void {
    this.joining?.finish(approve);
  }

  private addStorage(
    source: StorageSource,
    password: string | undefined,
    reason: string | undefined,
  ): null {
    const space = this.requireSpace();
    if (space.storage !== null) throw new LockraError("sync_already_on");
    if (source.type === "invite") checkInvite(source.text, source.code);
    const storage: StorageConfig =
      source.type === "storage"
        ? source.storage
        : {
            kind: "webdav",
            url: "https://dav.example.com/dav/",
            prefix: "lockra",
            username: "me@example.com",
            password: MOCK_STORAGE_SECRET,
          };
    checkStorage(storage);
    this.confirmPresence(password, reason);
    this.syncRun({
      ...space,
      storage: storageView(storage),
      transports: [
        ...space.transports,
        { kind: "cloud", status: { state: "idle" }, last_ok_ms: null },
      ],
    });
    return null;
  }

  private removeStorage(): null {
    const space = this.requireSpace();
    if (space.storage === null) throw new LockraError("sync_no_storage");
    if (space.lan === null) {
      this.setSpace(null);
      return null;
    }
    this.setSpace({
      ...space,
      storage: null,
      transports: space.transports.filter((t) => t.kind !== "cloud"),
    });
    return null;
  }

  private deviceName(name: string): string {
    const cleaned = name
      .replace(/\p{Cc}/gu, "")
      .trim()
      .slice(0, MAX_DEVICE_NAME_CHARS)
      .trim();
    if (cleaned !== "") return cleaned;
    return { windows: "Windows", macos: "macOS", linux: "Linux", android: "Android" }[
      this.state.platform
    ];
  }

  /** A run, at once: syncing, then synced (or the test's failure). */
  private syncRun(space: SyncSpaceView): void {
    this.setSpace({ ...space, status: { state: "syncing" } });
    const at_ms = this.now();
    this.setSpace({
      ...space,
      status: { state: "synced", at_ms },
      last_sync_ms: at_ms,
      keyring_pending: false,
      transports: space.transports.map((t) => ({
        ...t,
        status: { state: "synced", at_ms },
        last_ok_ms: at_ms,
      })),
      devices: space.devices.map((d) => (d.this_device ? { ...d, written_at_ms: at_ms } : d)),
    });
  }

  private newSpace(
    storage: StorageConfig,
    deviceName: string,
    others: SyncSpaceView["devices"],
  ): void {
    const name = this.deviceName(deviceName);
    this.syncRun({
      storage: storageView(storage),
      device_name: name,
      devices: [
        { tag: mockTag(`${name}:${this.now()}`), name, written_at_ms: null, this_device: true },
        ...others,
      ],
      status: { state: "idle" },
      last_sync_ms: null,
      rolled_back: [],
      unreadable: [],
      keyring_pending: false,
      key_saved: true,
      transports: [{ kind: "cloud", status: { state: "idle" }, last_ok_ms: null }],
      lan: null,
    });
  }

  private syncCreate(
    storage: StorageConfig,
    password: string,
    deviceName: string,
  ): { sync_key: string } {
    this.requireUnlocked();
    if (this.space !== null) throw new LockraError("sync_already_on");
    checkStorage(storage);
    this.checkPassword(password);
    this.newSpace(storage, deviceName, []);
    // The key is shown once now; Settings › Sync reminds of it until it is saved.
    this.setSpace({ ...this.requireSpace(), key_saved: false });
    return { sync_key: MOCK_SYNC_KEY };
  }

  private syncJoin(
    source: JoinSource,
    password: string,
    deviceName: string,
    spacePassword?: string,
  ): null {
    if (source.type === "invite") checkInvite(source.text, source.code);
    if (
      source.type === "manual" &&
      !/^LKS1(-?[A-Z2-7]{4}){14}$/i.test(source.sync_key.replace(/\s/g, ""))
    )
      throw new LockraError("sync_key_invalid");
    if (this.state.phase === "locked") throw new LockraError("locked");
    if (this.state.phase === "unlocked" && this.space !== null)
      throw new LockraError("sync_already_on");
    if (this.state.phase === "no_vault") this.checkLength(password);
    else this.checkPassword(password);
    const storage: StorageConfig =
      source.type === "manual"
        ? source.storage
        : {
            kind: "webdav",
            url: "https://dav.example.com/dav/",
            prefix: "lockra",
            username: "me@example.com",
            password: MOCK_STORAGE_SECRET,
          };
    checkStorage(storage);
    // The space's device uses the mock's master password. A vault of its own whose password is
    // another asks for the space's first, as the core does.
    if (
      this.state.phase === "unlocked" &&
      spacePassword === undefined &&
      password !== MOCK_PASSWORD
    )
      throw new LockraError("sync_space_password_needed");
    if ((spacePassword ?? password) !== MOCK_PASSWORD)
      throw new LockraError("sync_wrong_credentials");
    const others = [
      { tag: mockTag("Pixel 8"), name: "Pixel 8", written_at_ms: this.now(), this_device: false },
    ];
    if (this.state.phase === "no_vault") {
      // A new device: the vault comes from the space.
      this.password = password;
      const entries = sampleEntries();
      for (const entry of entries) this.secrets.set(entry.view.id, entry.secret);
      this.enterUnlocked(entries.map((e) => e.view));
    }
    this.newSpace(storage, deviceName, others);
    return null;
  }

  private syncInvite(password: string | undefined, reason: string | undefined): SyncInvite {
    this.requireSpace();
    this.confirmPresence(password, reason);
    return {
      invite: `lockra-invite:1:${btoa(`mock-invite:${this.now()}`)}`,
      svg: placeholderSvg("invite"),
      shared_text: `lockra-invite:2:${btoa(`mock-shared-invite:${this.now()}`)}`,
      code: MOCK_INVITE_CODE,
      sync_key: MOCK_SYNC_KEY,
    };
  }

  /** The master password, or without it the biometric check that unlocks this vault. */
  private confirmPresence(password: string | undefined, reason: string | undefined): void {
    if (password !== undefined) {
      this.checkPassword(password);
      return;
    }
    if (!this.state.lock.device_unlock.biometric.enabled)
      throw new LockraError("biometric_unavailable");
    this.checkUser(reason);
  }

  /** The sync key files saved, for tests (the template with the key in its slot). */
  readonly savedSyncKeys: { fileName: string; text: string }[] = [];

  /** Test hook: the next save dialog is cancelled. */
  cancelNextSave = false;

  /** Test hook: the sync reached `status` (a failure, a run in progress), as the core would
   *  publish it. */
  simulateSync(status: SyncSpaceView["status"]): void {
    if (this.space === null) return;
    this.setSpace({ ...this.space, status });
  }

  private updateBusy(): boolean {
    return ["checking", "downloading", "installing"].includes(this.state.update.status.state);
  }

  /** The core's update run, at once: each status it passes through is published in turn. A check
   *  asks afresh; an install goes on from what a check found or downloaded; the automatic run
   *  stops at `ready`. */
  private runUpdate(run: "check" | "install" | "auto"): null {
    if (this.state.update.method === null) throw new LockraError("update_unavailable");
    // The phone checks only: a newer release opens its page.
    if (this.state.update.method === "android" && run !== "check")
      throw new LockraError("update_unavailable");
    if (this.updateBusy()) throw new LockraError("update_busy");
    const step = (status: UiState["update"]["status"]) => {
      this.state.update = { ...this.state.update, status };
      this.publish();
    };
    const failed = (at: "check" | "download" | "install") => {
      if (this.updateFailure?.step !== at) return false;
      step({ state: "failed", code: this.updateFailure.code, at_ms: this.now() });
      return true;
    };
    let pending = run === "check" ? null : this.pending;
    this.pending = null;
    if (pending === null) {
      step({ state: "checking" });
      if (failed("check")) return null;
      const checked_at_ms = this.now();
      const release = this.release;
      if (release === null) {
        step({ state: "up_to_date", checked_at_ms });
        return null;
      }
      const { version, notes, date } = release;
      step({ state: "available", version, notes, date, checked_at_ms });
      pending = { release, downloaded: false };
      if (run === "check") {
        this.pending = pending;
        return null;
      }
    }
    const { version, size } = pending.release;
    if (!pending.downloaded) {
      for (const received of [0, Math.round(size / 2), size]) {
        step({ state: "downloading", version, received, total: size });
      }
      if (failed("download")) return null;
      step({ state: "ready", version });
    }
    if (run === "auto") {
      this.pending = { release: pending.release, downloaded: true };
      return null;
    }
    step({ state: "installing", version });
    failed("install");
    return null;
  }

  /** Test hook: what the next update check finds. */
  setRelease(release: MockRelease | null): void {
    this.release = release;
  }

  /** Test hook: the updater reached `status` (a download step, a failure), as the core would
   *  publish it. */
  simulateUpdate(status: UiState["update"]["status"]): void {
    this.state.update = { ...this.state.update, status };
    this.publish();
  }

  private unlock(password: string): null {
    if (this.state.phase === "no_vault") throw new LockraError("no_vault");
    if (this.state.phase === "unlocked") return null;
    const retryAt = this.state.lock.retry_at_ms;
    if (retryAt !== null && this.now() < retryAt) throw new LockraError("rate_limited", retryAt);
    if (password !== this.password) {
      const failed = this.state.lock.failed_attempts + 1;
      const delay =
        failed < FREE_ATTEMPTS
          ? 0
          : Math.min(30_000, 1000 * 2 ** Math.min(5, failed - FREE_ATTEMPTS));
      this.state.lock = {
        ...this.state.lock,
        failed_attempts: failed,
        retry_at_ms: delay > 0 ? this.now() + delay : null,
      };
      this.publish();
      throw new LockraError("wrong_password", delay > 0 ? this.now() + delay : undefined);
    }
    this.enterUnlocked(this.lockedEntries);
    return null;
  }

  private enterUnlocked(entries: EntryView[]): void {
    this.state.phase = "unlocked";
    this.state.entries = entries;
    this.state.collapsed_groups = [...this.collapsed];
    this.state.sync = { space: this.space, joining: null };
    this.lockedEntries = [];
    this.state.lock = { ...this.state.lock, failed_attempts: 0, retry_at_ms: null };
    this.refreshAutoLock();
    this.publish();
    this.pushFrame();
  }

  private lock(): void {
    if (this.state.phase !== "unlocked") return;
    this.lockedEntries = this.state.entries;
    this.state.entries = [];
    this.state.collapsed_groups = [];
    this.state.phase = "locked";
    this.state.sync = { space: null, joining: null };
    this.exports.clear();
    this.clearImport();
    this.state.auto_lock_at_ms = null;
    this.publish();
    this.pushFrame();
  }

  private restore(password: string, mode: "merge" | "replace"): null {
    const entries = this.restoreEntries;
    if (entries === null) throw new LockraError("no_restore");
    if (password !== MOCK_PASSWORD && password !== this.password)
      throw new LockraError("wrong_password");
    this.state.restore = null;
    this.restoreEntries = null;
    for (const entry of entries) this.secrets.set(entry.view.id, entry.secret);
    if (this.state.phase === "no_vault") {
      this.password = password;
      this.enterUnlocked(entries.map((e) => e.view));
      this.notice({ type: "restored", entries: entries.length });
      return null;
    }
    this.requireUnlocked();
    if (mode === "replace") {
      this.state.entries = entries.map((e) => e.view);
      this.changed();
      this.notice({ type: "restored", entries: entries.length });
      return null;
    }
    for (const entry of entries) {
      this.addCandidate(
        { type: "file", name: "lockra-auto-20260928-091500.lockrabackup" },
        "backup",
        entry.view.issuer,
        entry.view.account,
        entry.secret,
        entry.view.kind,
        entry.view.digits,
      );
    }
    this.publishImport();
    return null;
  }

  private exportStart(
    target: ExportTarget,
    entryIds: readonly string[],
    password: string,
  ): ExportStarted {
    this.requireUnlocked();
    const chosen = entryIds
      .map((id) => this.state.entries.find((e) => e.id === id))
      .filter((e): e is EntryView => e !== undefined);
    if (chosen.length === 0) throw new LockraError("export_nothing");
    this.checkPassword(password);
    const excluded = chosen
      .filter((e) => e.export[target] !== null)
      .map((e) => ({ entry_id: e.id, reason: e.export[target] ?? "too_large" }));
    const fit = chosen.filter((e) => e.export[target] === null);
    if (fit.length === 0) throw new LockraError("export_nothing");
    const groups: string[][] = [];
    if (target === "google") {
      for (let i = 0; i < fit.length; i += EXPORT_PER_CODE)
        groups.push(fit.slice(i, i + EXPORT_PER_CODE).map((e) => e.id));
    } else {
      for (const entry of fit) groups.push([entry.id]);
    }
    const session = nextId();
    this.exports.set(session, {
      target,
      pages: groups.map((ids) => ({ svg: placeholderSvg(ids.join()), entryIds: ids })),
    });
    return { session, target, pages: groups.length, excluded };
  }

  private importText(text: string, source: CandidateView["source"]): null {
    const lines = text
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter((l) => l !== "" && !l.startsWith("#"));
    if (lines.length === 0) throw new LockraError("import_empty");
    lines.forEach((line, index) => {
      const parsed = parseOtpauth(line);
      if (parsed) {
        this.addCandidate(
          source,
          "uri",
          parsed.issuer,
          parsed.account,
          parsed.secret,
          parsed.kind,
          parsed.digits,
        );
      } else {
        this.candidates.push({
          secret: "",
          view: {
            id: this.candidates.length,
            source,
            origin: "uri",
            issuer: "",
            account: "",
            kind: null,
            algorithm: null,
            digits: null,
            line: index + 1,
            status: { type: "unsupported", reason: "not_otpauth" },
            default_action: "skip",
          },
        });
      }
    });
    this.publishImport();
    return null;
  }

  private addCandidate(
    source: CandidateView["source"],
    origin: CandidateView["origin"],
    issuer: string,
    account: string,
    secret: string,
    kind: OtpKind,
    digits: number,
  ): void {
    const existing = this.state.entries.find((e) => this.secrets.get(e.id) === secret);
    const duplicate = this.candidates.some((c) => c.secret === secret);
    const conflict = this.state.entries.find(
      (e) =>
        e.issuer.toLowerCase() === issuer.toLowerCase() &&
        e.account.toLowerCase() === account.toLowerCase(),
    );
    const status: CandidateView["status"] = existing
      ? { type: "exists", entry_id: existing.id }
      : duplicate
        ? { type: "duplicate" }
        : conflict
          ? { type: "conflict", entry_id: conflict.id }
          : { type: "new" };
    const action: CandidateAction =
      status.type === "new" || status.type === "conflict" ? "add" : "skip";
    this.candidates.push({
      secret,
      view: {
        id: this.candidates.length,
        source,
        origin,
        issuer,
        account,
        kind,
        algorithm: "sha1",
        digits,
        line: null,
        status,
        default_action: action,
      },
    });
  }

  private commit(choices: readonly Choice[]): ImportOutcome {
    this.requireUnlocked();
    if (this.state.import === null) throw new LockraError("no_import");
    const outcome: ImportOutcome = { added: 0, replaced: 0, skipped: 0 };
    for (const candidate of this.candidates) {
      const view = candidate.view;
      const action = choices.find((c) => c.id === view.id)?.action ?? view.default_action;
      if (
        view.kind === null ||
        view.digits === null ||
        view.status.type === "unsupported" ||
        view.status.type === "exists" ||
        view.status.type === "duplicate" ||
        action === "skip"
      ) {
        outcome.skipped += 1;
      } else if (action === "replace" && view.status.type === "conflict") {
        const target = this.state.entries.find(
          (e) => e.id === (view.status.type === "conflict" ? view.status.entry_id : ""),
        );
        if (target) {
          this.secrets.set(target.id, candidate.secret);
          target.updated_at_ms = this.now();
          outcome.replaced += 1;
        }
      } else {
        this.addEntry(
          view.issuer,
          view.account,
          candidate.secret,
          view.kind,
          view.algorithm ?? "sha1",
          view.digits,
          view.origin,
          null,
          false,
        );
        outcome.added += 1;
      }
    }
    this.clearImport();
    this.changed();
    this.notice({ type: "imported", ...outcome });
    return outcome;
  }

  private addEntry(
    issuer: string,
    account: string,
    secret: string,
    kind: OtpKind,
    algorithm: EntryView["algorithm"],
    digits: number,
    origin: EntryView["origin"],
    group: string | null,
    publish = true,
  ): string {
    if (
      digits < 6 ||
      digits > 8 ||
      (kind.type === "totp" && (kind.period < 1 || kind.period > 3600))
    )
      throw new LockraError("invalid_parameters");
    if (
      [...this.secrets.values()].includes(secret) &&
      this.state.entries.some((e) => this.secrets.get(e.id) === secret)
    )
      throw new LockraError("duplicate_entry");
    const entry = mockEntry(issuer, account, {
      kind,
      algorithm,
      digits,
      origin,
      group,
      secret,
      at: this.now(),
    });
    this.secrets.set(entry.view.id, secret);
    this.state.entries = [...this.state.entries, entry.view];
    if (publish) this.changed();
    return entry.view.id;
  }

  private entry(id: string): EntryView {
    this.requireUnlocked();
    const entry = this.state.entries.find((e) => e.id === id);
    if (!entry) throw new LockraError("entry_not_found");
    return entry;
  }

  private requireUnlocked(): void {
    if (this.state.phase === "locked") throw new LockraError("locked");
    if (this.state.phase === "no_vault") throw new LockraError("no_vault");
  }

  /** The reasons the biometric checks were shown, for tests. */
  readonly biometricReasons: string[] = [];

  /** Test hook: how the next biometric checks answer. */
  answerBiometric(answer: ErrorCode | null): void {
    this.biometricAnswer = answer;
  }

  private checkUser(reason: string | undefined): void {
    this.biometricReasons.push(reason ?? "");
    if (this.state.lock.device_unlock.biometric.kind === null)
      throw new LockraError("biometric_unavailable");
    if (this.biometricAnswer !== null) throw new LockraError(this.biometricAnswer);
  }

  private checkPassword(password: string): void {
    if (password !== this.password) throw new LockraError("wrong_password");
  }

  private checkLength(password: string): void {
    // Characters, as the core counts them (code points), not UTF-16 units.
    if (Array.from(password).length < MIN_PASSWORD) throw new LockraError("password_too_short");
  }

  private clearImport(): void {
    this.candidates = [];
    this.batches = [];
    this.awaiting = null;
    this.state.import = null;
  }

  private publishImport(): void {
    this.state.import = {
      candidates: this.candidates.map((c) => c.view),
      google_batches: this.batches,
      awaiting_password: this.awaiting,
    };
    this.publish();
  }

  /** Test hook: the last automatic backup failed with `code`. */
  failAutoBackup(code: ErrorCode): void {
    this.state.backup = { ...this.state.backup, last_auto_error: { code, at_ms: this.now() } };
    this.publish();
  }

  /** Test hook: put a Lockra backup into the import, waiting for its password. */
  awaitBackup(name: string): void {
    this.awaiting = name;
    this.publishImport();
  }

  private refreshAutoLock(): void {
    const minutes = this.state.settings.auto_lock_minutes;
    this.state.auto_lock_at_ms =
      this.state.phase === "unlocked" && minutes > 0 ? this.now() + minutes * 60_000 : null;
  }

  private changed(): void {
    this.refreshAutoLock();
    this.publish();
    this.pushFrame();
  }

  private publish(): void {
    const state = structuredClone(this.state);
    for (const listener of this.listeners) listener({ type: "state", state });
  }

  private notice(notice: Notice): void {
    for (const listener of this.listeners) listener({ type: "notice", notice });
  }

  private frame(): CodesFrame {
    const now = this.now();
    const codes: CodeView[] =
      this.state.phase !== "unlocked"
        ? []
        : this.state.entries.map((entry) => {
            const secret = this.secrets.get(entry.id) ?? "";
            if (entry.kind.type === "hotp") {
              return {
                entry_id: entry.id,
                code: fakeCode(secret, entry.kind.counter, entry.digits),
                next_code: null,
                valid_from_ms: null,
                valid_until_ms: null,
              };
            }
            const step = entry.kind.period * 1000;
            const counter = Math.floor(now / step);
            return {
              entry_id: entry.id,
              code: fakeCode(secret, counter, entry.digits),
              next_code: fakeCode(secret, counter + 1, entry.digits),
              valid_from_ms: counter * step,
              valid_until_ms: (counter + 1) * step,
            };
          });
    return { at_ms: now, codes };
  }

  private pushFrame(): void {
    if (this.frameListeners.size === 0) return;
    const frame = this.frame();
    for (const listener of this.frameListeners) listener(frame);
    this.scheduleFrame();
  }

  private scheduleFrame(): void {
    if (this.frameTimer !== undefined) clearTimeout(this.frameTimer);
    const ends = this.frame()
      .codes.map((c) => c.valid_until_ms)
      .filter((v): v is number => v !== null);
    if (ends.length === 0) {
      this.frameTimer = undefined;
      return;
    }
    const delay = Math.max(0, Math.min(...ends) - this.now()) + 1;
    this.frameTimer = setTimeout(() => {
      this.frameTimer = undefined;
      this.pushFrame();
    }, delay);
  }
}
