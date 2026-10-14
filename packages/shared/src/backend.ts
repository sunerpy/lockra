// What the webview talks to: `TauriBackend` (the real core over Tauri IPC) or `MockBackend` (an
// in-memory stand-in for the browser preview and the page tests).
import type {
  BrowserId,
  BrowserStatus,
  CodesFrame,
  CommandName,
  CommandOf,
  ErrorCode,
  ItemsExportFormat,
  ResultOf,
  StorageConfig,
  UiEvent,
  UiState,
} from "./schema";

export type EventListener = (event: UiEvent) => void;
/** What the import's file picker offers: images, a text list, a Lockra backup, the exports of
 *  browsers, password managers and KeePass (`logins`), or every file. */
export type ImportPickKind = "images" | "text" | "backup" | "logins" | "any";
export type FrameListener = (frame: CodesFrame) => void;
export type Unsubscribe = () => void;

/** What `saveSyncKey` needs: the proof of presence and the file's words. */
export interface SaveSyncKey {
  password?: string;
  reason?: string;
  fileName: string;
  template: string;
}

export interface Backend {
  /** The state to render (`app_state`). */
  getState(): Promise<UiState>;
  /** Run one core command; resolves with its answer (`null` for most), rejects with `LockraError`. */
  dispatch<C extends CommandName>(command: CommandOf<C>): Promise<ResultOf<C>>;
  /** Every state change and notice. */
  on(listener: EventListener): Unsubscribe;
  /** Stream the codes; the first frame arrives at once. */
  subscribeCodes(onFrame: FrameListener): Promise<Unsubscribe>;
  /** Native file picker → import, filtered for one source (`any` shows every file: Microsoft's
   *  database has no extension). `false` when the user cancelled. */
  pickImportFiles(kind?: ImportPickKind): Promise<boolean>;
  /** Native file picker → the key file of the KeePass database waiting in the import: the core
   *  reads it and keeps the key it brings, never the file or its path (only its name shows in the
   *  preview). `false` when the user cancelled. */
  pickKeyFile(): Promise<boolean>;
  /** The phone's camera → import: the QR code it reads goes to the preview in Rust. `false` when
   *  the scan was left without one. `texts` are the camera page's words, in the app's language. */
  scanImport(texts: ScanTexts): Promise<boolean>;
  /** The phone's camera → joining a sync space: the invitation it reads (the storage's credentials
   *  and the sync key) goes to the core in Rust, never here, and joins as `sync_join` does with
   *  `join`. `false` when the scan was left without one. */
  scanJoin(texts: ScanTexts, join: ScanJoin): Promise<boolean>;
  /** Native save dialog → backup. The file name, or `null` when cancelled. */
  saveBackup(separatePassword?: string): Promise<string | null>;
  /** Native save dialog (the phone's file picker) → the sync key in a file: the core checks the
   *  user is there (`password`, else the biometric check with `reason`) and writes `template`
   *  with the key in its one `{{sync_key}}`; the key never comes here. `false` when cancelled. */
  saveSyncKey(args: SaveSyncKey): Promise<boolean>;
  /** Native folder picker → the automatic backup folder. The folder, or `null` when cancelled. */
  pickBackupDir(): Promise<string | null>;
  /** Native folder picker (the desktop's) → the folder for a sync space, one a cloud drive keeps in
   *  sync: the core keeps it for the next storage `{ kind: "folder" }`. The folder, to show, or
   *  `null` when cancelled. */
  pickSyncFolder(): Promise<string | null>;
  /** The browser integration (the desktop's): the chosen browsers' native-messaging manifests
   *  written and Lockra listening for the extension; every browser's status after. */
  browserEnable(browsers: readonly BrowserId[]): Promise<BrowserStatus[]>;
  /** The integration off: no longer listening, every manifest removed. */
  browserDisable(): Promise<BrowserStatus[]>;
  /** Every browser's status: its profiles here, Lockra's manifest in place. */
  browserStatus(): Promise<BrowserStatus[]>;
  /** Native file picker → a backup opened for restoring. `false` when cancelled. */
  pickRestoreFile(): Promise<boolean>;
  /** Native save dialog → a plain otpauth list. The file name, or `null` when cancelled. */
  /** Without a password, the biometric check that unlocks this vault proves presence. */
  exportOtpauthFile(
    entryIds: readonly string[],
    password?: string,
    reason?: string,
  ): Promise<string | null>;
  /** Native save dialog (the phone's file picker) → the logins `ids` as a KeePass database or
   *  Bitwarden's plain CSV, made in Rust once the user proved to be there; the file never comes
   *  here. The file name, or `null` when cancelled. */
  exportItemsFile(args: ExportItemsFile): Promise<string | null>;
  /** The phone's browser → the page of the release an update check found (else the newest
   *  release's), the address named by the shell. `null` once it opened, else the address, to
   *  show (no browser opened it). */
  openRelease(): Promise<string | null>;
}

/** What `exportItemsFile` exports, and how the user proves to be there. */
export interface ExportItemsFile {
  /** The logins, by id. */
  ids: readonly string[];
  format: ItemsExportFormat;
  /** The master password; without it, the biometric check that unlocks this vault, asked with
   *  `reason`. */
  password?: string;
  reason?: string;
  /** The user confirmed that anyone who reads a CSV file reads every password in it: required
   *  for `csv`. */
  acknowledged: boolean;
  /** The new database's own password, for `kdbx`. */
  kdbxPassword?: string;
}

/** The words on the phone's camera page. */
export interface ScanTexts {
  /** What to do, over the picture. */
  prompt: string;
  /** The button that leaves. */
  cancel: string;
}

/** What joining from a scanned invitation takes besides it, as `sync_join`. */
export interface ScanJoin {
  /** This device's master password (with no vault yet, the new vault's); an unlocked vault may
   *  leave it out, the fingerprint that unlocks it proving presence (`reason` for its prompt). */
  password?: string;
  reason?: string;
  /** This device's name in the space. */
  deviceName: string;
  /** The master password of the space's devices, when it is not `password`. */
  spacePassword?: string;
  /** This device's way to the space, for an invitation with the sync key alone. */
  storage?: StorageConfig;
}

/** A failed command: the core's error code, and when a rate-limited unlock may be retried. */
export class LockraError extends Error {
  readonly code: ErrorCode;
  readonly retryAtMs: number | undefined;

  constructor(code: ErrorCode, retryAtMs?: number) {
    super(code);
    this.name = "LockraError";
    this.code = code;
    this.retryAtMs = retryAtMs;
  }
}

export function isLockraError(value: unknown): value is LockraError {
  return value instanceof LockraError;
}
