// What the webview talks to: `TauriBackend` (the real core over Tauri IPC) or `MockBackend` (an
// in-memory stand-in for the browser preview and the page tests).
import type {
  CodesFrame,
  CommandName,
  CommandOf,
  ErrorCode,
  ResultOf,
  StorageConfig,
  UiEvent,
  UiState,
} from "./schema";

export type EventListener = (event: UiEvent) => void;
export type ImportPickKind = "images" | "text" | "backup" | "any";
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
  /** Native file picker → a backup opened for restoring. `false` when cancelled. */
  pickRestoreFile(): Promise<boolean>;
  /** Native save dialog → a plain otpauth list. The file name, or `null` when cancelled. */
  exportOtpauthFile(entryIds: readonly string[], password: string): Promise<string | null>;
  /** The phone's browser → the page of the release an update check found (else the newest
   *  release's), the address named by the shell. `null` once it opened, else the address, to
   *  show (no browser opened it). */
  openRelease(): Promise<string | null>;
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
  /** This device's master password (with no vault yet, the new vault's). */
  password: string;
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
