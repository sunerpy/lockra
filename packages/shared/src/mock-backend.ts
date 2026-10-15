// An in-memory stand-in for the Rust core: the browser preview (`pnpm dev` outside Tauri) and the
// page tests run on it. It follows the core's rules closely enough for the UI to be exercised
// (phases, rate limiting, previews, export sessions, backups) but computes stand-in codes from a
// hash, not HMAC: it never holds a real account. Loaded under `import.meta.env.DEV` only; the
// release bundle check fails if it ships.
import {
  type Backend,
  type EventListener,
  type ExportItemsFile,
  type FrameListener,
  type ImportPickKind,
  LockraError,
  type SaveSyncKey,
  type ScanJoin,
  type ScanTexts,
  type Unsubscribe,
} from "./backend";
import {
  type ActivityView,
  BROWSER_IDS,
  type BiometricKind,
  type BrowserId,
  type BrowserStatus,
  type BrowserView,
  type ConfirmView,
  type PairRequestView,
  type CandidateAction,
  type CandidateView,
  type Choice,
  type CodesFrame,
  type CodeView,
  type CommandName,
  type CommandOf,
  type CopyPart,
  defaultSettings,
  type EntryView,
  type ErrorCode,
  type ExportPage,
  type ExportStarted,
  type ExportTarget,
  type FieldChange,
  type GeneratedPastView,
  type GeneratedView,
  type GeneratorOptions,
  type GoogleBatchView,
  type ImportOutcome,
  type AwaitingView,
  type Incompatible,
  type InstallMethod,
  type ItemDraft,
  type ItemsExportFormat,
  type ItemPatch,
  type ItemRevealed,
  type ItemView,
  type JoinSource,
  MARK_CHARS,
  MAX_DEVICE_NAME_CHARS,
  type Notice,
  type OtpKind,
  type PastView,
  type PinnedGroup,
  type Platform,
  type ResultOf,
  type Revealed,
  type RevealParts,
  type ScoreView,
  type SecretChange,
  type Settings,
  type SiteRuleView,
  type StorageConfig,
  type StorageView,
  type SyncInvite,
  type SyncSpaceView,
  type UiCommand,
  type UiState,
} from "./schema";
import { CONFIRM_TTL_MS, PAIR_TTL_MS, defaultBrowserPolicy, riskierPolicy } from "./browser";
import { MIN_PASSWORD } from "./password";
import { isSealedInvite } from "./storage-form";

export interface MockEntry {
  view: EntryView;
  /** A stand-in secret (Base32 letters); only ever shown by `entry_reveal`. */
  secret: string;
}

/** A camera scan: the QR code's text, `null` when left without one, or how it failed. */
export type MockScan = string | null | { error: ErrorCode };

export interface MockOptions {
  phase?: UiState["phase"];
  /** The build offers password management (off by default, as a release build before it ships). */
  passwords?: boolean;
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
  /** The folder the sync folder dialog chooses; `null`: it is cancelled. */
  folder?: string | null;
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
  /** The groups folded in the code list (shown once unlocked). */
  collapsedGroups?: string[];
  /** The group pinned at the top of the code list (shown once unlocked). */
  pinnedGroup?: PinnedGroup | null;
  /** The logins (shown once unlocked; password management must be on). */
  items?: MockItem[];
  /** The browser extension's pairings, waiting requests, policy and activity (shown once
   *  unlocked). */
  browser?: Partial<Pick<BrowserView, "paired" | "pairing" | "confirms" | "policy" | "activity">>;
}

/** A login of the mock: its view and its secrets. */
export interface MockItem {
  view: ItemView;
  password?: string;
  notes?: string;
  /** Hidden custom fields' values, by field id. */
  hidden?: Record<string, string>;
  history?: PastView[];
}

/** A login's view with every part at its default but `title` (and what `fields` adds). */
export function mockItemView(title: string, fields: Partial<ItemView> = {}): ItemView {
  return {
    id: nextId(),
    kind: "login",
    title,
    username: "",
    email: "",
    has_password: false,
    urls: [],
    has_notes: false,
    tags: [],
    group: null,
    favorite: false,
    color: "auto",
    mark: null,
    protection: "fill",
    autofill: { on_page_load: "default", auto_submit: "default", fill_otp: "default" },
    totp_entry: null,
    fields: [],
    history_count: 0,
    conflicts: [],
    source: null,
    created_at_ms: 1_790_000_000_000,
    updated_at_ms: 1_790_000_000_000,
    last_used_at_ms: null,
    read_only: false,
    ...fields,
  };
}

/** A login of a password manager's export, as the mock's import adds it. */
function importedLogin(title: string, username: string, url: string, password: string): MockItem {
  return {
    view: mockItemView(title, {
      username,
      has_password: true,
      urls: [{ url, matching: "domain" }],
      source: "bitwarden",
    }),
    password,
  };
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
/** The key file `pickKeyFile` chooses. */
export const MOCK_KEY_FILE = "Passwords.keyx";
/** The storage secret the mock accepts (any other is refused, like wrong credentials). */
export const MOCK_STORAGE_SECRET = "storage secret";
/** The folder the mock's dialog chooses unless told otherwise. */
export const MOCK_SYNC_FOLDER = "/Users/me/Dropbox/Lockra";
/** What the mock's invitation of a space in a folder holds: the sync key alone. */
const KEY_ONLY = "mock-key-only-invite";
/** What a mock invitation to a relay holds before the relay's address. */
const RELAY_INVITE = "mock-relay-invite|";
/** What a mock invitation of Lockra up to 0.8.2 holds: no data key. */
const OLD_INVITE = "mock-old-invite";
/** An invitation of Lockra up to 0.8.2, which the mock refuses as the core does. */
export const MOCK_OLD_INVITE = `lockra-invite:1:${btoa(OLD_INVITE)}`;
/** The one-time code of the mock's sealed invitation. */
export const MOCK_INVITE_CODE = "7K2QM-XW4FD";
/** The sync key of the mock's spaces. */
export const MOCK_SYNC_KEY =
  "LKS1-MFRG-GZDF-MZTW-Q2LK-NNWG-23TP-OBYX-E43U-OR3W-C6DZ-PI2D-AMBR-GQ2D-ARQA";
const FREE_ATTEMPTS = 3;
const EXPORT_PER_CODE = 10;

/** A stand-in for zxcvbn: the common passwords score 0, then length and variety count. */
function mockScore(password: string): ScoreView {
  if (["password", "123456", "qwerty", "letmein"].includes(password.toLowerCase()))
    return { score: 0, warning: "top_ten" };
  const classes = [/[a-z]/, /[A-Z]/, /[0-9]/, /[^a-zA-Z0-9]/].filter((r) =>
    r.test(password),
  ).length;
  const score = Math.max(0, Math.min(4, Math.floor(password.length / 4) + classes - 3));
  return { score, warning: score < 2 ? "common" : null };
}

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

/** A day of September 2026, the samples' time. */
const sampleDay = (d: number) => Date.UTC(2026, 8, d);

/** A sample login: `fields` over the defaults, with the secrets the mock keeps. */
const sampleLogin = (
  title: string,
  fields: Partial<ItemView>,
  secrets: Omit<MockItem, "view"> = {},
): MockItem => ({
  view: mockItemView(title, {
    has_password: secrets.password !== undefined,
    has_notes: (secrets.notes ?? "") !== "",
    ...fields,
  }),
  ...secrets,
});

/** The browser extension as boards 13–16 show it: Chrome connected, Edge paired before, recent
 *  activity with a refusal and a pause (for the browser preview's `?browser`). */
export function sampleBrowser(now: number = Date.now()): NonNullable<MockOptions["browser"]> {
  const minute = 60_000;
  const activity = (
    ago: number,
    browser: string,
    site: string,
    event: ActivityView["event"],
    title: string | null,
    refusal: ActivityView["refusal"] = null,
  ): ActivityView => ({
    at_ms: now - ago * minute,
    browser,
    label: "",
    site,
    event,
    refusal,
    item_id: null,
    title,
  });
  return {
    paired: [
      {
        id: nextId(),
        browser: "chrome",
        label: "",
        extension: "egbpficliljjnpkjbigonldakfllebco",
        paired_at_ms: now - 3 * 24 * 60 * minute,
        last_seen_ms: now,
        connected: true,
        page_access: true,
        paused: false,
      },
      {
        id: nextId(),
        browser: "edge",
        label: "",
        extension: "egbpficliljjnpkjbigonldakfllebco",
        paired_at_ms: now - 10 * 24 * 60 * minute,
        last_seen_ms: now - 2 * 24 * 60 * minute,
        connected: false,
        page_access: false,
        paused: false,
      },
    ],
    activity: [
      activity(1, "chrome", "github.com", "filled", "GitHub"),
      activity(1, "chrome", "github.com", "otp_filled", "GitHub"),
      activity(6, "chrome", "bank.example.cn", "refused", null, "never_fill"),
      activity(12, "edge", "figma.com", "saved", "Figma"),
      activity(40, "edge", "", "paused", null),
    ],
  };
}

/** The sample logins (board 01 of Penpot 「Lockra 密码管理」): twelve, in 工作, 个人 and none, three
 *  favourites; GitHub, AWS, Cloudflare and Google go with the accounts of `entries` of their name,
 *  the bank asks for proof, V2EX holds a conflict. */
export function sampleItems(entries: readonly MockEntry[] = []): MockItem[] {
  const code = (issuer: string) => entries.find((e) => e.view.issuer === issuer)?.view.id ?? null;
  const recovery = nextId();
  return [
    sampleLogin(
      "GitHub",
      {
        username: "octocat",
        email: "octocat@github.com",
        urls: [{ url: "https://github.com/login", matching: "domain" }],
        group: "工作",
        tags: ["开发", "常用"],
        favorite: true,
        totp_entry: code("GitHub"),
        fields: [
          {
            id: recovery,
            label: "恢复码",
            hidden: true,
            value: null,
            has_value: true,
            page_field: null,
          },
        ],
        history_count: 2,
        last_used_at_ms: sampleDay(28),
        created_at_ms: sampleDay(1),
        updated_at_ms: sampleDay(20),
      },
      {
        password: "x7#Kq9!vLm2@Rt4z",
        notes: "公司组织 acme-corp 的管理员。\n恢复码打印件在抽屉里。",
        hidden: { [recovery]: "a1b2-c3d4-e5f6" },
        history: [
          {
            id: "gh-1",
            field: "password",
            value: "OldPass#2025",
            set_at_ms: sampleDay(10),
            conflict: false,
          },
          {
            id: "gh-2",
            field: "password",
            value: "Older!2024",
            set_at_ms: sampleDay(2),
            conflict: false,
          },
        ],
      },
    ),
    sampleLogin(
      "AWS 控制台",
      {
        username: "root@acme-corp",
        urls: [{ url: "https://signin.aws.amazon.com", matching: "host" }],
        group: "工作",
        tags: ["开发"],
        totp_entry: code("AWS"),
        protection: "confirm",
      },
      { password: "Aws!Root-2026-q3" },
    ),
    sampleLogin(
      "Cloudflare",
      {
        email: "ops@acme.dev",
        urls: [{ url: "https://dash.cloudflare.com", matching: "domain" }],
        group: "工作",
        tags: ["开发"],
        totp_entry: code("Cloudflare"),
      },
      { password: "cf-Ops_93kd!a" },
    ),
    sampleLogin(
      "Figma",
      {
        email: "alex@acme.dev",
        urls: [{ url: "https://www.figma.com", matching: "domain" }],
        group: "工作",
        tags: ["设计"],
        last_used_at_ms: sampleDay(29),
      },
      { password: "Figma#Design42" },
    ),
    sampleLogin(
      "Notion",
      {
        email: "alex@acme.dev",
        urls: [{ url: "https://www.notion.so", matching: "domain" }],
        group: "工作",
        tags: ["常用"],
      },
      { password: "n0tion-Pages!7" },
    ),
    sampleLogin(
      "招商银行",
      {
        username: "6222 •••• 1234",
        urls: [{ url: "https://pbsz.ebank.cmbchina.com", matching: "host" }],
        group: "个人",
        favorite: true,
        protection: "confirm",
        color: "red",
        mark: "招",
      },
      { password: "Cmb#880613" },
    ),
    sampleLogin(
      "Google",
      {
        email: "alex@gmail.com",
        urls: [{ url: "https://accounts.google.com", matching: "domain" }],
        group: "个人",
        favorite: true,
        tags: ["常用"],
        totp_entry: code("Google"),
      },
      { password: "g00gle-Alex!92" },
    ),
    sampleLogin(
      "Apple ID",
      {
        email: "alex@icloud.com",
        urls: [{ url: "https://appleid.apple.com", matching: "domain" }],
        group: "个人",
      },
      { password: "Apple-Id#2026" },
    ),
    sampleLogin(
      "Steam",
      {
        username: "player-one",
        urls: [{ url: "https://store.steampowered.com", matching: "domain" }],
        group: "个人",
        tags: ["游戏"],
      },
      { password: "Steam!Play3r" },
    ),
    sampleLogin(
      "路由器",
      { username: "admin", urls: [{ url: "http://192.168.1.1", matching: "host" }] },
      { password: "router-admin" },
    ),
    sampleLogin(
      "Netflix",
      { email: "alex@gmail.com", urls: [{ url: "https://www.netflix.com", matching: "domain" }] },
      { password: "Netfl1x&Chill" },
    ),
    sampleLogin(
      "V2EX",
      {
        username: "alex",
        urls: [{ url: "https://www.v2ex.com", matching: "domain" }],
        conflicts: [{ field: "password", purpose: "password", label: "" }],
        history_count: 1,
      },
      {
        password: "v2ex-New!2026",
        history: [
          {
            id: "v2-1",
            field: "password",
            value: "v2ex-Other#99",
            set_at_ms: sampleDay(27),
            conflict: true,
          },
        ],
      },
    ),
  ];
}

interface MockCandidate {
  view: CandidateView;
  secret: string;
  /** A login's: what the commit adds (its code linked by `view.login.totp`). */
  login?: MockItem;
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
    keyring_unsealed: false,
    key_saved: true,
    ...overrides,
  };
}

/** The core's checks of a storage configuration (lockra-sync `StorageConfig::validate`); a folder
 *  is the one `chosen`. */
function checkStorage(storage: StorageConfig, chosen: string | null): void {
  if (storage.kind === "folder") {
    if (chosen === null) throw new LockraError("sync_folder_not_chosen");
    return;
  }
  if (storage.kind === "relay") {
    // Its address alone: HTTPS (plain HTTP to this computer only), no credential, query or anchor.
    let url: URL;
    try {
      url = new URL(storage.url.trim());
    } catch {
      throw new LockraError("sync_config_invalid");
    }
    if (url.username !== "" || url.password !== "" || url.search !== "" || url.hash !== "")
      throw new LockraError("sync_config_invalid");
    const loopback = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
    if (url.protocol === "http:" && !loopback) throw new LockraError("sync_insecure");
    if (url.protocol !== "https:" && url.protocol !== "http:")
      throw new LockraError("sync_config_invalid");
    return;
  }
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

/** What a mock invitation (either text) holds. */
function held(text: string): string {
  try {
    return atob(text.slice("lockra-invite:1:".length));
  } catch {
    return "";
  }
}

/** A mock invitation (either text) that holds the sync key alone. */
function keyOnly(text: string): boolean {
  return held(text).startsWith(KEY_ONLY);
}

/** The relay a mock invitation names, if it names one. */
function invitedRelay(text: string): string | undefined {
  const inside = held(text);
  if (!inside.startsWith(RELAY_INVITE)) return undefined;
  return inside.slice(RELAY_INVITE.length).split("\n")[0];
}

function storageView(storage: StorageConfig, chosen: string | null): StorageView {
  if (storage.kind === "folder") return { kind: "folder", path: chosen ?? "" };
  if (storage.kind === "relay") return { kind: "relay", url: storage.url.trim() };
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
  private awaiting: AwaitingView | null = null;
  private exports = new Map<string, MockExport>();
  private restoreEntries: MockEntry[] | null = null;
  private clipboard: string | undefined;
  private scan: MockScan;
  /** What the sync folder dialog chooses, and what it chose last. */
  private folderPick: string | null;
  private chosenFolder: string | null = null;
  private readonly now: () => number;
  private release: MockRelease | null;
  private updateFailure: MockOptions["updateFailure"];
  private releasePageOpens: boolean;
  /** What the last run found, and whether its package is downloaded (the core's `Pending`). */
  private pending: { release: MockRelease; downloaded: boolean } | null = null;
  /** The sync space, kept here while locked (the core keeps it in the vault). */
  private space: SyncSpaceView | null;
  /** The folded groups, kept here while locked (the core keeps them in the vault). */
  private collapsed: string[];
  /** The accounts and the groups in the order they were dragged into, as the core keeps them. */
  private order: string[] = [];
  private groupOrder: string[] = [];
  private pinned: PinnedGroup | null;
  /** The last join was by the fingerprint: the space's keyring waits for the master password. */
  private joinedUnsealed = false;
  private biometricAnswer: ErrorCode | null;
  private readonly passwords: boolean;
  private items: MockItem[];
  private siteRules: SiteRuleView[] = [];
  private generated: { password: string; site: string | null } | null = null;
  private generatedHistory: GeneratedPastView[] = [];
  private itemFolds: string[] = [];
  private browser: Pick<BrowserView, "paired" | "pairing" | "confirms" | "policy" | "activity">;
  /** The browsers whose manifests the mock's shell has written. */
  private manifests = new Set<BrowserId>();
  /** The shell could not register the quick search's shortcut (the core keeps it in memory). */
  private shortcutError: ErrorCode | null = null;
  /** Every command dispatched, for tests. */
  readonly calls: UiCommand[] = [];
  /** Every quick search's end the webview told the shell of (`quickAccessDone`), for tests. */
  readonly quickAccessEnds: boolean[] = [];

  constructor(options: MockOptions = {}) {
    this.now = options.now ?? (() => Date.now());
    this.password = options.password ?? MOCK_PASSWORD;
    this.clipboard = options.clipboard;
    this.scan = options.scan ?? null;
    this.folderPick = options.folder === undefined ? MOCK_SYNC_FOLDER : options.folder;
    this.release = options.release ?? null;
    this.updateFailure = options.updateFailure;
    this.releasePageOpens = options.releasePageOpens ?? true;
    this.space = options.sync ?? null;
    this.collapsed = options.collapsedGroups ?? [];
    this.pinned = options.pinnedGroup ?? null;
    this.biometricAnswer = options.biometricAnswer ?? null;
    this.passwords = options.passwords ?? false;
    this.items = options.items ?? [];
    this.browser = {
      paired: options.browser?.paired ?? [],
      pairing: options.browser?.pairing ?? [],
      confirms: options.browser?.confirms ?? [],
      policy: options.browser?.policy ?? defaultBrowserPolicy(),
      activity: options.browser?.activity ?? [],
    };
    const entries = options.entries ?? [];
    for (const entry of entries) this.secrets.set(entry.view.id, entry.secret);
    const phase = options.phase ?? (entries.length > 0 ? "unlocked" : "no_vault");
    this.state = {
      app_version: "0.1.0",
      platform: options.platform ?? "linux",
      features: { passwords: options.passwords ?? false },
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
      items_rev: 0,
      items_count: phase === "unlocked" ? this.items.length : 0,
      site_rules: [],
      item_collapsed_groups: [],
      generator: { history_count: 0 },
      browser: {
        available: false,
        enabled: false,
        paired: [],
        pairing: [],
        confirms: [],
        policy: defaultBrowserPolicy(),
        activity: [],
      },
      quick_access: { shortcut: null, error: null },
      collapsed_groups: phase === "unlocked" ? [...this.collapsed] : [],
      entry_order: [],
      group_order: [],
      pinned_group: phase === "unlocked" ? this.pinned : null,
      settings: { ...defaultSettings(), ...options.settings },
      import: null,
      backup: { last_backup_ms: null, last_auto_file: null, last_auto_error: null },
      restore: null,
      auto_lock_at_ms: null,
      update: { method: options.updateMethod ?? null, status: { state: "idle" } },
      sync: { space: phase === "unlocked" ? this.space : null },
    };
    this.lockedEntries = phase === "unlocked" ? [] : entries.map((e) => e.view);
    this.state.browser = this.browserView();
    this.state.quick_access = this.quickAccessView();
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
      { type: "invite", text: this.scan, storage: join.storage },
      join.password,
      join.deviceName,
      join.spacePassword,
      join.reason,
    );
    return true;
  }

  async pickImportFiles(kind?: ImportPickKind): Promise<boolean> {
    this.requireUnlocked();
    if (kind === "logins") {
      this.requirePasswords();
      this.addLoginCandidates({ type: "file", name: "bitwarden_export.json" });
      this.publishImport();
      return true;
    }
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

  async pickKeyFile(): Promise<boolean> {
    this.requireUnlocked();
    this.requirePasswords();
    if (this.awaiting?.kind !== "kdbx") throw new LockraError("no_import");
    this.awaiting = { ...this.awaiting, key_file: MOCK_KEY_FILE };
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

  async exportOtpauthFile(
    entryIds: readonly string[],
    password?: string,
    reason?: string,
  ): Promise<string | null> {
    this.requireUnlocked();
    this.confirmPresence(password, reason);
    if (!entryIds.some((id) => this.state.entries.some((e) => e.id === id)))
      throw new LockraError("export_nothing");
    return "lockra-export.txt";
  }

  /** The exports of logins made, for tests: which logins and the format (no file, as in Rust). */
  readonly exportedItems: { ids: string[]; format: ItemsExportFormat }[] = [];

  async exportItemsFile(args: ExportItemsFile): Promise<string | null> {
    this.requirePasswords();
    const ids = args.ids.filter((id) =>
      this.items.some((i) => i.view.id === id && !i.view.read_only),
    );
    if (ids.length === 0) throw new LockraError("export_nothing");
    // What the user chose is checked before they are asked to prove they are there.
    if (args.format === "csv" && !args.acknowledged)
      throw new LockraError("export_acknowledge_needed");
    if (args.format === "kdbx" && Array.from(args.kdbxPassword ?? "").length < MIN_PASSWORD)
      throw new LockraError("password_too_short");
    this.confirmPresence(args.password, args.reason);
    if (this.cancelNextSave) {
      this.cancelNextSave = false;
      return null;
    }
    this.exportedItems.push({ ids, format: args.format });
    return args.format === "kdbx" ? "lockra-logins.kdbx" : "lockra-logins.csv";
  }

  /** Test hook: what the clipboard holds for `import_clipboard`. */
  setClipboard(text: string | undefined): void {
    this.clipboard = text;
  }

  /** Test hook: what was last put on the clipboard (`sync_invite_copy`). */
  get clipboardText(): string | undefined {
    return this.clipboard;
  }

  async openRelease(): Promise<string | null> {
    if (this.releasePageOpens) return null;
    if (this.state.update.method === "play")
      return "https://play.google.com/store/apps/details?id=dev.lockra.mobile";
    const status = this.state.update.status;
    const releases = "https://github.com/sunerpy/lockra/releases";
    return status.state === "available"
      ? `${releases}/tag/v${status.version}`
      : `${releases}/latest`;
  }

  async pickSyncFolder(): Promise<string | null> {
    if (this.folderPick === null) return null;
    this.chosenFolder = this.folderPick;
    return this.folderPick;
  }

  /** The mock's browsers: Chrome, Edge and Firefox have profiles here. */
  private browserStatuses(): BrowserStatus[] {
    return BROWSER_IDS.map((browser) => ({
      browser,
      installed: browser === "chrome" || browser === "edge" || browser === "firefox",
      manifest: this.manifests.has(browser),
    }));
  }

  private requireBrowserShell(): void {
    if (!this.passwords) throw new LockraError("feature_unavailable");
    if (this.state.platform === "android") throw new LockraError("browser_unavailable");
  }

  async browserEnable(browsers: readonly BrowserId[]): Promise<BrowserStatus[]> {
    this.requireBrowserShell();
    for (const browser of browsers) this.manifests.add(browser);
    this.state.settings = { ...this.state.settings, browser_integration: true };
    this.browserChanged();
    return this.browserStatuses();
  }

  async browserDisable(): Promise<BrowserStatus[]> {
    this.requireBrowserShell();
    this.manifests.clear();
    this.state.settings = { ...this.state.settings, browser_integration: false };
    this.browserChanged();
    return this.browserStatuses();
  }

  async browserStatus(): Promise<BrowserStatus[]> {
    this.requireBrowserShell();
    return this.browserStatuses();
  }

  async quickAccessDone(copied: boolean): Promise<void> {
    this.quickAccessEnds.push(copied);
  }

  /** The quick search where the core offers it (password management on, the desktop), with what
   *  the shell reported of its shortcut. */
  private quickAccessView(): UiState["quick_access"] {
    if (!this.passwords || this.state.platform === "android")
      return { shortcut: null, error: null };
    return { shortcut: this.state.settings.quick_access_shortcut, error: this.shortcutError };
  }

  /** Test hook: the quick search asked for, as the shell does on its shortcut, the tray's
   *  「快捷搜索」 or `lockra-desktop --palette`: the palette opens while unlocked. Whether it was
   *  asked to open. */
  summonQuickAccess(): boolean {
    if (!this.passwords || this.state.platform === "android" || this.state.phase !== "unlocked")
      return false;
    this.notice({ type: "open_palette" });
    return true;
  }

  /** Test hook: the shell registered the chosen shortcut, or could not. */
  reportShortcut(registered: boolean): void {
    if (this.state.settings.quick_access_shortcut === null) return;
    this.shortcutError = registered ? null : "shortcut_unavailable";
    this.state.quick_access = this.quickAccessView();
    this.publish();
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
        this.confirmPresence(command.password, command.reason);
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
      case "view_order_entries": {
        this.requireUnlocked();
        // As the core: accounts that exist, once each; the code order becomes manual.
        const existing = new Set(this.state.entries.map((e) => e.id));
        this.order = [...new Set(command.ids)].filter((id) => existing.has(id));
        this.state.entry_order = [...this.order];
        this.state.settings = { ...this.state.settings, sort: "manual" };
        this.publish();
        return null;
      }
      case "view_order_groups": {
        this.requireUnlocked();
        // As the core: groups in use, cleaned, once each; "" (no group) moves like a group.
        const existing = new Set(this.state.entries.map((e) => e.group ?? ""));
        this.groupOrder = [...new Set(command.groups.map((g) => g.trim()))].filter(
          (g) => g === "" || existing.has(g),
        );
        this.state.group_order = [...this.groupOrder];
        this.publish();
        return null;
      }
      case "view_pin_group": {
        this.requireUnlocked();
        // As the core: a group in use, its name cleaned; another name pins nothing.
        const pinned = command.pinned;
        if (pinned?.type === "group") {
          const name = pinned.name.trim();
          const used = this.state.entries.some((e) => e.group === name);
          this.pinned = used ? { type: "group", name } : null;
        } else this.pinned = pinned;
        this.state.pinned_group = this.pinned;
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
        if (this.awaiting.kind === "kdbx") {
          // A KeePass database opens with the mock's password, with or without a key file.
          if (command.password !== MOCK_PASSWORD)
            throw new LockraError(
              this.awaiting.key_file === null ? "wrong_password" : "import_key_file_invalid",
            );
          const name = this.awaiting.name;
          this.awaiting = null;
          this.addLoginCandidates({ type: "file", name });
          this.publishImport();
          return null;
        }
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
      case "import_key_file_clear":
        this.requireUnlocked();
        this.requirePasswords();
        if (this.awaiting?.kind !== "kdbx") throw new LockraError("no_import");
        this.awaiting = { ...this.awaiting, key_file: null };
        this.publishImport();
        return null;
      case "import_commit":
        return this.commit(command.choices ?? []);
      case "import_cancel":
        this.clearImport();
        this.publish();
        return null;
      case "export_start":
        return this.exportStart(
          command.target,
          command.entry_ids,
          command.password,
          command.reason,
        );
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
        const shortcut = settings.quick_access_shortcut?.trim() || null;
        // The shell's report was about the shortcut chosen before.
        if (shortcut !== this.state.settings.quick_access_shortcut) this.shortcutError = null;
        this.state.settings = {
          ...settings,
          font_size_px: Math.min(18, Math.max(12, settings.font_size_px)),
          quick_access_shortcut: shortcut,
        };
        this.state.quick_access = this.quickAccessView();
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
          command.reason,
        );
      case "sync_invite":
        return this.syncInvite(command.password, command.reason);
      case "sync_invite_copy": {
        const { storage } = this.requireSpace();
        // As the core: a sealed text, or a relay space's own pairing link; never an invitation
        // that carries storage credentials.
        const text = command.text.trim();
        const ownLink =
          storage.kind === "relay" &&
          text.startsWith("lockra-invite:1:") &&
          invitedRelay(text) === storage.url;
        if (!isSealedInvite(text) && !ownLink) throw new LockraError("sync_invite_invalid");
        this.clipboard = text;
        return null;
      }
      case "sync_key_acknowledge":
        this.setSpace({ ...this.requireSpace(), key_saved: true });
        return null;
      case "sync_key_reveal":
        this.requireSpace();
        this.confirmPresence(command.password, command.reason);
        return { sync_key: MOCK_SYNC_KEY };
      case "sync_set_storage": {
        const space = this.requireSpace();
        this.confirmPresence(command.password, command.reason);
        checkStorage(command.storage, this.chosenFolder);
        this.setSpace({ ...space, storage: storageView(command.storage, this.chosenFolder) });
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
      case "items_list":
        this.requirePasswords();
        return this.items.map((item) => structuredClone(item.view));
      case "item_add":
        return this.itemAdd(command.draft);
      case "item_update":
        return this.itemUpdate(command.id, command.patch);
      case "item_delete": {
        this.requirePasswords();
        if (!command.ids.some((id) => this.items.some((i) => i.view.id === id)))
          throw new LockraError("item_not_found");
        this.items = this.items.filter((i) => !command.ids.includes(i.view.id));
        this.itemsChanged();
        return null;
      }
      case "item_reveal":
        return this.itemReveal(command.id, command.parts, command.password, command.reason);
      case "item_copy":
        return this.itemCopy(command.id, command.part, command.password, command.reason);
      case "generator_generate":
        return this.generatorGenerate(command.options, command.domain ?? null);
      case "generator_copy": {
        this.requirePasswords();
        if (this.generated === null) throw new LockraError("item_invalid");
        this.rememberGenerated();
        this.clipboard = this.generated.password;
        const clear = this.state.settings.clipboard_clear_seconds;
        this.changed();
        this.notice({
          type: "item_copied",
          item_id: null,
          part: "generated",
          clear_after_s: clear > 0 ? clear : null,
        });
        return null;
      }
      case "generator_history_reveal":
        this.requirePasswords();
        this.confirmPresence(command.password, command.reason);
        return structuredClone(this.generatedHistory);
      case "generator_history_clear":
        this.requirePasswords();
        this.generatedHistory = [];
        this.state.generator = { history_count: 0 };
        this.changed();
        return null;
      case "password_score":
        this.requirePasswords();
        return mockScore(command.password);
      case "site_rule_add": {
        this.requirePasswords();
        const domains = command.domains.map((d) => d.trim().toLowerCase()).filter((d) => d !== "");
        if (domains.length === 0 || (command.kind === "equivalent" && domains.length < 2))
          throw new LockraError("item_invalid");
        const same = this.siteRules.find(
          (r) =>
            r.kind === command.kind &&
            domains.length === 1 &&
            r.domains.length === 1 &&
            r.domains[0] === domains[0],
        );
        if (!same) this.siteRules.push({ id: nextId(), kind: command.kind, domains });
        this.itemsChanged();
        return null;
      }
      case "site_rule_remove": {
        this.requirePasswords();
        if (!this.siteRules.some((r) => r.id === command.id))
          throw new LockraError("item_not_found");
        this.siteRules = this.siteRules.filter((r) => r.id !== command.id);
        this.itemsChanged();
        return null;
      }
      case "view_collapse_items": {
        this.requirePasswords();
        const groups = new Set(this.items.map((i) => i.view.group).filter((g) => g !== null));
        this.itemFolds = [...new Set(command.groups.map((g) => g.trim()))]
          .filter((g) => g === "" || groups.has(g))
          // oxlint-disable-next-line unicorn/no-array-sort
          .sort();
        this.state.item_collapsed_groups = [...this.itemFolds];
        this.changed();
        return null;
      }
      case "browser_pair_approve": {
        this.requireBrowser();
        const request = this.browser.pairing.find((p) => p.id === command.id);
        if (request === undefined || request.expires_at_ms <= this.now())
          throw new LockraError("browser_pair_expired");
        this.confirmPresence(command.password, command.reason);
        this.browser.pairing = this.browser.pairing.filter((p) => p.id !== command.id);
        this.browser.paired = [
          ...this.browser.paired.filter(
            (p) => p.extension !== request.extension || p.browser !== request.browser,
          ),
          {
            id: nextId(),
            browser: request.browser,
            label: request.label,
            extension: request.extension,
            paired_at_ms: this.now(),
            last_seen_ms: null,
            connected: false,
            page_access: null,
            paused: false,
          },
        ];
        this.recordBrowser(request.browser, request.label, "paired");
        this.browserChanged();
        return null;
      }
      case "browser_pair_deny":
        this.requireBrowser();
        if (!this.browser.pairing.some((p) => p.id === command.id))
          throw new LockraError("browser_pair_expired");
        this.browser.pairing = this.browser.pairing.filter((p) => p.id !== command.id);
        this.browserChanged();
        return null;
      case "browser_fill_confirm": {
        this.requireBrowser();
        const waiting = this.browser.confirms.find((c) => c.id === command.id);
        if (waiting === undefined || waiting.expires_at_ms <= this.now())
          throw new LockraError("browser_pair_expired");
        if (waiting.needs_proof) this.confirmPresence(command.password, command.reason);
        this.browser.confirms = this.browser.confirms.filter((c) => c.id !== command.id);
        this.browserChanged();
        return null;
      }
      case "browser_fill_deny":
        this.requireBrowser();
        if (!this.browser.confirms.some((c) => c.id === command.id))
          throw new LockraError("browser_pair_expired");
        this.browser.confirms = this.browser.confirms.filter((c) => c.id !== command.id);
        this.browserChanged();
        return null;
      case "browser_unpair": {
        this.requireBrowser();
        const paired = this.browser.paired.find((p) => p.id === command.id);
        if (paired === undefined) throw new LockraError("browser_not_paired");
        this.browser.paired = this.browser.paired.filter((p) => p.id !== command.id);
        this.recordBrowser(paired.browser, paired.label, "unpaired");
        this.browserChanged();
        return null;
      }
      case "browser_resume":
        this.requireBrowser();
        this.browser.paired = this.browser.paired.map((p) => ({ ...p, paused: false }));
        this.browserChanged();
        return null;
      case "browser_activity_clear":
        this.requireBrowser();
        this.browser.activity = [];
        this.browserChanged();
        return null;
      case "browser_set_policy":
        this.requireBrowser();
        if (riskierPolicy(this.browser.policy, command.policy))
          this.confirmPresence(command.password, command.reason);
        this.browser.policy = structuredClone(command.policy);
        this.browserChanged();
        return null;
      case "sync_disable":
        this.requireSpace();
        this.setSpace(null);
        return null;
    }
  }

  private requireSpace(): SyncSpaceView {
    this.requireUnlocked();
    if (this.space === null) throw new LockraError("sync_off");
    return this.space;
  }

  private setSpace(space: SyncSpaceView | null): void {
    this.space = space;
    this.state.sync = { space };
    this.publish();
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
      storage: storageView(storage, this.chosenFolder),
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
      keyring_unsealed: this.joinedUnsealed,
      key_saved: true,
    });
  }

  private syncCreate(storage: StorageConfig, password: string, deviceName: string): null {
    this.requireUnlocked();
    if (this.space !== null) throw new LockraError("sync_already_on");
    checkStorage(storage, this.chosenFolder);
    this.checkPassword(password);
    this.newSpace(storage, deviceName, []);
    // Settings › Sync reminds of the key until it is saved.
    this.setSpace({ ...this.requireSpace(), key_saved: false });
    return null;
  }

  private syncJoin(
    source: JoinSource,
    password: string | undefined,
    deviceName: string,
    spacePassword?: string,
    reason?: string,
  ): null {
    const text = source.type === "invite" ? source.text.trim() : "";
    if (source.type === "invite" && text.startsWith("lockra-invite:2:")) {
      const typed = (source.code ?? "").toUpperCase().replace(/[\s-]/g, "");
      if (typed !== MOCK_INVITE_CODE.replace("-", ""))
        throw new LockraError("sync_invite_code_wrong");
    } else if (source.type === "invite" && !text.startsWith("lockra-invite:1:"))
      throw new LockraError("sync_invite_invalid");
    if (source.type === "invite" && held(text).startsWith(OLD_INVITE))
      throw new LockraError("sync_invite_outdated");
    if (source.type === "invite" && source.storage === undefined && keyOnly(text))
      throw new LockraError("sync_invite_needs_storage");
    if (
      source.type === "manual" &&
      !/^LKS1(-?[A-Z2-7]{4}){14}$/i.test(source.sync_key.replace(/\s/g, ""))
    )
      throw new LockraError("sync_key_invalid");
    if (this.state.phase === "locked") throw new LockraError("locked");
    if (this.state.phase === "unlocked" && this.space !== null)
      throw new LockraError("sync_already_on");
    // A new vault needs its own password; an unlocked one may prove presence by the fingerprint
    // that unlocks it, its keyring then sealed at the next unlock with the master password.
    if (this.state.phase === "no_vault") this.checkLength(password ?? "");
    else this.confirmPresence(password, reason);
    this.joinedUnsealed = this.state.phase === "unlocked" && password === undefined;
    const relay = source.type === "invite" ? invitedRelay(text) : undefined;
    const storage: StorageConfig =
      source.type === "manual"
        ? source.storage
        : (source.storage ??
          (relay !== undefined
            ? { kind: "relay", url: relay }
            : {
                kind: "webdav",
                url: "https://dav.example.com/dav/",
                prefix: "lockra",
                username: "me@example.com",
                password: MOCK_STORAGE_SECRET,
              }));
    checkStorage(storage, this.chosenFolder);
    // An invitation hands the space over. A recovery opens it with a master password of its
    // devices (the mock's): a vault of its own whose password is another asks for theirs first,
    // as the core does.
    if (source.type === "manual") {
      if (
        this.state.phase === "unlocked" &&
        spacePassword === undefined &&
        password !== MOCK_PASSWORD
      )
        throw new LockraError("sync_space_password_needed");
      if ((spacePassword ?? password) !== MOCK_PASSWORD)
        throw new LockraError("sync_wrong_credentials");
    }
    const others = [
      { tag: mockTag("Pixel 8"), name: "Pixel 8", written_at_ms: this.now(), this_device: false },
    ];
    if (this.state.phase === "no_vault") {
      // A new device: the vault comes from the space (its password was checked above).
      this.password = password ?? "";
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
    // A space in a folder of this computer invites with its sync key alone; one on a relay with
    // the relay's address and the sync key.
    const storage = this.requireSpace().storage;
    const includes = storage.kind !== "folder";
    const inside =
      storage.kind === "relay"
        ? `${RELAY_INVITE}${storage.url}\n`
        : includes
          ? "mock-invite"
          : KEY_ONLY;
    return {
      invite: `lockra-invite:1:${btoa(`${inside}:${this.now()}`)}`,
      svg: placeholderSvg("invite"),
      // On a relay nothing is sealed: the invitation is the pairing link, sent as it is.
      shared:
        storage.kind === "relay"
          ? null
          : { text: `lockra-invite:2:${btoa(`${inside}:${this.now()}`)}`, code: MOCK_INVITE_CODE },
      includes_storage: includes,
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
    // A phone from Google Play is updated by Play: nothing is even checked.
    if (this.state.update.method === "play") throw new LockraError("update_unavailable");
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
    // As the core: a keyring left unsealed by a fingerprint join is sealed now, written next run.
    if (this.space?.keyring_unsealed === true)
      this.space = { ...this.space, keyring_unsealed: false, keyring_pending: true };
    this.enterUnlocked(this.lockedEntries);
    return null;
  }

  private enterUnlocked(entries: EntryView[]): void {
    this.state.phase = "unlocked";
    this.state.items_rev += 1;
    this.state.items_count = this.items.length;
    this.state.site_rules = structuredClone(this.siteRules);
    this.state.generator = { history_count: this.generatedHistory.length };
    this.state.entries = entries;
    this.state.collapsed_groups = [...this.collapsed];
    this.state.entry_order = [...this.order];
    this.state.group_order = [...this.groupOrder];
    this.state.pinned_group = this.pinned;
    this.state.sync = { space: this.space };
    this.lockedEntries = [];
    this.state.lock = { ...this.state.lock, failed_attempts: 0, retry_at_ms: null };
    this.state.browser = this.browserView();
    this.state.item_collapsed_groups = this.passwords ? [...this.itemFolds] : [];
    this.refreshAutoLock();
    this.publish();
    this.pushFrame();
  }

  private lock(): void {
    if (this.state.phase !== "unlocked") return;
    this.generated = null;
    this.browser.pairing = [];
    this.browser.confirms = [];
    this.browser.activity = [];
    this.browser.paired = this.browser.paired.map((p) => ({ ...p, connected: false }));
    this.state.items_rev += 1;
    this.state.items_count = 0;
    this.state.site_rules = [];
    this.state.item_collapsed_groups = [];
    this.state.generator = { history_count: 0 };
    this.lockedEntries = this.state.entries;
    this.state.entries = [];
    this.state.collapsed_groups = [];
    this.state.entry_order = [];
    this.state.group_order = [];
    this.state.pinned_group = null;
    this.state.phase = "locked";
    this.state.sync = { space: null };
    this.exports.clear();
    this.clearImport();
    this.state.auto_lock_at_ms = null;
    this.state.browser = this.browserView();
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
    password: string | undefined,
    reason: string | undefined,
  ): ExportStarted {
    this.requireUnlocked();
    const chosen = entryIds
      .map((id) => this.state.entries.find((e) => e.id === id))
      .filter((e): e is EntryView => e !== undefined);
    if (chosen.length === 0) throw new LockraError("export_nothing");
    this.confirmPresence(password, reason);
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
            login: null,
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
        login: null,
      },
    });
  }

  /** What a password manager's export holds, as the core lists it: a login with its code right
   *  after it, a login the vault may hold already, and a card it does not take. */
  private addLoginCandidates(source: CandidateView["source"]): void {
    const candidate = (
      view: Omit<CandidateView, "id" | "source" | "origin" | "line">,
      item?: MockItem,
    ) =>
      this.candidates.push({
        secret: "",
        view: { id: this.candidates.length, source, origin: "uri", line: null, ...view },
        login: item,
      });
    const blank = { issuer: "", account: "", kind: null, algorithm: null, digits: null } as const;
    const github = importedLogin(
      "GitHub",
      "octocat",
      "https://github.com/login",
      "mock-imported-1",
    );
    candidate(
      {
        ...blank,
        status: { type: "new" },
        default_action: "add",
        login: {
          title: "GitHub",
          username: "octocat",
          host: "github.com",
          totp: this.candidates.length + 1,
        },
      },
      github,
    );
    this.addCandidate(
      source,
      "uri",
      "GitHub",
      "octocat",
      "GEZDGNBVGY3TQOJQ",
      { type: "totp", period: 30 },
      6,
    );
    const router = importedLogin("Router", "admin", "https://router.local", "mock-imported-2");
    const existing = this.items.find((i) => i.view.title === "Router");
    candidate(
      {
        ...blank,
        status: existing
          ? { type: "login_conflict", item_id: existing.view.id, reason: "password" }
          : { type: "new" },
        default_action: existing ? "replace" : "add",
        login: { title: "Router", username: "admin", host: "router.local", totp: null },
      },
      router,
    );
    candidate({
      ...blank,
      account: "Visa",
      status: { type: "unsupported", reason: "card_item" },
      default_action: "skip",
      login: null,
    });
  }

  private commit(choices: readonly Choice[]): ImportOutcome {
    this.requireUnlocked();
    if (this.state.import === null) throw new LockraError("no_import");
    const outcome: ImportOutcome = {
      added: 0,
      replaced: 0,
      skipped: 0,
      logins: { added: 0, replaced: 0, skipped: 0 },
    };
    // The codes first, each mapped to the account it ends as; then the logins, linked through it.
    const accounts = new Map<number, string>();
    const logins: MockCandidate[] = [];
    for (const candidate of this.candidates) {
      const view = candidate.view;
      const action = choices.find((c) => c.id === view.id)?.action ?? view.default_action;
      if (
        view.login !== null ||
        (view.status.type === "unsupported" && view.status.reason.endsWith("_item"))
      ) {
        logins.push(candidate);
        continue;
      }
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
        const id = this.addEntry(
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
        accounts.set(view.id, id);
        outcome.added += 1;
      }
    }
    for (const candidate of logins) {
      const view = candidate.view;
      const action = choices.find((c) => c.id === view.id)?.action ?? view.default_action;
      const item = candidate.login;
      const link =
        view.login?.totp === null || view.login === null
          ? undefined
          : accounts.get(view.login.totp);
      if (item === undefined || action === "skip" || view.status.type === "login_exists") {
        outcome.logins.skipped += 1;
      } else if (action === "replace" && view.status.type === "login_conflict") {
        const itemId = view.status.item_id;
        const target = this.items.find((i) => i.view.id === itemId);
        if (target) {
          target.password = item.password;
          if (link !== undefined) target.view.totp_entry = link;
          outcome.logins.replaced += 1;
        }
      } else {
        this.items.push({ ...item, view: { ...item.view, totp_entry: link ?? null } });
        outcome.logins.added += 1;
      }
    }
    if (logins.length > 0) this.itemsChanged();
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

  private requirePasswords(): void {
    if (!this.passwords) throw new LockraError("feature_unavailable");
    this.requireUnlocked();
  }

  /** The browser extension's commands: password management on, the desktop, unlocked. */
  private requireBrowser(): void {
    if (!this.passwords) throw new LockraError("feature_unavailable");
    if (this.state.platform === "android") throw new LockraError("browser_unavailable");
    this.requireUnlocked();
  }

  /** Settings › Browser as the core shows it: the switch always, the rest once unlocked. */
  private browserView(): BrowserView {
    const available = this.passwords && this.state.platform !== "android";
    const enabled = available && this.state.settings.browser_integration;
    if (!available || this.state.phase !== "unlocked")
      return {
        available,
        enabled,
        paired: [],
        pairing: [],
        confirms: [],
        policy: defaultBrowserPolicy(),
        activity: [],
      };
    const now = this.now();
    return {
      available,
      enabled,
      paired: structuredClone(this.browser.paired),
      pairing: this.browser.pairing.filter((p) => p.expires_at_ms > now).map((p) => ({ ...p })),
      confirms: this.browser.confirms
        .filter((c) => c.expires_at_ms > now)
        .map((c) => structuredClone(c)),
      policy: structuredClone(this.browser.policy),
      activity: structuredClone(this.browser.activity),
    };
  }

  private browserChanged(): void {
    this.changed();
  }

  private recordBrowser(browser: string, label: string, event: ActivityView["event"]): void {
    this.browser.activity = [
      {
        at_ms: this.now(),
        browser,
        label,
        site: "",
        event,
        refusal: null,
        item_id: null,
        title: null,
      },
      ...this.browser.activity,
    ].slice(0, 100);
  }

  /** Test hook: an extension finished the pairing handshake, as the channel would tell the core. */
  simulatePairRequest(
    request: Omit<PairRequestView, "id" | "expires_at_ms"> & { expires_at_ms?: number },
  ): string {
    const id = nextId();
    this.browser.pairing.push({
      ...request,
      id,
      expires_at_ms: request.expires_at_ms ?? this.now() + PAIR_TTL_MS,
    });
    this.changed();
    this.notice({
      type: "browser_pair_requested",
      id,
      browser: request.browser,
      code: request.code,
    });
    return id;
  }

  /** Test hook: an extension asks to fill or copy a login that waits for the user. */
  simulateConfirm(
    view: Omit<ConfirmView, "id" | "expires_at_ms"> & { expires_at_ms?: number },
  ): string {
    const id = nextId();
    this.browser.confirms.push({
      ...view,
      id,
      expires_at_ms: view.expires_at_ms ?? this.now() + CONFIRM_TTL_MS,
    });
    this.changed();
    this.notice({
      type: "browser_confirm_requested",
      id,
      browser: view.browser,
      title: view.title,
    });
    return id;
  }

  /** The logins or the site rules changed: the list is to be asked for again. */
  private itemsChanged(): void {
    this.state.items_rev += 1;
    this.state.items_count = this.items.length;
    this.state.site_rules = structuredClone(this.siteRules);
    this.changed();
  }

  private item(id: string): MockItem {
    this.requirePasswords();
    const item = this.items.find((i) => i.view.id === id);
    if (!item) throw new LockraError("item_not_found");
    return item;
  }

  /** The value a secret change leaves, `undefined` for one that leaves it as it was. */
  private secretValue(change: SecretChange | undefined): string | undefined {
    if (change === undefined || change.type === "keep") return undefined;
    if (change.type === "clear") return "";
    if (change.type === "set") return change.value;
    if (this.generated === null) throw new LockraError("item_invalid");
    this.rememberGenerated();
    return this.generated.password;
  }

  private rememberGenerated(): void {
    if (this.generated === null) return;
    const { password, site } = this.generated;
    this.generatedHistory = [
      { password, at_ms: this.now(), site },
      ...this.generatedHistory.filter((g) => g.password !== password),
    ].slice(0, 100);
    this.state.generator = { history_count: this.generatedHistory.length };
  }

  private applyFields(item: MockItem, changes: readonly FieldChange[]): void {
    for (const change of changes) {
      const existing =
        change.id === undefined ? undefined : item.view.fields.find((f) => f.id === change.id);
      if (change.id !== undefined && !existing) throw new LockraError("item_not_found");
      if (change.remove === true) {
        item.view.fields = item.view.fields.filter((f) => f.id !== change.id);
        if (change.id !== undefined) delete item.hidden?.[change.id];
        continue;
      }
      const field = existing ?? {
        id: nextId(),
        label: "",
        hidden: false,
        value: "",
        has_value: false,
        page_field: null,
      };
      if (change.label !== undefined) field.label = change.label.trim();
      if (change.hidden !== undefined) field.hidden = change.hidden;
      if (change.page_field !== undefined)
        field.page_field = change.page_field.trim() === "" ? null : change.page_field.trim();
      const value =
        this.secretValue(change.value) ??
        (field.hidden ? (item.hidden?.[field.id] ?? "") : (field.value ?? ""));
      field.has_value = value !== "";
      if (field.hidden) {
        item.hidden = { ...item.hidden, [field.id]: value };
        field.value = null;
      } else {
        field.value = value;
      }
      if (!existing) item.view.fields.push(field);
    }
    if (item.view.fields.length > 61) throw new LockraError("item_invalid");
  }

  private itemAdd(draft: ItemDraft): { id: string } {
    this.requirePasswords();
    if (draft.totp_entry != null && !this.state.entries.some((e) => e.id === draft.totp_entry))
      throw new LockraError("entry_not_found");
    const now = this.now();
    const title = (draft.title ?? "").trim();
    if (Array.from(title).length > 200) throw new LockraError("item_invalid");
    const password = this.secretValue(draft.password) ?? "";
    const item: MockItem = {
      view: mockItemView(title, {
        username: (draft.username ?? "").trim(),
        email: (draft.email ?? "").trim(),
        has_password: password !== "",
        urls: (draft.urls ?? [])
          .filter((u) => u.url.trim() !== "")
          .map((u) => ({ url: u.url.trim(), matching: u.matching ?? "domain" })),
        has_notes: (draft.notes ?? "") !== "",
        tags: [...new Set((draft.tags ?? []).map((t) => t.trim()).filter((t) => t !== ""))],
        group: draft.group?.trim() || null,
        favorite: draft.favorite ?? false,
        color: draft.color ?? "auto",
        mark: draft.mark?.trim() || null,
        protection: draft.protection ?? "fill",
        autofill: {
          on_page_load: "default",
          auto_submit: "default",
          fill_otp: "default",
          ...draft.autofill,
        },
        totp_entry: draft.totp_entry ?? null,
        created_at_ms: now,
        updated_at_ms: now,
      }),
      password,
      notes: draft.notes ?? "",
      history: [],
    };
    this.applyFields(
      item,
      (draft.fields ?? []).map((f) => ({ ...f, id: undefined })),
    );
    this.items.push(item);
    this.itemsChanged();
    return { id: item.view.id };
  }

  private itemUpdate(id: string, patch: ItemPatch): null {
    const item = this.item(id);
    if (item.view.read_only) throw new LockraError("item_read_only");
    if (patch.totp_entry != null && !this.state.entries.some((e) => e.id === patch.totp_entry))
      throw new LockraError("entry_not_found");
    const view = item.view;
    if (patch.title !== undefined) {
      if (Array.from(patch.title.trim()).length > 200) throw new LockraError("item_invalid");
      view.title = patch.title.trim();
    }
    if (patch.username !== undefined) view.username = patch.username.trim();
    if (patch.email !== undefined) view.email = patch.email.trim();
    const password = this.secretValue(patch.password);
    if (password !== undefined && password !== item.password) {
      if ((item.password ?? "") !== "") {
        item.history = [
          {
            id: String(this.now()),
            field: "password",
            value: item.password ?? "",
            set_at_ms: view.updated_at_ms,
            conflict: false,
          },
          ...(item.history ?? []),
        ].slice(0, 20);
      }
      item.password = password;
      view.has_password = password !== "";
    }
    const notes = patch.notes === undefined ? undefined : this.secretValue(patch.notes);
    if (notes !== undefined) {
      item.notes = notes;
      view.has_notes = notes !== "";
    }
    if (patch.urls !== undefined)
      view.urls = patch.urls
        .filter((u) => u.url.trim() !== "")
        .map((u) => ({ url: u.url.trim(), matching: u.matching ?? "domain" }));
    if (patch.tags !== undefined)
      view.tags = [...new Set(patch.tags.map((t) => t.trim()).filter((t) => t !== ""))];
    if (patch.group !== undefined) view.group = patch.group.trim() || null;
    if (patch.favorite !== undefined) view.favorite = patch.favorite;
    if (patch.color !== undefined) view.color = patch.color;
    if (patch.mark !== undefined) view.mark = patch.mark.trim() || null;
    if (patch.protection !== undefined) view.protection = patch.protection;
    if (patch.autofill !== undefined) view.autofill = { ...view.autofill, ...patch.autofill };
    if (patch.totp_entry !== undefined) view.totp_entry = patch.totp_entry;
    this.applyFields(item, patch.fields ?? []);
    for (const settle of patch.settle ?? []) {
      const past =
        settle.take === undefined ? undefined : item.history?.find((p) => p.id === settle.take);
      if (settle.take !== undefined && past === undefined) throw new LockraError("item_not_found");
      if (past !== undefined) item.password = past.value;
      view.conflicts = view.conflicts.filter((c) => c.field !== settle.field);
      item.history = (item.history ?? []).map((p) => ({ ...p, conflict: false }));
    }
    view.history_count = item.history?.length ?? 0;
    view.updated_at_ms = this.now();
    this.itemsChanged();
    return null;
  }

  private itemReveal(
    id: string,
    parts: RevealParts,
    password: string | undefined,
    reason: string | undefined,
  ): ItemRevealed {
    const item = this.item(id);
    if (item.view.protection === "confirm") this.confirmPresence(password, reason);
    return {
      item_id: id,
      password: parts.password === true ? (item.password ?? "") : null,
      notes: parts.notes === true ? (item.notes ?? "") : null,
      fields: (parts.fields ?? []).flatMap((field) => {
        const value = item.hidden?.[field];
        return value === undefined ? [] : [{ id: field, value }];
      }),
      history: parts.history === true ? structuredClone(item.history ?? []) : null,
    };
  }

  private itemCopy(
    id: string,
    part: CopyPart,
    password: string | undefined,
    reason: string | undefined,
  ): null {
    const item = this.item(id);
    const secret =
      part.type === "password" || (part.type === "field" && item.hidden?.[part.id] !== undefined);
    if (secret && item.view.protection === "confirm") this.confirmPresence(password, reason);
    let text: string | undefined;
    if (part.type === "username") text = item.view.username;
    else if (part.type === "email") text = item.view.email;
    else if (part.type === "password") text = item.password;
    else if (part.type === "field")
      text =
        item.hidden?.[part.id] ??
        item.view.fields.find((f) => f.id === part.id)?.value ??
        undefined;
    else {
      const entry = this.state.entries.find((e) => e.id === item.view.totp_entry);
      if (!entry) throw new LockraError("entry_not_found");
      text = this.frame().codes.find((c) => c.entry_id === entry.id)?.code;
    }
    if (text === undefined || text === "") throw new LockraError("item_not_found");
    this.clipboard = text;
    item.view.last_used_at_ms = this.now();
    const clear = this.state.settings.clipboard_clear_seconds;
    this.itemsChanged();
    this.notice({
      type: "item_copied",
      item_id: id,
      part: part.type,
      clear_after_s: clear > 0 ? clear : null,
    });
    return null;
  }

  private generatorGenerate(options: GeneratorOptions, domain: string | null): GeneratedView {
    this.requirePasswords();
    let password: string;
    let bits: number;
    if (options.kind === "passphrase") {
      const words = Math.min(20, Math.max(3, options.words));
      const list = [
        "plaza",
        "ribbon",
        "onward",
        "lucky",
        "cosmic",
        "fable",
        "harbor",
        "velvet",
        "maple",
        "quartz",
      ];
      const picked = Array.from(
        { length: words },
        (_, n) => list[(this.generatedHistory.length + n * 7) % list.length] ?? "word",
      );
      password =
        picked
          .map((w) => (options.capitalize ? w.charAt(0).toUpperCase() + w.slice(1) : w))
          .join(options.separator) + (options.number ? "7" : "");
      bits = Math.floor(words * 12.925);
    } else {
      const groups = [
        options.lower ? "abcdefghijkmnopqrstuvwxyz" : "",
        options.upper ? "ABCDEFGHJKLMNPQRSTUVWXYZ" : "",
        options.digits ? "23456789" : "",
        options.symbols ? "!#$%&*+-=?@^_" : "",
      ].filter((g) => g !== "");
      const alphabet = groups
        .join("")
        .split("")
        .filter((c) => !options.exclude.includes(c))
        .join("");
      if (alphabet === "") throw new LockraError("item_invalid");
      const length = Math.min(128, Math.max(4, options.length));
      const seed = this.now() + this.generatedHistory.length;
      password = Array.from(
        { length },
        (_, n) => alphabet[(seed + n * 31 + n * n) % alphabet.length],
      ).join("");
      bits = Math.floor(length * Math.log2(alphabet.length));
    }
    this.generated = { password, site: domain };
    return { password, bits, rules_met: domain !== null };
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
      awaiting: this.awaiting,
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
    this.awaiting = { name, kind: "backup", key_file: null };
    this.publishImport();
  }

  /** Test hook: put a KeePass database into the import, waiting for its password (the mock's,
   *  `MOCK_PASSWORD`) and, if the user wants, a key file. */
  awaitKdbx(name: string): void {
    this.awaiting = { name, kind: "kdbx", key_file: null };
    this.publishImport();
  }

  private refreshAutoLock(): void {
    const minutes = this.state.settings.auto_lock_minutes;
    this.state.auto_lock_at_ms =
      this.state.phase === "unlocked" && minutes > 0 ? this.now() + minutes * 60_000 : null;
  }

  private changed(): void {
    this.state.browser = this.browserView();
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
