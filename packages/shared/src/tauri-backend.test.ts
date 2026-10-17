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
    const frames: CodesFrame[] = [];
    const first = await backend.subscribeCodes(() => undefined);
    const second = await backend.subscribeCodes((frame) => frames.push(frame));
    first();
    await Promise.resolve();
    expect(f.calls.map(([command]) => command)).not.toContain("codes_unsubscribe");
    f.channels.at(-1)?.onmessage(ipcFixtures.responses.codes_frame);
    expect(frames).toHaveLength(1);
    second();
    await Promise.resolve();
    expect(f.calls.at(-1)?.[0]).toBe("codes_unsubscribe");
  });

  it("regression: one subscription ending leaves the others running (the detail pane under the edit dialog)", async () => {
    // 2026-10-17, the tutorial recording: after editing a login, the Passwords page's detail pane
    // showed "— — —" for good. The core keeps one stream per window: the dialog's subscription
    // replaced the pane's, and ending it stopped the stream for both.
    const f = fake();
    const backend = new TauriBackend(f.transport);
    const pane: CodesFrame[] = [];
    const dialog: CodesFrame[] = [];
    const offPane = await backend.subscribeCodes((frame) => pane.push(frame));
    f.channels[0]?.onmessage(ipcFixtures.responses.codes_frame);
    // The dialog shares the stream, and has the last frame at once.
    const offDialog = await backend.subscribeCodes((frame) => dialog.push(frame));
    expect(f.calls.filter(([command]) => command === "codes_subscribe")).toHaveLength(1);
    expect([pane.length, dialog.length]).toEqual([1, 1]);
    f.channels[0]?.onmessage(ipcFixtures.responses.codes_frame);
    expect([pane.length, dialog.length]).toEqual([2, 2]);
    offDialog();
    await Promise.resolve();
    expect(f.calls.map(([command]) => command)).not.toContain("codes_unsubscribe");
    f.channels[0]?.onmessage(ipcFixtures.responses.codes_frame);
    expect([pane.length, dialog.length]).toEqual([3, 2]);
    // The last one out stops the stream; the next subscription starts a new one.
    offPane();
    offPane();
    await Promise.resolve();
    expect(f.calls.filter(([command]) => command === "codes_unsubscribe")).toHaveLength(1);
    await backend.subscribeCodes(() => undefined);
    expect(f.calls.filter(([command]) => command === "codes_subscribe")).toHaveLength(2);
    expect(f.calls.at(-1)?.[1]?.onFrame).toBe(f.channels[1]);
  });

  it("shares one subscription among callers that ask at the same time, and a failed one with none", async () => {
    const f = fake();
    const backend = new TauriBackend(f.transport);
    await Promise.all([
      backend.subscribeCodes(() => undefined),
      backend.subscribeCodes(() => undefined),
    ]);
    expect(f.calls.filter(([command]) => command === "codes_subscribe")).toHaveLength(1);
    const failing = new TauriBackend(fake({}, new Error("no shell")).transport);
    await expect(failing.subscribeCodes(() => undefined)).rejects.toBeInstanceOf(Error);
    await expect(failing.subscribeCodes(() => undefined)).rejects.toBeInstanceOf(Error);
  });

  it("exports logins through the shell, which answers with the file's name alone", async () => {
    const f = fake({ export_items_file: "lockra-logins.kdbx" });
    const backend = new TauriBackend(f.transport);
    const kdbx = {
      ids: ["a", "b"],
      format: "kdbx",
      password: "pw",
      acknowledged: false,
      kdbxPassword: "database password",
    } as const;
    expect(await backend.exportItemsFile(kdbx)).toBe("lockra-logins.kdbx");
    // Without a password: the fingerprint, with the prompt's words.
    const csv = { ids: ["a"], format: "csv", reason: "export", acknowledged: true } as const;
    expect(await backend.exportItemsFile(csv)).toBe("lockra-logins.kdbx");
    expect(f.calls.map(([c, a]) => [c, a])).toEqual([
      [
        "export_items_file",
        {
          ids: ["a", "b"],
          format: "kdbx",
          password: "pw",
          reason: null,
          acknowledged: false,
          kdbxPassword: "database password",
        },
      ],
      [
        "export_items_file",
        {
          ids: ["a"],
          format: "csv",
          password: null,
          reason: "export",
          acknowledged: true,
          kdbxPassword: null,
        },
      ],
    ]);
  });

  it("ends a quick search through the shell, saying whether something was copied", async () => {
    const f = fake();
    const backend = new TauriBackend(f.transport);
    await backend.quickAccessDone(true);
    await backend.quickAccessDone(false);
    expect(f.calls).toEqual([
      ["quick_access_done", { copied: true }],
      ["quick_access_done", { copied: false }],
    ]);
  });

  it("calls the shell commands with their arguments", async () => {
    const f = fake({
      import_pick_files: true,
      import_pick_key_file: true,
      import_scan: false,
      backup_save: "b.lockrabackup",
      backup_pick_dir: null,
      sync_pick_folder: "C:\\Users\\me\\OneDrive\\Lockra",
      restore_pick: false,
      export_otpauth_file: "x.txt",
      sync_scan_join: true,
      update_open_release: "https://github.com/sunerpy/lockra/releases/tag/v0.7.0",
    });
    const backend = new TauriBackend(f.transport);
    expect(await backend.pickImportFiles()).toBe(true);
    expect(await backend.pickImportFiles("images")).toBe(true);
    expect(await backend.pickImportFiles("logins")).toBe(true);
    expect(await backend.pickKeyFile()).toBe(true);
    expect(await backend.scanImport({ prompt: "Point at a code", cancel: "Cancel" })).toBe(false);
    expect(await backend.saveBackup("separate password")).toBe("b.lockrabackup");
    expect(await backend.saveBackup()).toBe("b.lockrabackup");
    expect(await backend.pickBackupDir()).toBeNull();
    expect(await backend.pickSyncFolder()).toBe("C:\\Users\\me\\OneDrive\\Lockra");
    expect(await backend.pickRestoreFile()).toBe(false);
    expect(await backend.exportOtpauthFile(["a"], "pw")).toBe("x.txt");
    // Without a password: the fingerprint, with the prompt's words.
    expect(await backend.exportOtpauthFile(["a"], undefined, "export")).toBe("x.txt");
    const join = { password: "pw", deviceName: "Phone" };
    expect(await backend.scanJoin({ prompt: "Point at it", cancel: "Cancel" }, join)).toBe(true);
    const dav = {
      kind: "webdav",
      url: "https://dav.jianguoyun.com/dav/",
      prefix: "我的坚果云/Lockra",
      username: "me@example.com",
      password: "app password",
    } as const;
    expect(
      await backend.scanJoin(
        { prompt: "Point at it", cancel: "Cancel" },
        { ...join, storage: dav },
      ),
    ).toBe(true);
    expect(
      await backend.scanJoin(
        { prompt: "Point at it", cancel: "Cancel" },
        { deviceName: "Phone", reason: "join" },
      ),
    ).toBe(true);
    expect(await backend.openRelease()).toBe(
      "https://github.com/sunerpy/lockra/releases/tag/v0.7.0",
    );
    expect(f.calls.map(([c, a]) => [c, a])).toEqual([
      ["import_pick_files", { kind: "any" }],
      ["import_pick_files", { kind: "images" }],
      ["import_pick_files", { kind: "logins" }],
      ["import_pick_key_file", undefined],
      ["import_scan", { prompt: "Point at a code", cancel: "Cancel" }],
      ["backup_save", { separatePassword: "separate password" }],
      ["backup_save", { separatePassword: null }],
      ["backup_pick_dir", undefined],
      ["sync_pick_folder", undefined],
      ["restore_pick", undefined],
      ["export_otpauth_file", { entryIds: ["a"], password: "pw", reason: null }],
      ["export_otpauth_file", { entryIds: ["a"], password: null, reason: "export" }],
      [
        "sync_scan_join",
        {
          prompt: "Point at it",
          cancel: "Cancel",
          password: "pw",
          deviceName: "Phone",
          spacePassword: null,
          storage: null,
          reason: null,
        },
      ],
      [
        "sync_scan_join",
        {
          prompt: "Point at it",
          cancel: "Cancel",
          password: "pw",
          deviceName: "Phone",
          spacePassword: null,
          storage: dav,
          reason: null,
        },
      ],
      [
        "sync_scan_join",
        {
          prompt: "Point at it",
          cancel: "Cancel",
          password: null,
          deviceName: "Phone",
          spacePassword: null,
          storage: null,
          reason: "join",
        },
      ],
      ["update_open_release", undefined],
    ]);
  });

  it("asks the shell for the extension's folder, puts its path on the clipboard and shows it", async () => {
    const folder = {
      path: "C:\\Users\\me\\AppData\\Local\\Lockra\\browser\\extension",
      version: "0.10.2",
      ready: true,
    };
    const f = fake({ browser_extension: folder, browser_extension_reveal: true });
    const backend = new TauriBackend(f.transport);
    expect(await backend.browserExtension()).toEqual(folder);
    await backend.browserExtensionCopy();
    expect(await backend.browserExtensionReveal()).toBe(true);
    expect(f.calls.map(([c, a]) => [c, a])).toEqual([
      ["browser_extension", undefined],
      ["browser_extension_copy", undefined],
      ["browser_extension_reveal", undefined],
    ]);
    // A build made before the extension carries none; an answer that is no folder is refused.
    expect(await new TauriBackend(fake().transport).browserExtension()).toBeNull();
    await expect(
      new TauriBackend(fake({ browser_extension: { path: 1 } }).transport).browserExtension(),
    ).rejects.toThrow(/expected string/);
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
