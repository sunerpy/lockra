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
  type Unsubscribe,
} from "./backend";
import {
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
  type Notice,
  type OtpKind,
  type Platform,
  type ResultOf,
  type Revealed,
  type Settings,
  type UiCommand,
  type UiState,
  defaultSettings,
} from "./schema";

export interface MockEntry {
  view: EntryView;
  /** A stand-in secret (Base32 letters); only ever shown by `entry_reveal`. */
  secret: string;
}

export interface MockOptions {
  phase?: UiState["phase"];
  entries?: MockEntry[];
  password?: string;
  settings?: Partial<Settings>;
  platform?: Platform;
  keychainAvailable?: boolean;
  deviceUnlock?: boolean;
  /** Text the clipboard import reads. */
  clipboard?: string;
  now?: () => number;
}

export const MOCK_PASSWORD = "correct horse battery";
const MIN_PASSWORD = 8;
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
      "kind" | "algorithm" | "digits" | "group" | "favorite" | "origin" | "last_used_at_ms"
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
      origin: options.origin ?? "uri",
      created_at_ms: at,
      updated_at_ms: at,
      last_used_at_ms: options.last_used_at_ms ?? null,
      export: exportCompat(kind, algorithm, digits),
    },
  };
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
    mockEntry("Bank", "6222 •••• 1234", { kind: { type: "hotp", counter: 12 }, origin: "manual" }),
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
  private readonly now: () => number;
  /** Every command dispatched, for tests. */
  readonly calls: UiCommand[] = [];

  constructor(options: MockOptions = {}) {
    this.now = options.now ?? (() => Date.now());
    this.password = options.password ?? MOCK_PASSWORD;
    this.clipboard = options.clipboard;
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
        },
        failed_attempts: 0,
        retry_at_ms: null,
      },
      entries: phase === "unlocked" ? entries.map((e) => e.view) : [],
      settings: { ...defaultSettings(), ...options.settings },
      import: null,
      backup: { last_backup_ms: null, last_auto_file: null, last_auto_error: null },
      restore: null,
      auto_lock_at_ms: null,
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

  /** Test hook: fire a notice as the core would. */
  emitNotice(notice: Notice): void {
    this.notice(notice);
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
        const { issuer, account, group, favorite } = command.patch;
        if (issuer !== undefined) entry.issuer = issuer.trim();
        if (account !== undefined) entry.account = account.trim();
        if (group !== undefined) entry.group = group.trim() === "" ? null : group.trim();
        if (favorite !== undefined) entry.favorite = favorite;
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
        this.state.settings = {
          ...settings,
          font_size_px: Math.min(18, Math.max(12, settings.font_size_px)),
        };
        this.refreshAutoLock();
        this.publish();
        return null;
      }
      case "activity":
        this.refreshAutoLock();
        return null;
    }
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
    this.state.phase = "locked";
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
