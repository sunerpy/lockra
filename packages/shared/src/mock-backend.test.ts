import { isLockraError } from "./backend";
import { MOCK_PASSWORD, MockBackend, fakeCode, mockEntry, sampleEntries } from "./mock-backend";
import type { CodesFrame, Notice, UiEvent } from "./schema";

async function errorCode(promise: Promise<unknown>): Promise<string | undefined> {
  const error = await promise.then(
    () => undefined,
    (e: unknown) => e,
  );
  return isLockraError(error) ? error.code : undefined;
}

function recorder(backend: MockBackend): { events: UiEvent[]; notices: () => Notice[] } {
  const events: UiEvent[] = [];
  backend.on((e) => events.push(e));
  return { events, notices: () => events.flatMap((e) => (e.type === "notice" ? [e.notice] : [])) };
}

describe("MockBackend", () => {
  it("creates, locks, rate-limits and unlocks", async () => {
    let now = Date.UTC(2026, 8, 30);
    const backend = new MockBackend({ now: () => now });
    expect((await backend.getState()).phase).toBe("no_vault");
    expect(await errorCode(backend.dispatch({ command: "vault_create", password: "short" }))).toBe(
      "password_too_short",
    );
    await backend.dispatch({ command: "vault_create", password: "a long password" });
    expect(
      await errorCode(backend.dispatch({ command: "vault_create", password: "a long password" })),
    ).toBe("vault_exists");
    await backend.dispatch({ command: "vault_lock" });
    for (let i = 0; i < 3; i += 1)
      expect(await errorCode(backend.dispatch({ command: "vault_unlock", password: "x" }))).toBe(
        "wrong_password",
      );
    expect(
      await errorCode(backend.dispatch({ command: "vault_unlock", password: "a long password" })),
    ).toBe("rate_limited");
    now += 1000;
    await backend.dispatch({ command: "vault_unlock", password: "a long password" });
    expect((await backend.getState()).phase).toBe("unlocked");
    await backend.dispatch({ command: "vault_unlock", password: "ignored" });
  });

  it("manages entries and streams stand-in codes", async () => {
    vi.useFakeTimers({ now: Date.UTC(2026, 8, 30, 0, 0, 5) });
    try {
      const backend = new MockBackend({ entries: [mockEntry("GitHub", "octocat")] });
      const frames: CodesFrame[] = [];
      const off = await backend.subscribeCodes((f) => frames.push(f));
      expect(frames[0]?.codes[0]?.code).toMatch(/^\d{6}$/);
      vi.advanceTimersByTime(25_001);
      expect(frames.length).toBeGreaterThan(1);
      expect(frames.at(-1)?.codes[0]?.code).toBe(frames[0]?.codes[0]?.next_code);
      const added = await backend.dispatch({
        command: "entry_add_uri",
        uri: "otpauth://totp/Mail:me?secret=GEZDGNBV&issuer=Mail",
      });
      expect(
        await errorCode(
          backend.dispatch({
            command: "entry_add_uri",
            uri: "otpauth://totp/Other:x?secret=GEZDGNBV",
          }),
        ),
      ).toBe("duplicate_entry");
      expect(
        await errorCode(backend.dispatch({ command: "entry_add_uri", uri: "https://x" })),
      ).toBe("invalid_uri");
      const manual = await backend.dispatch({
        command: "entry_add_manual",
        draft: {
          secret: "mzxw 6ytb oi",
          kind: { type: "hotp", counter: 0 },
          issuer: "Bank",
          group: " Work ",
        },
      });
      expect(
        await errorCode(
          backend.dispatch({
            command: "entry_add_manual",
            draft: { secret: "1!", kind: { type: "hotp", counter: 0 } },
          }),
        ),
      ).toBe("invalid_secret");
      await backend.dispatch({
        command: "entry_update",
        id: added.id,
        patch: { issuer: " Mail2 ", favorite: true, group: "" },
      });
      await backend.dispatch({ command: "entry_hotp_next", id: manual.id });
      expect(await errorCode(backend.dispatch({ command: "entry_hotp_next", id: added.id }))).toBe(
        "invalid_parameters",
      );
      const state = await backend.getState();
      expect(state.entries.find((e) => e.id === added.id)?.issuer).toBe("Mail2");
      expect(state.entries.find((e) => e.id === manual.id)?.kind).toEqual({
        type: "hotp",
        counter: 1,
      });
      expect(state.entries.find((e) => e.id === manual.id)?.group).toBe("Work");
      await backend.dispatch({ command: "entry_delete", id: added.id });
      expect(await errorCode(backend.dispatch({ command: "entry_delete", id: added.id }))).toBe(
        "entry_not_found",
      );
      off();
    } finally {
      vi.useRealTimers();
    }
  });

  it("copies, reveals and exports behind the password", async () => {
    const backend = new MockBackend({ entries: sampleEntries() });
    const { notices } = recorder(backend);
    const [first] = (await backend.getState()).entries;
    if (!first) throw new Error("no sample");
    await backend.dispatch({ command: "entry_copy", id: first.id });
    expect(notices()[0]).toEqual({ type: "copied", entry_id: first.id, clear_after_s: 30 });
    expect(
      await errorCode(backend.dispatch({ command: "entry_reveal", id: first.id, password: "no" })),
    ).toBe("wrong_password");
    const revealed = await backend.dispatch({
      command: "entry_reveal",
      id: first.id,
      password: MOCK_PASSWORD,
    });
    expect(revealed.svg).toContain("<svg");
    const ids = (await backend.getState()).entries.map((e) => e.id);
    const started = await backend.dispatch({
      command: "export_start",
      target: "google",
      entry_ids: ids,
      password: MOCK_PASSWORD,
    });
    expect(started.excluded.length).toBeGreaterThan(0);
    const page = await backend.dispatch({
      command: "export_page",
      session: started.session,
      index: 0,
    });
    expect(page.total).toBe(started.pages);
    await backend.dispatch({ command: "export_close", session: started.session });
    expect(
      await errorCode(
        backend.dispatch({ command: "export_page", session: started.session, index: 0 }),
      ),
    ).toBe("export_expired");
    const microsoft = await backend.dispatch({
      command: "export_start",
      target: "microsoft",
      entry_ids: ids,
      password: MOCK_PASSWORD,
    });
    expect(microsoft.pages).toBeGreaterThan(1);
    expect(
      await errorCode(
        backend.dispatch({
          command: "export_start",
          target: "google",
          entry_ids: [],
          password: MOCK_PASSWORD,
        }),
      ),
    ).toBe("export_nothing");
    expect(await backend.exportOtpauthFile(ids, MOCK_PASSWORD)).toBe("lockra-export.txt");
    expect(await errorCode(backend.exportOtpauthFile(["missing"], MOCK_PASSWORD))).toBe(
      "export_nothing",
    );
  });

  it("previews and commits imports", async () => {
    const backend = new MockBackend({
      entries: [mockEntry("GitHub", "octocat", { secret: "JBSWY3DPEHPK3PXP" })],
      clipboard: "nothing here",
    });
    expect(await errorCode(backend.dispatch({ command: "import_clipboard" }))).toBe(
      "clipboard_empty",
    );
    backend.setClipboard("otpauth://totp/Clip:board?secret=MFRGGZDF");
    await backend.dispatch({ command: "import_clipboard" });
    await backend.dispatch({
      command: "import_text",
      text: "otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP\notpauth://totp/github:OCTOCAT?secret=MZXW6YTBOI\nnot a link",
    });
    expect(await errorCode(backend.dispatch({ command: "import_text", text: "\n# nothing" }))).toBe(
      "import_empty",
    );
    await backend.pickImportFiles();
    const preview = (await backend.getState()).import;
    expect(preview?.candidates.map((c) => c.status.type)).toEqual([
      "new",
      "exists",
      "conflict",
      "unsupported",
      "new",
      "conflict",
      "new",
    ]);
    expect(preview?.google_batches[0]?.missing).toEqual([1]);
    backend.awaitBackup("other.lockrabackup");
    expect(
      await errorCode(backend.dispatch({ command: "import_backup_password", password: "x" })),
    ).toBe("wrong_password");
    await backend.dispatch({ command: "import_backup_password", password: MOCK_PASSWORD });
    const outcome = await backend.dispatch({
      command: "import_commit",
      choices: [
        { id: 2, action: "replace" },
        { id: 4, action: "skip" },
      ],
    });
    expect(outcome.replaced).toBe(1);
    expect(outcome.added).toBeGreaterThan(2);
    expect(await errorCode(backend.dispatch({ command: "import_commit" }))).toBe("no_import");
    await backend.dispatch({ command: "import_text", text: "otpauth://totp/A:b?secret=MFRGGZA" });
    await backend.dispatch({ command: "import_cancel" });
    expect((await backend.getState()).import).toBeNull();
  });

  it("backs up and restores", async () => {
    const backend = new MockBackend({ entries: sampleEntries().slice(0, 2) });
    const { notices } = recorder(backend);
    expect(await errorCode(backend.saveBackup("short"))).toBe("password_too_short");
    expect(await backend.saveBackup()).toBe("lockra-backup.lockrabackup");
    expect(await errorCode(backend.dispatch({ command: "backup_auto_now" }))).toBe(
      "backup_dir_missing",
    );
    expect(await backend.pickBackupDir()).toContain("Lockra");
    await backend.dispatch({ command: "backup_auto_now" });
    expect((await backend.getState()).backup.last_auto_file).toMatch(/^lockra-auto-/);
    backend.failAutoBackup("backup_dir_unavailable");
    expect((await backend.getState()).backup.last_auto_error?.code).toBe("backup_dir_unavailable");
    expect(
      await errorCode(
        backend.dispatch({ command: "restore_commit", password: MOCK_PASSWORD, mode: "merge" }),
      ),
    ).toBe("no_restore");
    await backend.pickRestoreFile();
    await backend.dispatch({ command: "restore_commit", password: MOCK_PASSWORD, mode: "merge" });
    expect((await backend.getState()).import?.candidates.length).toBe(3);
    await backend.pickRestoreFile();
    await backend.dispatch({ command: "restore_commit", password: MOCK_PASSWORD, mode: "replace" });
    expect((await backend.getState()).entries).toHaveLength(3);
    await backend.pickRestoreFile();
    await backend.dispatch({ command: "restore_cancel" });
    expect((await backend.getState()).restore).toBeNull();
    expect(notices().some((n) => n.type === "restored")).toBe(true);
    const fresh = new MockBackend();
    await fresh.pickRestoreFile();
    await fresh
      .dispatch({ command: "restore_commit", password: "the backup password", mode: "merge" })
      .catch(() => undefined);
    await fresh.dispatch({ command: "restore_commit", password: MOCK_PASSWORD, mode: "merge" });
    expect((await fresh.getState()).phase).toBe("unlocked");
  });

  it("device unlock, password change, reset and settings", async () => {
    const backend = new MockBackend({ entries: sampleEntries().slice(0, 1) });
    await backend.dispatch({ command: "device_unlock_enable" });
    await backend.dispatch({ command: "vault_lock" });
    await backend.dispatch({ command: "vault_unlock_device" });
    expect(
      await errorCode(backend.dispatch({ command: "device_unlock_disable", password: "x" })),
    ).toBe("wrong_password");
    await backend.dispatch({ command: "device_unlock_disable", password: MOCK_PASSWORD });
    expect(
      await errorCode(
        backend.dispatch({ command: "device_unlock_disable", password: MOCK_PASSWORD }),
      ),
    ).toBe("device_unlock_off");
    await backend.dispatch({
      command: "vault_change_password",
      current: MOCK_PASSWORD,
      new: "a newer password",
    });
    await backend.dispatch({ command: "vault_lock" });
    expect(await errorCode(backend.dispatch({ command: "vault_unlock_device" }))).toBe(
      "device_unlock_off",
    );
    expect(await errorCode(backend.dispatch({ command: "entry_copy", id: "x" }))).toBe("locked");
    await backend.dispatch({ command: "vault_reset" });
    expect((await backend.getState()).phase).toBe("no_vault");
    expect(await errorCode(backend.dispatch({ command: "vault_unlock", password: "x" }))).toBe(
      "no_vault",
    );
    const settings = (await backend.getState()).settings;
    await backend.dispatch({
      command: "settings_set",
      settings: { ...settings, font_size_px: 40, theme: "graphite" },
    });
    expect((await backend.getState()).settings.font_size_px).toBe(18);
    expect(
      await errorCode(
        backend.dispatch({
          command: "settings_set",
          settings: { ...settings, auto_backup: { enabled: true, dir: null, keep: 3 } },
        }),
      ),
    ).toBe("backup_dir_missing");
    await backend.dispatch({ command: "activity" });
    await backend.dispatch({ command: "secret_view_closed" });
    expect(backend.calls.length).toBeGreaterThan(10);
    const noKeychain = new MockBackend({
      entries: sampleEntries().slice(0, 1),
      keychainAvailable: false,
    });
    expect(await errorCode(noKeychain.dispatch({ command: "device_unlock_enable" }))).toBe(
      "keychain_unavailable",
    );
  });

  it("updates the way the core does", async () => {
    const none = new MockBackend();
    expect((await none.getState()).update).toEqual({ method: null, status: { state: "idle" } });
    expect(await errorCode(none.dispatch({ command: "update_check" }))).toBe("update_unavailable");

    const backend = new MockBackend({ updateMethod: "rpm", now: () => 1_790_000_000_000 });
    const seen: string[] = [];
    backend.on((event: UiEvent) => {
      if (event.type === "state") seen.push(event.state.update.status.state);
    });
    await backend.dispatch({ command: "update_install" });
    expect((await backend.getState()).update.status).toEqual({
      state: "up_to_date",
      checked_at_ms: 1_790_000_000_000,
    });
    backend.setRelease({ version: "0.2.0", notes: "## notes", date: null, size: 100 });
    await backend.dispatch({ command: "update_check" });
    await backend.dispatch({ command: "update_install" });
    expect(seen).toEqual([
      "checking",
      "up_to_date",
      "checking",
      "available",
      "checking",
      "downloading",
      "downloading",
      "downloading",
      "installing",
    ]);
    expect(await errorCode(backend.dispatch({ command: "update_check" }))).toBe("update_busy");

    const failing = new MockBackend({
      updateMethod: "app",
      release: { version: "0.2.0", notes: null, date: null, size: 1 },
      updateFailure: { step: "install", code: "update_cancelled" },
    });
    await failing.dispatch({ command: "update_install" });
    expect((await failing.getState()).update.status).toMatchObject({
      state: "failed",
      code: "update_cancelled",
    });
    const notices: Notice[] = [];
    failing.on((event: UiEvent) => {
      if (event.type === "notice") notices.push(event.notice);
    });
    failing.announceUpdate("0.2.0");
    expect(notices).toEqual([{ type: "update_available", version: "0.2.0" }]);
  });

  it("stand-in codes are stable and padded", () => {
    expect(fakeCode("A", 1, 6)).toBe(fakeCode("A", 1, 6));
    expect(fakeCode("A", 1, 8)).toMatch(/^\d{8}$/);
    expect(fakeCode("A", 1, 6)).not.toBe(fakeCode("A", 2, 6));
  });
});
