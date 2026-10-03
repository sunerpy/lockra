import { ipcFixtures } from "./fixtures";
import { LockraError, isLockraError } from "./backend";
import type { CodesFrame, UiEvent } from "./schema";
import {
  type ChannelLike,
  TauriBackend,
  type TauriTransport,
  toLockraError,
} from "./tauri-backend";

interface Fake {
  transport: TauriTransport;
  calls: [string, Record<string, unknown> | undefined][];
  emit: (payload: unknown) => void;
  channels: ChannelLike[];
  unlistened: () => number;
}

function fake(answers: Record<string, unknown> = {}, failWith?: unknown): Fake {
  const calls: [string, Record<string, unknown> | undefined][] = [];
  let handler: ((event: { payload: unknown }) => void) | undefined;
  let unlistened = 0;
  const channels: ChannelLike[] = [];
  const transport: TauriTransport = {
    invoke: async (command, args) => {
      calls.push([command, args]);
      if (failWith !== undefined) throw failWith;
      return answers[command] ?? null;
    },
    listen: async (_event, h) => {
      handler = h;
      return () => {
        unlistened += 1;
      };
    },
    channel: () => {
      const channel: ChannelLike = { onmessage: () => undefined };
      channels.push(channel);
      return channel;
    },
  };
  return {
    transport,
    calls,
    emit: (payload) => handler?.({ payload }),
    channels,
    unlistened: () => unlistened,
  };
}

describe("TauriBackend", () => {
  it("reads the state through the dispatcher and validates it", async () => {
    const f = fake({ lockra_dispatch: ipcFixtures.state.unlocked });
    const state = await new TauriBackend(f.transport).getState();
    expect(state.entries).toHaveLength(4);
    expect(f.calls).toEqual([["lockra_dispatch", { command: { command: "app_state" } }]]);
    await expect(
      new TauriBackend(fake({ lockra_dispatch: { phase: "nope" } }).transport).getState(),
    ).rejects.toBeInstanceOf(Error);
  });

  it("passes valid events on and drops invalid ones", async () => {
    const f = fake();
    const seen: UiEvent[] = [];
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const off = new TauriBackend(f.transport).on((event) => seen.push(event));
    await Promise.resolve();
    f.emit(ipcFixtures.events[1]);
    f.emit({ type: "notice", notice: { type: "nonsense" } });
    expect(seen).toHaveLength(1);
    expect(error).toHaveBeenCalledOnce();
    off();
    expect(f.unlistened()).toBe(1);
  });

  it("an unsubscribe before the listener is ready still unsubscribes", async () => {
    const f = fake();
    const off = new TauriBackend(f.transport).on(() => undefined);
    off();
    await Promise.resolve();
    await Promise.resolve();
    expect(f.unlistened()).toBe(1);
  });

  it("streams code frames through a channel and stops on unsubscribe", async () => {
    const f = fake();
    const frames: CodesFrame[] = [];
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const off = await new TauriBackend(f.transport).subscribeCodes((frame) => frames.push(frame));
    expect(f.calls[0]?.[0]).toBe("codes_subscribe");
    expect(f.calls[0]?.[1]?.onFrame).toBe(f.channels[0]);
    f.channels[0]?.onmessage(ipcFixtures.responses.codes_frame);
    f.channels[0]?.onmessage({ at_ms: -1 });
    expect(frames).toHaveLength(1);
    expect(error).toHaveBeenCalledOnce();
    off();
    f.channels[0]?.onmessage(ipcFixtures.responses.codes_frame);
    expect(frames).toHaveLength(1);
    await Promise.resolve();
    expect(f.calls.at(-1)?.[0]).toBe("codes_unsubscribe");
  });

  it("regression: a stale unsubscribe leaves a newer subscription running (StrictMode runs effects twice)", async () => {
    // The core keeps one code stream and a new subscription replaces the old one, so the late
    // `codes_unsubscribe` of the first subscription stopped the second: no codes in `tauri dev`.
    const f = fake();
    const backend = new TauriBackend(f.transport);
    const first = await backend.subscribeCodes(() => undefined);
    const second = await backend.subscribeCodes(() => undefined);
    first();
    await Promise.resolve();
    expect(f.calls.map(([command]) => command)).toEqual(["codes_subscribe", "codes_subscribe"]);
    second();
    await Promise.resolve();
    expect(f.calls.at(-1)?.[0]).toBe("codes_unsubscribe");
  });

  it("calls the shell commands with their arguments", async () => {
    const f = fake({
      import_pick_files: true,
      import_scan: false,
      backup_save: "b.lockrabackup",
      backup_pick_dir: null,
      restore_pick: false,
      export_otpauth_file: "x.txt",
      sync_scan_join: true,
    });
    const backend = new TauriBackend(f.transport);
    expect(await backend.pickImportFiles()).toBe(true);
    expect(await backend.pickImportFiles("images")).toBe(true);
    expect(await backend.scanImport({ prompt: "Point at a code", cancel: "Cancel" })).toBe(false);
    expect(await backend.saveBackup("separate password")).toBe("b.lockrabackup");
    expect(await backend.saveBackup()).toBe("b.lockrabackup");
    expect(await backend.pickBackupDir()).toBeNull();
    expect(await backend.pickRestoreFile()).toBe(false);
    expect(await backend.exportOtpauthFile(["a"], "pw")).toBe("x.txt");
    const join = { password: "pw", deviceName: "Phone" };
    expect(await backend.scanJoin({ prompt: "Point at it", cancel: "Cancel" }, join)).toBe(true);
    expect(f.calls.map(([c, a]) => [c, a])).toEqual([
      ["import_pick_files", { kind: "any" }],
      ["import_pick_files", { kind: "images" }],
      ["import_scan", { prompt: "Point at a code", cancel: "Cancel" }],
      ["backup_save", { separatePassword: "separate password" }],
      ["backup_save", { separatePassword: null }],
      ["backup_pick_dir", undefined],
      ["restore_pick", undefined],
      ["export_otpauth_file", { entryIds: ["a"], password: "pw" }],
      [
        "sync_scan_join",
        {
          prompt: "Point at it",
          cancel: "Cancel",
          password: "pw",
          deviceName: "Phone",
          spacePassword: null,
        },
      ],
    ]);
  });

  it("turns the core's errors into LockraError", async () => {
    const limited = new TauriBackend(fake({}, { code: "rate_limited", retry_at_ms: 99 }).transport);
    const error = await limited
      .dispatch({ command: "vault_unlock", password: "x" })
      .catch((e: unknown) => e);
    expect(isLockraError(error) && error.code === "rate_limited" && error.retryAtMs === 99).toBe(
      true,
    );
    const spy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    expect(toLockraError("boom").code).toBe("internal");
    expect(spy).toHaveBeenCalledOnce();
    const known = new LockraError("locked");
    expect(toLockraError(known)).toBe(known);
    expect(isLockraError(new Error("x"))).toBe(false);
  });
});
