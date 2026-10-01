// The real backend: Tauri IPC to the desktop shell. Every answer and event is validated with the
// zod schemas before the UI sees it; a payload that does not match is a contract bug and is
// reported, never rendered half-parsed.
import { Channel, invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { z } from "zod";

import {
  type Backend,
  type EventListener,
  type FrameListener,
  type ImportPickKind,
  LockraError,
  type Unsubscribe,
} from "./backend";
import {
  type CommandName,
  type CommandOf,
  type ResultOf,
  type UiCommand,
  type UiState,
  RESULT_SCHEMAS,
  UI_EVENT_NAME,
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

export class TauriBackend implements Backend {
  private readonly transport: TauriTransport;
  /** Counts code subscriptions; only the latest may stop the core's stream. */
  private codesGeneration = 0;

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
    const generation = ++this.codesGeneration;
    const channel = this.transport.channel();
    let open = true;
    // Tauri's `Channel` is not an EventTarget: `onmessage` is its only delivery hook.
    // oxlint-disable-next-line unicorn/prefer-add-event-listener
    channel.onmessage = (raw) => {
      if (!open) return;
      const parsed = codesFrameSchema.safeParse(raw);
      if (parsed.success) onFrame(parsed.data);
      else console.error("lockra: a code frame does not match the contract", parsed.error.issues);
    };
    await this.call("codes_subscribe", { onFrame: channel });
    return () => {
      open = false;
      // The core keeps one stream and a newer subscription has already replaced this one: stopping
      // now would stop that one (StrictMode subscribes twice; a page switch can too).
      if (generation === this.codesGeneration)
        void this.call("codes_unsubscribe").catch(() => undefined);
    };
  }

  async pickImportFiles(kind: ImportPickKind = "any"): Promise<boolean> {
    return z.boolean().parse(await this.call("import_pick_files", { kind }));
  }

  async saveBackup(separatePassword?: string): Promise<string | null> {
    return fileNameSchema.parse(
      await this.call("backup_save", { separatePassword: separatePassword ?? null }),
    );
  }

  async pickBackupDir(): Promise<string | null> {
    return fileNameSchema.parse(await this.call("backup_pick_dir"));
  }

  async pickRestoreFile(): Promise<boolean> {
    return z.boolean().parse(await this.call("restore_pick"));
  }

  async exportOtpauthFile(entryIds: readonly string[], password: string): Promise<string | null> {
    return fileNameSchema.parse(
      await this.call("export_otpauth_file", { entryIds: [...entryIds], password }),
    );
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
