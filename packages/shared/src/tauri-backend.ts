// The real backend: Tauri IPC to the desktop shell. Every answer and event is validated with the
// zod schemas before the UI sees it; a payload that does not match is a contract bug and is
// reported, never rendered half-parsed.
import { Channel, invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { z } from "zod";

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
  type BrowserId,
  type BrowserStatus,
  browserStatusSchema,
  extensionFolderSchema,
  type ExtensionFolder,
  type CommandName,
  type CommandOf,
  type ResultOf,
  type UiCommand,
  type UiState,
  RESULT_SCHEMAS,
  UI_EVENT_NAME,
  type CodesFrame,
  codesFrameSchema,
  coreErrorSchema,
  hasResult,
  uiEventSchema,
  uiStateSchema,
} from "./schema";

/** The part of `Channel` the backend needs; injectable because the real one registers itself with
 *  the Tauri runtime on construction and cannot exist in a plain browser or under vitest. */
export interface ChannelLike {
  onmessage: (raw: unknown) => void;
}

export interface TauriTransport {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  listen(event: string, handler: (event: { payload: unknown }) => void): Promise<() => void>;
  channel(): ChannelLike;
}

export const defaultTransport: TauriTransport = {
  invoke: (command, args) => tauriInvoke(command, args),
  listen: (event, handler) => tauriListen(event, handler),
  channel: () => new Channel(),
};

const fileNameSchema = z.string().nullable();

/** The core's code stream, which it keeps one of per window: every subscriber of the backend
 *  shares it, and it stops once the last one leaves. */
interface CodeStream {
  listeners: Set<FrameListener>;
  /** The last frame, for a subscriber that joins between two. */
  last: CodesFrame | null;
  /** `codes_subscribe`, done: the core streams. */
  started: Promise<void>;
}

export class TauriBackend implements Backend {
  private readonly transport: TauriTransport;
  private codes: CodeStream | null = null;

  constructor(transport: TauriTransport = defaultTransport) {
    this.transport = transport;
  }

  async getState(): Promise<UiState> {
    return uiStateSchema.parse(
      await this.call("lockra_dispatch", { command: { command: "app_state" } }),
    );
  }

  // The overload gives callers the exact answer type; the implementation returns what the schema
  // for the command parsed, which is that type, without a cast TypeScript could not check.
  dispatch<C extends CommandName>(command: CommandOf<C>): Promise<ResultOf<C>>;
  async dispatch(command: UiCommand): Promise<unknown> {
    const raw = await this.call("lockra_dispatch", { command });
    return hasResult(command.command) ? RESULT_SCHEMAS[command.command].parse(raw) : null;
  }

  on(listener: EventListener): Unsubscribe {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void this.transport
      .listen(UI_EVENT_NAME, (event) => {
        const parsed = uiEventSchema.safeParse(event.payload);
        if (parsed.success) listener(parsed.data);
        else console.error("lockra: an event does not match the contract", parsed.error.issues);
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((error: unknown) => console.error("lockra: cannot listen for events", error));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }

  async subscribeCodes(onFrame: FrameListener): Promise<Unsubscribe> {
    // A second `codes_subscribe` would replace the first subscriber's stream, and a
    // `codes_unsubscribe` would stop everyone's (the detail pane under the edit dialog, StrictMode's
    // second effect): the subscribers share one stream instead.
    const stream = this.codes ?? this.startCodes();
    await stream.started;
    stream.listeners.add(onFrame);
    if (stream.last) onFrame(stream.last);
    let open = true;
    return () => {
      if (!open) return;
      open = false;
      stream.listeners.delete(onFrame);
      if (stream.listeners.size > 0 || this.codes !== stream) return;
      this.codes = null;
      void this.call("codes_unsubscribe").catch(() => undefined);
    };
  }

  private startCodes(): CodeStream {
    const channel = this.transport.channel();
    const stream: CodeStream = { listeners: new Set(), last: null, started: Promise.resolve() };
    // Tauri's `Channel` is not an EventTarget: `onmessage` is its only delivery hook.
    // oxlint-disable-next-line unicorn/prefer-add-event-listener
    channel.onmessage = (raw) => {
      if (this.codes !== stream) return;
      const parsed = codesFrameSchema.safeParse(raw);
      if (!parsed.success) {
        console.error("lockra: a code frame does not match the contract", parsed.error.issues);
        return;
      }
      stream.last = parsed.data;
      for (const listener of stream.listeners) listener(parsed.data);
    };
    stream.started = this.call("codes_subscribe", { onFrame: channel }).then(() => undefined);
    // A failed start leaves no stream: the next subscriber asks again.
    stream.started.catch(() => {
      if (this.codes === stream) this.codes = null;
    });
    this.codes = stream;
    return stream;
  }

  async pickImportFiles(kind: ImportPickKind = "any"): Promise<boolean> {
    return z.boolean().parse(await this.call("import_pick_files", { kind }));
  }

  async pickKeyFile(): Promise<boolean> {
    return z.boolean().parse(await this.call("import_pick_key_file"));
  }

  async addEntryFromImage(): Promise<string | null> {
    return z
      .string()
      .min(1)
      .nullable()
      .parse(await this.call("entry_add_image"));
  }

  async scanImport(texts: ScanTexts): Promise<boolean> {
    return z
      .boolean()
      .parse(await this.call("import_scan", { prompt: texts.prompt, cancel: texts.cancel }));
  }

  async scanJoin(texts: ScanTexts, join: ScanJoin): Promise<boolean> {
    return z.boolean().parse(
      await this.call("sync_scan_join", {
        prompt: texts.prompt,
        cancel: texts.cancel,
        password: join.password ?? null,
        deviceName: join.deviceName,
        spacePassword: join.spacePassword ?? null,
        storage: join.storage ?? null,
        reason: join.reason ?? null,
      }),
    );
  }

  async saveSyncKey(args: SaveSyncKey): Promise<boolean> {
    return z.boolean().parse(
      await this.call("sync_key_save", {
        password: args.password ?? null,
        reason: args.reason ?? null,
        fileName: args.fileName,
        template: args.template,
      }),
    );
  }

  async saveBackup(separatePassword?: string): Promise<string | null> {
    return fileNameSchema.parse(
      await this.call("backup_save", { separatePassword: separatePassword ?? null }),
    );
  }

  async pickBackupDir(): Promise<string | null> {
    return fileNameSchema.parse(await this.call("backup_pick_dir"));
  }

  async pickSyncFolder(): Promise<string | null> {
    return fileNameSchema.parse(await this.call("sync_pick_folder"));
  }

  async browserEnable(browsers: readonly BrowserId[]): Promise<BrowserStatus[]> {
    return z.array(browserStatusSchema).parse(await this.call("browser_enable", { browsers }));
  }

  async browserDisable(): Promise<BrowserStatus[]> {
    return z.array(browserStatusSchema).parse(await this.call("browser_disable"));
  }

  async browserStatus(): Promise<BrowserStatus[]> {
    return z.array(browserStatusSchema).parse(await this.call("browser_status"));
  }

  async browserExtension(): Promise<ExtensionFolder | null> {
    return extensionFolderSchema.nullable().parse(await this.call("browser_extension"));
  }

  async browserExtensionCopy(): Promise<void> {
    await this.call("browser_extension_copy");
  }

  async browserExtensionReveal(): Promise<boolean> {
    return z.boolean().parse(await this.call("browser_extension_reveal"));
  }

  async browserExtensionDownload(): Promise<string | null> {
    return z
      .string()
      .nullable()
      .parse(await this.call("browser_extension_download"));
  }

  async quickAccessDone(copied: boolean): Promise<void> {
    await this.call("quick_access_done", { copied });
  }

  async pickRestoreFile(): Promise<boolean> {
    return z.boolean().parse(await this.call("restore_pick"));
  }

  async exportOtpauthFile(
    entryIds: readonly string[],
    password?: string,
    reason?: string,
  ): Promise<string | null> {
    return fileNameSchema.parse(
      await this.call("export_otpauth_file", {
        entryIds: [...entryIds],
        password: password ?? null,
        reason: reason ?? null,
      }),
    );
  }

  async exportItemsFile(args: ExportItemsFile): Promise<string | null> {
    return fileNameSchema.parse(
      await this.call("export_items_file", {
        ids: [...args.ids],
        format: args.format,
        password: args.password ?? null,
        reason: args.reason ?? null,
        acknowledged: args.acknowledged,
        kdbxPassword: args.kdbxPassword ?? null,
      }),
    );
  }

  async openRelease(): Promise<string | null> {
    return z
      .string()
      .nullable()
      .parse(await this.call("update_open_release"));
  }

  private async call(command: string, args?: Record<string, unknown>): Promise<unknown> {
    try {
      return await this.transport.invoke(command, args);
    } catch (error: unknown) {
      throw toLockraError(error);
    }
  }
}

/** The shell rejects with the core's `{ code, retry_at_ms? }`; anything else is an internal error. */
export function toLockraError(error: unknown): LockraError {
  if (error instanceof LockraError) return error;
  const parsed = coreErrorSchema.safeParse(error);
  if (parsed.success) return new LockraError(parsed.data.code, parsed.data.retry_at_ms);
  console.error("lockra: a command failed outside the contract", error);
  return new LockraError("internal");
}
