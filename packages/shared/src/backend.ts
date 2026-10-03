// What the webview talks to: `TauriBackend` (the real core over Tauri IPC) or `MockBackend` (an
// in-memory stand-in for the browser preview and the page tests).
import type {
  CodesFrame,
  CommandName,
  CommandOf,
  ErrorCode,
  ResultOf,
  UiEvent,
  UiState,
} from "./schema";

export type EventListener = (event: UiEvent) => void;
export type ImportPickKind = "images" | "text" | "backup" | "any";
export type FrameListener = (frame: CodesFrame) => void;
export type Unsubscribe = () => void;

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
  /** Native save dialog → backup. The file name, or `null` when cancelled. */
  saveBackup(separatePassword?: string): Promise<string | null>;
  /** Native folder picker → the automatic backup folder. The folder, or `null` when cancelled. */
  pickBackupDir(): Promise<string | null>;
  /** Native file picker → a backup opened for restoring. `false` when cancelled. */
  pickRestoreFile(): Promise<boolean>;
  /** Native save dialog → a plain otpauth list. The file name, or `null` when cancelled. */
  exportOtpauthFile(entryIds: readonly string[], password: string): Promise<string | null>;
}

/** The words on the phone's camera page. */
export interface ScanTexts {
  /** What to do, over the picture. */
  prompt: string;
  /** The button that leaves. */
  cancel: string;
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
