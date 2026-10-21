import { isLockraError } from "./backend";
import {
  MOCK_PASSWORD,
  MOCK_STORAGE_SECRET,
  MOCK_SYNC_FOLDER,
  MOCK_OLD_INVITE,
  MOCK_SPACE_INVITE,
  MOCK_SYNC_KEY,
  MockBackend,
  fakeCode,
  mockEntry,
  mockItemView,
  mockSyncSpace,
  sampleBrowser,
  sampleEntries,
} from "./mock-backend";
import type { CodesFrame, JoinSource, Notice, StorageConfig, UiEvent } from "./schema";

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

  it("adds the one account the clipboard or an image holds, at once", async () => {
    const backend = new MockBackend({ phase: "unlocked", clipboard: "nothing here" });
    expect(await errorCode(backend.dispatch({ command: "entry_add_clipboard" }))).toBe(
      "clipboard_empty",
    );
    backend.setClipboard(
      "otpauth://totp/A:a?secret=MFRGGZDF\notpauth://totp/B:b?secret=MZXW6YTBOI",
    );
    expect(await errorCode(backend.dispatch({ command: "entry_add_clipboard" }))).toBe(
      "several_accounts",
    );
    backend.setClipboard("otpauth://totp/Clip:board?secret=MFRGGZDF");
    const { id } = await backend.dispatch({ command: "entry_add_clipboard" });
    const added = (await backend.getState()).entries.find((e) => e.id === id);
    expect([added?.issuer, added?.account, added?.origin]).toEqual(["Clip", "board", "uri"]);
    expect((await backend.getState()).import).toBeNull();
    // An image's QR code; the dialog cancelled; one with two accounts.
    backend.setImagePick("otpauth://totp/Shot:me?secret=JBSWY3DPEHPK3PXP&issuer=Shot");
    const shot = await backend.addEntryFromImage();
    expect((await backend.getState()).entries.find((e) => e.id === shot)?.issuer).toBe("Shot");
    backend.setImagePick(null);
    expect(await backend.addEntryFromImage()).toBeNull();
    backend.setImagePick("otpauth://totp/A:a?secret=GEZDGNBV\notpauth://totp/B:b?secret=MFRGGZDF");
    expect(await errorCode(backend.addEntryFromImage())).toBe("several_accounts");
    await backend.dispatch({ command: "vault_lock" });
    expect(await errorCode(backend.dispatch({ command: "entry_add_clipboard" }))).toBe("locked");
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

  it("previews a password manager's logins with their codes, and commits them linked", async () => {
    const router = { view: mockItemView("Router", { username: "admin" }), password: "old" };
    const backend = new MockBackend({ phase: "unlocked", passwords: true, items: [router] });
    const { notices } = recorder(backend);
    expect(await backend.pickImportFiles("logins")).toBe(true);
    const preview = (await backend.getState()).import;
    expect(
      preview?.candidates.map((c) => [c.login?.title ?? c.issuer ?? c.account, c.status.type]),
    ).toEqual([
      ["GitHub", "new"],
      ["GitHub", "new"],
      ["Router", "login_conflict"],
      ["", "unsupported"],
    ]);
    expect(preview?.candidates[0]?.login?.totp).toBe(1);
    expect(preview?.candidates[2]?.default_action).toBe("replace");
    expect(JSON.stringify(preview)).not.toContain("mock-imported");
    const outcome = await backend.dispatch({ command: "import_commit" });
    expect(outcome).toEqual({
      added: 1,
      replaced: 0,
      skipped: 0,
      logins: { added: 1, replaced: 1, skipped: 1 },
    });
    expect(notices().at(-1)).toEqual({ type: "imported", ...outcome });
    const items = await backend.dispatch({ command: "items_list" });
    const github = items.find((i) => i.title === "GitHub");
    const code = (await backend.getState()).entries.find((e) => e.issuer === "GitHub");
    expect(github?.totp_entry).toBe(code?.id);
    expect(items.find((i) => i.title === "Router")?.id).toBe(router.view.id);
    // Without password management there are no exports of logins to pick.
    const off = new MockBackend({ phase: "unlocked" });
    expect(await errorCode(off.pickImportFiles("logins"))).toBe("feature_unavailable");
  });

  it("opens a KeePass database waiting for its password, with a key file or without", async () => {
    const backend = new MockBackend({ phase: "unlocked", passwords: true });
    expect(await errorCode(backend.pickKeyFile())).toBe("no_import");
    expect(await errorCode(backend.dispatch({ command: "import_key_file_clear" }))).toBe(
      "no_import",
    );
    backend.awaitKdbx("Passwords.kdbx");
    const awaiting = async () => (await backend.getState()).import?.awaiting;
    expect(await awaiting()).toEqual({ name: "Passwords.kdbx", kind: "kdbx", key_file: null });
    expect(
      await errorCode(backend.dispatch({ command: "import_backup_password", password: "x" })),
    ).toBe("wrong_password");
    expect(await backend.pickKeyFile()).toBe(true);
    expect((await awaiting())?.key_file).toBe("Passwords.keyx");
    expect(
      await errorCode(backend.dispatch({ command: "import_backup_password", password: "x" })),
    ).toBe("import_key_file_invalid");
    await backend.dispatch({ command: "import_key_file_clear" });
    expect((await awaiting())?.key_file).toBeNull();
    await backend.dispatch({ command: "import_backup_password", password: MOCK_PASSWORD });
    const preview = (await backend.getState()).import;
    expect(preview?.awaiting).toBeNull();
    expect(preview?.candidates.filter((c) => c.login !== null)).toHaveLength(2);
    expect(preview?.candidates[0]?.source).toEqual({ type: "file", name: "Passwords.kdbx" });
    // A backup waiting takes no key file.
    backend.awaitBackup("old.lockrabackup");
    expect(await errorCode(backend.pickKeyFile())).toBe("no_import");
  });

  it("exports logins once the user, the choice and a plain file's risk are checked", async () => {
    const github = { view: mockItemView("GitHub"), password: "github password" };
    const backend = new MockBackend({ phase: "unlocked", passwords: true, items: [github] });
    const ids = [github.view.id];
    const csv = { ids, format: "csv", password: MOCK_PASSWORD, acknowledged: true } as const;
    const kdbx = {
      ...csv,
      format: "kdbx",
      acknowledged: false,
      kdbxPassword: "a database pw",
    } as const;
    expect(await errorCode(backend.exportItemsFile({ ...csv, ids: ["nope"] }))).toBe(
      "export_nothing",
    );
    expect(await errorCode(backend.exportItemsFile({ ...csv, acknowledged: false }))).toBe(
      "export_acknowledge_needed",
    );
    expect(await errorCode(backend.exportItemsFile({ ...kdbx, kdbxPassword: "short" }))).toBe(
      "password_too_short",
    );
    expect(await errorCode(backend.exportItemsFile({ ...csv, password: "wrong" }))).toBe(
      "wrong_password",
    );
    expect(await errorCode(backend.exportItemsFile({ ...csv, password: undefined }))).toBe(
      "biometric_unavailable",
    );
    expect(await backend.exportItemsFile(csv)).toBe("lockra-logins.csv");
    expect(await backend.exportItemsFile(kdbx)).toBe("lockra-logins.kdbx");
    backend.cancelNextSave = true;
    expect(await backend.exportItemsFile(kdbx)).toBeNull();
    expect(backend.exportedItems).toEqual([
      { ids, format: "csv" },
      { ids, format: "kdbx" },
    ]);
    // Without password management, nothing of it.
    const off = new MockBackend({ phase: "unlocked" });
    expect(await errorCode(off.exportItemsFile(csv))).toBe("feature_unavailable");
  });

  it("scans with the camera into the preview, or says why it could not", async () => {
    const TEXTS = { prompt: "Point at a code", cancel: "Cancel" };
    const backend = new MockBackend({ phase: "unlocked" });
    expect(await backend.scanImport(TEXTS)).toBe(false);
    backend.setScan("otpauth://totp/Cam:era?secret=MFRGGZDF");
    expect(await backend.scanImport(TEXTS)).toBe(true);
    const preview = (await backend.getState()).import;
    expect(preview?.candidates.map((c) => [c.issuer, c.source.type])).toEqual([["Cam", "camera"]]);
    backend.setScan({ error: "camera_denied" });
    expect(await errorCode(backend.scanImport(TEXTS))).toBe("camera_denied");
  });

  it("joins a space from the invitation the camera reads, or says why it could not", async () => {
    const TEXTS = { prompt: "Point at the invitation", cancel: "Cancel" };
    const join = { password: MOCK_PASSWORD, deviceName: "Phone" };
    const backend = new MockBackend({ phase: "no_vault" });
    expect(await backend.scanJoin(TEXTS, join)).toBe(false);
    backend.setScan("otpauth://totp/Cam:era?secret=MFRGGZDF");
    expect(await errorCode(backend.scanJoin(TEXTS, join))).toBe("sync_invite_invalid");
    backend.setScan({ error: "camera_denied" });
    expect(await errorCode(backend.scanJoin(TEXTS, join))).toBe("camera_denied");
    backend.setScan("lockra-invite:1:bW9jaw");
    expect(await backend.scanJoin(TEXTS, join)).toBe(true);
    const state = await backend.getState();
    expect(state.phase).toBe("unlocked");
    expect(state.sync.space?.device_name).toBe("Phone");
  });

  it("checks for a newer release on the phone and opens its page, never installing", async () => {
    const release = { version: "0.7.0", notes: null, date: null, size: 1 };
    const backend = new MockBackend({ phase: "unlocked", updateMethod: "android", release });
    await backend.dispatch({ command: "update_check" });
    expect((await backend.getState()).update.status).toMatchObject({
      state: "available",
      version: "0.7.0",
    });
    expect(await errorCode(backend.dispatch({ command: "update_install" }))).toBe(
      "update_unavailable",
    );
    expect(await backend.openRelease()).toBeNull();
    const noBrowser = new MockBackend({ updateMethod: "android", releasePageOpens: false });
    expect(await noBrowser.openRelease()).toBe("https://github.com/sunerpy/lockra/releases/latest");
  });

  it("leaves a phone from Google Play to Play: no check, and its listing opens", async () => {
    const release = { version: "0.7.0", notes: null, date: null, size: 1 };
    const backend = new MockBackend({ phase: "unlocked", updateMethod: "play", release });
    for (const command of ["update_check", "update_install"] as const)
      expect(await errorCode(backend.dispatch({ command }))).toBe("update_unavailable");
    expect((await backend.getState()).update.status).toEqual({ state: "idle" });
    expect(await backend.openRelease()).toBeNull();
    const noStore = new MockBackend({ updateMethod: "play", releasePageOpens: false });
    expect(await noStore.openRelease()).toBe(
      "https://play.google.com/store/apps/details?id=dev.lockra.mobile",
    );
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
    // The install goes on from what the check found: no second check.
    await backend.dispatch({ command: "update_install" });
    expect(seen).toEqual([
      "checking",
      "up_to_date",
      "checking",
      "available",
      "downloading",
      "downloading",
      "downloading",
      "ready",
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
  });

  it("turning automatic updates on downloads to ready and installs on the restart", async () => {
    const backend = new MockBackend({
      updateMethod: "nsis",
      release: { version: "0.3.0", notes: null, date: null, size: 10 },
    });
    const seen: string[] = [];
    backend.on((event: UiEvent) => {
      if (event.type === "state") seen.push(event.state.update.status.state);
    });
    const settings = (await backend.getState()).settings;
    await backend.dispatch({
      command: "settings_set",
      settings: { ...settings, auto_update: true },
    });
    expect((await backend.getState()).update.status).toEqual({ state: "ready", version: "0.3.0" });
    await backend.dispatch({ command: "update_install" });
    expect(seen.filter((state, i) => state !== seen[i - 1])).toEqual([
      "idle",
      "checking",
      "available",
      "downloading",
      "ready",
      "installing",
    ]);
    // A test can put the updater anywhere a run goes.
    backend.simulateUpdate({ state: "failed", code: "update_network", at_ms: 1 });
    expect((await backend.getState()).update.status).toEqual({
      state: "failed",
      code: "update_network",
      at_ms: 1,
    });
    // Left off, the switch goes nowhere.
    const off = new MockBackend({ updateMethod: "deb" });
    await off.dispatch({
      command: "settings_set",
      settings: { ...(await off.getState()).settings, auto_update: false },
    });
    expect((await off.getState()).update.status).toEqual({ state: "idle" });
  });

  it("backs up automatically before a space moves to a place that holds nothing of it", async () => {
    const backend = new MockBackend({ entries: sampleEntries(), now: () => Date.UTC(2026, 9, 2) });
    const { notices } = recorder(backend);
    const webdav = (url: string): StorageConfig => ({
      kind: "webdav",
      url,
      prefix: "",
      username: "me",
      password: MOCK_STORAGE_SECRET,
    });
    await backend.dispatch({
      command: "sync_create",
      storage: webdav("https://dav.example.com/dav/"),
      password: MOCK_PASSWORD,
      device_name: "Laptop",
    });
    await backend.pickBackupDir();
    const settings = (await backend.getState()).settings;
    await backend.dispatch({
      command: "settings_set",
      settings: { ...settings, auto_backup: { ...settings.auto_backup, enabled: true } },
    });
    const move = (url: string) =>
      backend.dispatch({
        command: "sync_set_storage",
        storage: webdav(url),
        password: MOCK_PASSWORD,
        move_here: true,
      });
    // Where the space is, nothing moves and nothing is backed up.
    await move("https://dav.example.com/dav");
    expect(notices().filter((n) => n.type === "backup_written")).toHaveLength(0);
    await move("https://dav.jianguoyun.com/dav/");
    expect(notices().filter((n) => n.type === "backup_written")).toEqual([
      {
        type: "backup_written",
        file_name: "lockra-auto-20261001-081500.lockrabackup",
        automatic: true,
      },
    ]);
    expect((await backend.getState()).sync.space?.storage).toMatchObject({
      url: "https://dav.jianguoyun.com/dav/",
    });
  });

  it("sets up sync on storage of the user's own and shows the space only while unlocked", async () => {
    const backend = new MockBackend({ entries: sampleEntries(), now: () => Date.UTC(2026, 9, 2) });
    const s3 = (secret: string, endpoint = "https://s3.example.com"): StorageConfig => ({
      kind: "s3",
      endpoint,
      region: "us-east-1",
      bucket: "lockra",
      prefix: "",
      access_key_id: "AKID",
      secret_access_key: secret,
      path_style: false,
    });
    const create = (storage: StorageConfig, password = MOCK_PASSWORD) =>
      backend.dispatch({
        command: "sync_create",
        storage,
        password,
        device_name: "  Work laptop ",
      });
    expect(await errorCode(backend.dispatch({ command: "sync_now" }))).toBe("sync_off");
    expect(await errorCode(create(s3("wrong")))).toBe("sync_denied");
    expect(await errorCode(create(s3(MOCK_STORAGE_SECRET, "http://192.168.1.2")))).toBe(
      "sync_insecure",
    );
    expect(await errorCode(create(s3(MOCK_STORAGE_SECRET, "not a url")))).toBe(
      "sync_config_invalid",
    );
    expect(await errorCode(create(s3("")))).toBe("sync_config_invalid");
    expect(await errorCode(create(s3(MOCK_STORAGE_SECRET), "wrong password"))).toBe(
      "wrong_password",
    );
    // Setting up shows no key: Settings › Sync reminds of it, and shows it on request.
    expect(await create(s3(MOCK_STORAGE_SECRET, "http://127.0.0.1:9000"))).toBeNull();
    expect((await backend.getState()).sync.space?.key_saved).toBe(false);
    expect(
      await errorCode(backend.dispatch({ command: "sync_key_reveal", password: "wrong password" })),
    ).toBe("wrong_password");
    expect(await backend.dispatch({ command: "sync_key_reveal", password: MOCK_PASSWORD })).toEqual(
      { sync_key: MOCK_SYNC_KEY },
    );
    let space = (await backend.getState()).sync.space;
    expect(space?.status.state).toBe("synced");
    expect(space?.device_name).toBe("Work laptop");
    expect(JSON.stringify(space)).not.toContain(MOCK_STORAGE_SECRET);
    expect(await errorCode(create(s3(MOCK_STORAGE_SECRET)))).toBe("sync_already_on");

    const invite = await backend.dispatch({ command: "sync_invite", password: MOCK_PASSWORD });
    expect(invite.invite).toMatch(/^lockra-invite:1:/);
    expect(invite).not.toHaveProperty("sync_key");
    backend.simulateSync({ state: "failed", code: "sync_network", at_ms: 1 });
    expect((await backend.getState()).sync.space?.status.state).toBe("failed");
    await backend.dispatch({ command: "sync_now" });
    expect((await backend.getState()).sync.space?.status.state).toBe("synced");
    await backend.dispatch({ command: "sync_rename_device", name: " \u0007 " });
    expect((await backend.getState()).sync.space?.device_name).toBe("Linux");
    const webdav: StorageConfig = {
      kind: "webdav",
      url: "https://dav.example.com/dav/",
      prefix: "lockra",
      username: "me",
      password: MOCK_STORAGE_SECRET,
    };
    // Another place holds nothing of the space: refused until the user confirms the move.
    expect(
      await errorCode(
        backend.dispatch({ command: "sync_set_storage", storage: webdav, password: MOCK_PASSWORD }),
      ),
    ).toBe("sync_space_not_found");
    expect((await backend.getState()).sync.space?.storage.kind).toBe("s3");
    await backend.dispatch({
      command: "sync_set_storage",
      storage: webdav,
      password: MOCK_PASSWORD,
      move_here: true,
    });
    expect((await backend.getState()).sync.space?.storage).toEqual({
      kind: "webdav",
      url: "https://dav.example.com/dav/",
      prefix: "lockra",
      username: "me",
    });
    // The same place written another way, or the place it moved from: no move.
    await backend.dispatch({
      command: "sync_set_storage",
      storage: { ...webdav, url: "https://dav.example.com/dav" },
      password: MOCK_PASSWORD,
    });
    await backend.dispatch({
      command: "sync_set_storage",
      storage: s3(MOCK_STORAGE_SECRET, "http://127.0.0.1:9000"),
      password: MOCK_PASSWORD,
    });
    expect((await backend.getState()).sync.space?.storage.kind).toBe("s3");
    await backend.dispatch({
      command: "sync_set_storage",
      storage: webdav,
      password: MOCK_PASSWORD,
    });

    await backend.dispatch({ command: "vault_lock" });
    expect((await backend.getState()).sync.space).toBeNull();
    await backend.dispatch({ command: "vault_unlock", password: MOCK_PASSWORD });
    space = (await backend.getState()).sync.space;
    expect(space?.devices).toHaveLength(1);
    expect(
      await errorCode(
        backend.dispatch({ command: "sync_remove_device", tag: space?.devices[0]?.tag ?? "" }),
      ),
    ).toBe("internal");
    await backend.dispatch({ command: "sync_disable" });
    expect((await backend.getState()).sync.space).toBeNull();
  });

  it("joins a space from an invitation or the sync key, making a vault where there was none", async () => {
    const fresh = new MockBackend();
    const storage: StorageConfig = {
      kind: "webdav",
      url: "https://dav.example.com/dav/",
      prefix: "",
      username: "me",
      password: MOCK_STORAGE_SECRET,
    };
    const join = (
      backend: MockBackend,
      source: JoinSource,
      password = MOCK_PASSWORD,
      space_password?: string,
    ) =>
      backend.dispatch({
        command: "sync_join",
        source,
        password,
        device_name: "Phone",
        space_password,
      });
    expect(await errorCode(join(fresh, { type: "invite", text: "otpauth://x" }))).toBe(
      "sync_invite_invalid",
    );
    expect(await errorCode(join(fresh, { type: "manual", storage, sync_key: "LKS1-ABC" }))).toBe(
      "sync_key_invalid",
    );
    expect(
      await errorCode(join(fresh, { type: "manual", storage, sync_key: MOCK_SYNC_KEY }, "short")),
    ).toBe("password_too_short");
    expect(
      await errorCode(
        join(fresh, { type: "manual", storage, sync_key: MOCK_SYNC_KEY }, "a wrong password"),
      ),
    ).toBe("sync_wrong_credentials");
    await join(fresh, { type: "manual", storage, sync_key: MOCK_SYNC_KEY });
    const state = await fresh.getState();
    expect(state.phase).toBe("unlocked");
    expect(state.entries.length).toBeGreaterThan(0);
    expect(state.sync.space?.devices.map((d) => [d.name, d.this_device])).toEqual([
      ["Phone", true],
      ["Pixel 8", false],
    ]);
    const tag = state.sync.space?.devices[1]?.tag ?? "";
    await fresh.dispatch({ command: "sync_remove_device", tag });
    expect((await fresh.getState()).sync.space?.devices).toHaveLength(1);

    const unlocked = new MockBackend({ entries: [mockEntry("Bank", "card")] });
    const invite: JoinSource = { type: "invite", text: "lockra-invite:1:abc" };
    // An invitation hands the space over: this vault's own master password is all it checks.
    expect(await errorCode(join(unlocked, invite, "a wrong password"))).toBe("wrong_password");
    await join(unlocked, invite);
    expect((await unlocked.getState()).entries.map((e) => e.issuer)).toEqual(["Bank"]);
    expect(await errorCode(join(unlocked, { type: "invite", text: "lockra-invite:1:abc" }))).toBe(
      "sync_already_on",
    );
    // An invitation from a device of the space this vault is in brings its storage settings.
    await unlocked.dispatch({
      command: "sync_set_storage",
      storage: { kind: "relay", url: "https://relay.example.com" },
      password: MOCK_PASSWORD,
    });
    const own: JoinSource = { type: "invite", text: MOCK_SPACE_INVITE };
    expect(await errorCode(join(unlocked, own, "a wrong password"))).toBe("wrong_password");
    expect((await unlocked.getState()).sync.space?.storage.kind).toBe("relay");
    await join(unlocked, own);
    expect((await unlocked.getState()).sync.space?.storage).toEqual({
      kind: "webdav",
      url: "https://dav.example.com/dav/",
      prefix: "lockra",
      username: "me@example.com",
    });
    await unlocked.dispatch({ command: "vault_lock" });
    expect(await errorCode(join(unlocked, { type: "invite", text: "lockra-invite:1:abc" }))).toBe(
      "locked",
    );
  });

  it("joins from an invitation under a new device's own master password", async () => {
    const join = (backend: MockBackend, text: string, password: string) =>
      backend.dispatch({
        command: "sync_join",
        source: { type: "invite", text },
        password,
        device_name: "Phone",
      });
    const phone = new MockBackend({ phase: "no_vault" });
    // An invitation of Lockra up to 0.8.2 carries no key: the inviting device updates first.
    expect(await errorCode(join(phone, MOCK_OLD_INVITE, "the phone's password"))).toBe(
      "sync_invite_outdated",
    );
    expect(await errorCode(join(phone, "lockra-invite:1:abc", "short"))).toBe("password_too_short");
    await join(phone, "lockra-invite:1:abc", "the phone's password");
    await phone.dispatch({ command: "vault_lock" });
    expect(
      await errorCode(phone.dispatch({ command: "vault_unlock", password: MOCK_PASSWORD })),
    ).toBe("wrong_password");
    await phone.dispatch({ command: "vault_unlock", password: "the phone's password" });
    expect((await phone.getState()).sync.space?.devices).toHaveLength(2);
  });

  it("keeps a space in the folder its dialog chose and invites with the sync key alone", async () => {
    const windows = new MockBackend({ entries: [mockEntry("GitHub", "octocat")] });
    const folder: StorageConfig = { kind: "folder" };
    const create = (backend: MockBackend) =>
      backend.dispatch({
        command: "sync_create",
        storage: folder,
        password: MOCK_PASSWORD,
        device_name: "Windows",
      });
    expect(await errorCode(create(windows))).toBe("sync_folder_not_chosen");
    expect(await windows.pickSyncFolder()).toBe(MOCK_SYNC_FOLDER);
    await create(windows);
    expect((await windows.getState()).sync.space?.storage).toEqual({
      kind: "folder",
      path: MOCK_SYNC_FOLDER,
    });
    const invite = await windows.dispatch({ command: "sync_invite", password: MOCK_PASSWORD });
    expect(invite.includes_storage).toBe(false);

    // A phone with the invitation alone is asked how it reaches the space, then joins over WebDAV.
    const phone = new MockBackend({ phase: "no_vault", scan: invite.invite });
    const dav: StorageConfig = {
      kind: "webdav",
      url: "https://dav.jianguoyun.com/dav/",
      prefix: "我的坚果云/Lockra",
      username: "me@example.com",
      password: MOCK_STORAGE_SECRET,
    };
    const texts = { prompt: "Point at it", cancel: "Cancel" };
    const join = { password: MOCK_PASSWORD, deviceName: "Phone" };
    expect(await errorCode(phone.scanJoin(texts, join))).toBe("sync_invite_needs_storage");
    expect((await phone.getState()).phase).toBe("no_vault");
    expect(await phone.scanJoin(texts, { ...join, storage: dav })).toBe(true);
    expect((await phone.getState()).sync.space?.storage.kind).toBe("webdav");
    // The sealed text holds the key alone too.
    const mac = new MockBackend({ phase: "no_vault", folder: null });
    expect(await mac.pickSyncFolder()).toBeNull();
    expect(invite.shared).not.toBeNull();
    const sealed: JoinSource = {
      type: "invite",
      text: invite.shared?.text ?? "",
      code: invite.shared?.code,
    };
    const joinMac = (source: JoinSource) =>
      mac.dispatch({ command: "sync_join", source, password: MOCK_PASSWORD, device_name: "Mac" });
    expect(await errorCode(joinMac(sealed))).toBe("sync_invite_needs_storage");
    expect(await errorCode(joinMac({ ...sealed, storage: folder }))).toBe("sync_folder_not_chosen");
  });

  it("keeps a space on a relay by its address, and a phone joins it from the desktop's code", async () => {
    const desktop = new MockBackend({ entries: [mockEntry("GitHub", "octocat")] });
    const create = (storage: StorageConfig) =>
      desktop.dispatch({
        command: "sync_create",
        storage,
        password: MOCK_PASSWORD,
        device_name: "Desktop",
      });
    // An address alone, over HTTPS (plain HTTP to this computer only), nothing else in it.
    expect(await errorCode(create({ kind: "relay", url: "http://relay.example.com" }))).toBe(
      "sync_insecure",
    );
    for (const url of [
      "relay.example.com",
      "https://relay.example.com/?space=1",
      "https://me:pw@relay.example.com",
      "ftp://relay.example.com",
    ])
      expect(await errorCode(create({ kind: "relay", url }))).toBe("sync_config_invalid");
    await create({ kind: "relay", url: " https://relay.example.com " });
    expect((await desktop.getState()).sync.space?.storage).toEqual({
      kind: "relay",
      url: "https://relay.example.com",
    });
    const invite = await desktop.dispatch({ command: "sync_invite", password: MOCK_PASSWORD });
    expect(invite.includes_storage).toBe(true);
    // On a relay nothing is sealed: the invitation is also the pairing link, sent as it is.
    expect(invite.shared).toBeNull();
    // The phone scans it and is on the same relay; the tablet pastes the link, with no code.
    const phone = new MockBackend({ phase: "no_vault", scan: invite.invite });
    expect(
      await phone.scanJoin(
        { prompt: "Point at it", cancel: "Cancel" },
        { password: MOCK_PASSWORD, deviceName: "Phone" },
      ),
    ).toBe(true);
    expect((await phone.getState()).sync.space?.storage).toEqual({
      kind: "relay",
      url: "https://relay.example.com",
    });
    const tablet = new MockBackend({ phase: "no_vault" });
    await tablet.dispatch({
      command: "sync_join",
      source: { type: "invite", text: invite.invite },
      password: MOCK_PASSWORD,
      device_name: "Tablet",
    });
    expect((await tablet.getState()).sync.space?.storage.kind).toBe("relay");
  });

  it("puts the sealed invitation on the clipboard, and nothing else", async () => {
    const backend = new MockBackend({ entries: sampleEntries(), sync: mockSyncSpace() });
    const invite = await backend.dispatch({ command: "sync_invite", password: MOCK_PASSWORD });
    const copy = (text: string) => backend.dispatch({ command: "sync_invite_copy", text });
    const sealed = invite.shared?.text ?? "";
    expect(sealed).toMatch(/^lockra-invite:2:/);
    // The plain invitation carries the storage's credentials: only the sealed text is handed on.
    expect(await errorCode(copy(invite.invite))).toBe("sync_invite_invalid");
    expect(await errorCode(copy("a note to self"))).toBe("sync_invite_invalid");
    expect(backend.clipboardText).toBeUndefined();
    expect(await copy(sealed)).toBeNull();
    expect(backend.clipboardText).toBe(sealed);
    const alone = new MockBackend({ entries: sampleEntries() });
    expect(await errorCode(alone.dispatch({ command: "sync_invite_copy", text: sealed }))).toBe(
      "sync_off",
    );
  });

  it("puts a relay space's pairing link on the clipboard as it is, and no other", async () => {
    const relay = { kind: "relay", url: "https://relay.example.com" } as const;
    const backend = new MockBackend({
      entries: sampleEntries(),
      sync: mockSyncSpace({ storage: relay }),
    });
    const invite = await backend.dispatch({ command: "sync_invite", password: MOCK_PASSWORD });
    const copy = (text: string) => backend.dispatch({ command: "sync_invite_copy", text });
    expect(invite.shared).toBeNull();
    // Another relay's link, or anything else, is not this space's.
    const elsewhere = `lockra-invite:1:${btoa("mock-relay-invite|https://relay.other.example.com\n:1")}`;
    for (const text of ["a note to self", elsewhere])
      expect(await errorCode(copy(text))).toBe("sync_invite_invalid");
    expect(backend.clipboardText).toBeUndefined();
    expect(await copy(`  ${invite.invite}\n`)).toBeNull();
    expect(backend.clipboardText).toBe(invite.invite);
  });

  it("starts with a space when told to", async () => {
    const backend = new MockBackend({ entries: sampleEntries(), sync: mockSyncSpace() });
    expect((await backend.getState()).sync.space?.devices).toHaveLength(2);
    const locked = new MockBackend({
      phase: "locked",
      entries: sampleEntries(),
      sync: mockSyncSpace(),
    });
    expect((await locked.getState()).sync.space).toBeNull();
    await locked.dispatch({ command: "vault_lock" });
    await locked.dispatch({ command: "vault_unlock", password: MOCK_PASSWORD });
    expect((await locked.getState()).sync.space?.device_name).toBe("Desktop");
  });

  it("keeps logins without their secrets, and shows and copies them as the core does", async () => {
    const backend = new MockBackend({ passwords: true, entries: [mockEntry("GitHub", "octocat")] });
    const { notices } = recorder(backend);
    const entry = (await backend.getState()).entries[0]?.id ?? "";
    const before = (await backend.getState()).items_rev;
    const { id } = await backend.dispatch({
      command: "item_add",
      draft: {
        title: " GitHub ",
        username: "octocat",
        password: { type: "set", value: "hunter2" },
        notes: "recovery codes",
        urls: [{ url: "https://github.com/" }, { url: " " }],
        tags: ["work", "work"],
        totp_entry: entry,
        fields: [
          { label: "PIN", value: { type: "set", value: "4242" }, hidden: true },
          { label: "No.", value: { type: "set", value: "7" } },
        ],
      },
    });
    const state = await backend.getState();
    expect(state.items_count).toBe(1);
    expect(state.items_rev).toBeGreaterThan(before);
    const [view] = await backend.dispatch({ command: "items_list" });
    expect(view).toMatchObject({
      title: "GitHub",
      username: "octocat",
      has_password: true,
      has_notes: true,
      tags: ["work"],
    });
    expect(view?.urls).toEqual([{ url: "https://github.com/", matching: "domain" }]);
    expect(JSON.stringify(view)).not.toMatch(/hunter2|recovery|4242/);
    const pin = view?.fields.find((f) => f.label === "PIN")?.id ?? "";
    expect(view?.fields.find((f) => f.label === "No.")?.value).toBe("7");
    const revealed = await backend.dispatch({
      command: "item_reveal",
      id,
      parts: { password: true, notes: true, fields: [pin], history: true },
    });
    expect(revealed).toMatchObject({
      password: "hunter2",
      notes: "recovery codes",
      fields: [{ id: pin, value: "4242" }],
      history: [],
    });
    await backend.dispatch({ command: "item_copy", id, part: { type: "password" } });
    expect(backend.clipboardText).toBe("hunter2");
    expect(notices().at(-1)).toEqual({
      type: "item_copied",
      item_id: id,
      part: "password",
      clear_after_s: 30,
    });
    await backend.dispatch({ command: "item_copy", id, part: { type: "field", id: pin } });
    expect(backend.clipboardText).toBe("4242");
    await backend.dispatch({ command: "item_copy", id, part: { type: "totp" } });
    expect(backend.clipboardText).toMatch(/^\d{6}$/);
    await backend.dispatch({ command: "item_copy", id, part: { type: "username" } });
    expect(
      await errorCode(backend.dispatch({ command: "item_copy", id, part: { type: "email" } })),
    ).toBe("item_not_found");
    // An edit: a new password keeps the old one in the history; fields change and go.
    await backend.dispatch({
      command: "item_update",
      id,
      patch: {
        title: "GitHub (work)",
        email: "me@example.com",
        password: { type: "set", value: "second" },
        notes: { type: "clear" },
        urls: [{ url: "https://gist.github.com/", matching: "host" }],
        tags: [],
        group: "Dev",
        favorite: true,
        color: "teal",
        mark: "GH",
        protection: "confirm",
        autofill: { auto_submit: "on" },
        totp_entry: null,
        fields: [
          { id: pin, value: { type: "set", value: "9999" } },
          { id: view?.fields[1]?.id ?? "", remove: true },
        ],
      },
    });
    const [edited] = await backend.dispatch({ command: "items_list" });
    expect(edited).toMatchObject({
      title: "GitHub (work)",
      email: "me@example.com",
      has_notes: false,
      group: "Dev",
      favorite: true,
      history_count: 1,
      protection: "confirm",
    });
    expect(edited?.fields.map((f) => f.label)).toEqual(["PIN"]);
    // Now of the confirm level: a secret needs proof.
    expect(
      await errorCode(backend.dispatch({ command: "item_reveal", id, parts: { password: true } })),
    ).toBe("biometric_unavailable");
    const history = await backend.dispatch({
      command: "item_reveal",
      id,
      parts: { history: true },
      password: MOCK_PASSWORD,
    });
    expect(history.history?.map((p) => p.value)).toEqual(["hunter2"]);
    await backend.dispatch({
      command: "item_update",
      id,
      patch: { settle: [{ field: "password", take: history.history?.[0]?.id ?? "" }] },
    });
    expect(
      (
        await backend.dispatch({
          command: "item_reveal",
          id,
          parts: { password: true },
          password: MOCK_PASSWORD,
        })
      ).password,
    ).toBe("hunter2");
    expect(
      await errorCode(
        backend.dispatch({
          command: "item_update",
          id,
          patch: { settle: [{ field: "password", take: "nope" }] },
        }),
      ),
    ).toBe("item_not_found");
    expect(
      await errorCode(
        backend.dispatch({ command: "item_update", id, patch: { title: "x".repeat(201) } }),
      ),
    ).toBe("item_invalid");
    expect(
      await errorCode(
        backend.dispatch({
          command: "item_update",
          id,
          patch: { totp_entry: "00000000-0000-4000-8000-ffffffffffff" },
        }),
      ),
    ).toBe("entry_not_found");
    expect(
      await errorCode(
        backend.dispatch({
          command: "item_update",
          id,
          patch: { fields: [{ id: "00000000-0000-4000-8000-fffffffffffe" }] },
        }),
      ),
    ).toBe("item_not_found");
    await backend.dispatch({ command: "item_delete", ids: [id] });
    expect(await backend.dispatch({ command: "items_list" })).toEqual([]);
    expect(await errorCode(backend.dispatch({ command: "item_delete", ids: [id] }))).toBe(
      "item_not_found",
    );
    expect(await errorCode(backend.dispatch({ command: "item_reveal", id, parts: {} }))).toBe(
      "item_not_found",
    );
  });

  it("makes passwords, saves one without sending it back, and keeps its history behind proof", async () => {
    const backend = new MockBackend({
      passwords: true,
      phase: "unlocked",
      items: [{ view: mockItemView("Old", { read_only: true, kind: "bank_card" }) }],
    });
    const options = {
      kind: "password",
      length: 24,
      lower: true,
      upper: true,
      digits: true,
      symbols: false,
      avoid_ambiguous: false,
      exclude: "",
    } as const;
    expect(await errorCode(backend.dispatch({ command: "generator_copy" }))).toBe("item_invalid");
    const made = await backend.dispatch({
      command: "generator_generate",
      options,
      domain: "example.com",
    });
    expect(made.password).toHaveLength(24);
    expect(made.rules_met).toBe(true);
    await backend.dispatch({ command: "generator_copy" });
    expect(backend.clipboardText).toBe(made.password);
    expect((await backend.getState()).generator.history_count).toBe(1);
    const { id } = await backend.dispatch({
      command: "item_add",
      draft: { title: "New", password: { type: "generated" } },
    });
    expect(
      (await backend.dispatch({ command: "item_reveal", id, parts: { password: true } })).password,
    ).toBe(made.password);
    const words = await backend.dispatch({
      command: "generator_generate",
      options: { kind: "passphrase", words: 4, separator: " ", capitalize: true, number: true },
    });
    expect(words.password.split(" ")).toHaveLength(4);
    expect(await errorCode(backend.dispatch({ command: "generator_history_reveal" }))).toBe(
      "biometric_unavailable",
    );
    const history = await backend.dispatch({
      command: "generator_history_reveal",
      password: MOCK_PASSWORD,
    });
    expect(history.map((g) => g.site)).toEqual(["example.com"]);
    await backend.dispatch({ command: "generator_history_clear" });
    expect((await backend.getState()).generator.history_count).toBe(0);
    expect(
      await errorCode(
        backend.dispatch({
          command: "generator_generate",
          options: { ...options, lower: false, upper: false, digits: false },
        }),
      ),
    ).toBe("item_invalid");
    expect(await backend.dispatch({ command: "password_score", password: "password" })).toEqual({
      score: 0,
      warning: "top_ten",
    });
    expect(
      (await backend.dispatch({ command: "password_score", password: "Kx9!vQ2#mZ7@pL4&" })).score,
    ).toBe(4);
    // A login a later Lockra wrote is shown and not changed.
    const [later] = await backend.dispatch({ command: "items_list" });
    expect(
      await errorCode(
        backend.dispatch({ command: "item_update", id: later?.id ?? "", patch: { title: "x" } }),
      ),
    ).toBe("item_read_only");
    // Site rules.
    await backend.dispatch({
      command: "site_rule_add",
      kind: "never_save",
      domains: [" Example.com "],
    });
    await backend.dispatch({
      command: "site_rule_add",
      kind: "never_save",
      domains: ["example.com"],
    });
    expect(
      await errorCode(
        backend.dispatch({ command: "site_rule_add", kind: "equivalent", domains: ["a.example"] }),
      ),
    ).toBe("item_invalid");
    const rules = (await backend.getState()).site_rules;
    expect(rules).toEqual([{ id: rules[0]?.id, kind: "never_save", domains: ["example.com"] }]);
    await backend.dispatch({ command: "site_rule_remove", id: rules[0]?.id ?? "" });
    expect((await backend.getState()).site_rules).toEqual([]);
    expect(
      await errorCode(backend.dispatch({ command: "site_rule_remove", id: rules[0]?.id ?? "" })),
    ).toBe("item_not_found");
    // Locking forgets what was made, and the list moves.
    const rev = (await backend.getState()).items_rev;
    await backend.dispatch({ command: "vault_lock" });
    expect((await backend.getState()).items_rev).toBeGreaterThan(rev);
    expect(await errorCode(backend.dispatch({ command: "items_list" }))).toBe("locked");
  });

  it("matches a single site by its full host or by its domain, one way for each", async () => {
    const backend = new MockBackend({ passwords: true, phase: "unlocked" });
    const rules = async () =>
      (await backend.getState()).site_rules.map((r) => [r.kind, ...r.domains]);
    await backend.dispatch({
      command: "site_rule_add",
      kind: "never_fill",
      domains: ["git.example.com"],
    });
    await backend.dispatch({
      command: "site_rule_add",
      kind: "match_host",
      domains: [" Git.Example.com "],
    });
    await backend.dispatch({
      command: "site_rule_add",
      kind: "match_host",
      domains: ["git.example.com"],
    });
    await backend.dispatch({
      command: "site_rule_add",
      kind: "match_domain",
      domains: ["wiki.example.com"],
    });
    expect(await rules()).toEqual([
      ["never_fill", "git.example.com"],
      ["match_host", "git.example.com"],
      ["match_domain", "wiki.example.com"],
    ]);
    // The other way for the same domain replaces it; a list of another kind stays.
    await backend.dispatch({
      command: "site_rule_add",
      kind: "match_domain",
      domains: ["git.example.com"],
    });
    await backend.dispatch({
      command: "site_rule_add",
      kind: "match_host",
      domains: ["wiki.example.com"],
    });
    expect(await rules()).toEqual([
      ["never_fill", "git.example.com"],
      ["match_domain", "git.example.com"],
      ["match_host", "wiki.example.com"],
    ]);
    // One domain a rule.
    expect(
      await errorCode(
        backend.dispatch({
          command: "site_rule_add",
          kind: "match_host",
          domains: ["a.example", "b.example"],
        }),
      ),
    ).toBe("item_invalid");
    // Matching the full host by default lets the extensions do nothing more, so no proof.
    const policy = (await backend.getState()).browser.policy;
    expect(policy.match_host).toBe(false);
    await backend.dispatch({
      command: "browser_set_policy",
      policy: { ...policy, match_host: true },
    });
    expect((await backend.getState()).browser.policy.match_host).toBe(true);
    await backend.dispatch({ command: "browser_set_policy", policy });
    expect((await backend.getState()).browser.policy.match_host).toBe(false);
  });

  it("answers nothing of password management while the build does not offer it", async () => {
    const backend = new MockBackend({ phase: "unlocked" });
    expect((await backend.getState()).features.passwords).toBe(false);
    expect(await errorCode(backend.dispatch({ command: "items_list" }))).toBe(
      "feature_unavailable",
    );
    expect(await errorCode(backend.dispatch({ command: "password_score", password: "x" }))).toBe(
      "feature_unavailable",
    );
  });

  it("pairs browser extensions behind proof and keeps what they may do", async () => {
    let now = 1_790_000_000_000;
    const backend = new MockBackend({ phase: "unlocked", passwords: true, now: () => now });
    const { notices } = recorder(backend);
    expect((await backend.getState()).browser).toMatchObject({
      available: true,
      enabled: false,
      paired: [],
    });
    const request = backend.simulatePairRequest({
      code: "123456",
      browser: "chrome",
      label: "Work",
      extension: "abc",
    });
    expect(notices()).toContainEqual({
      type: "browser_pair_requested",
      id: request,
      browser: "chrome",
      code: "123456",
    });
    expect((await backend.getState()).browser.pairing.map((p) => p.code)).toEqual(["123456"]);
    expect(
      await errorCode(backend.dispatch({ command: "browser_pair_approve", id: request })),
    ).toBe("biometric_unavailable");
    await backend.dispatch({
      command: "browser_pair_approve",
      id: request,
      password: "correct horse battery",
    });
    let browser = (await backend.getState()).browser;
    expect(browser.pairing).toEqual([]);
    expect(browser.paired.map((p) => [p.browser, p.label, p.connected])).toEqual([
      ["chrome", "Work", false],
    ]);
    expect(browser.activity[0]?.event).toBe("paired");
    // Denied, lapsed, removed.
    const denied = backend.simulatePairRequest({
      code: "654321",
      browser: "edge",
      label: "",
      extension: "def",
    });
    await backend.dispatch({ command: "browser_pair_deny", id: denied });
    expect(await errorCode(backend.dispatch({ command: "browser_pair_deny", id: denied }))).toBe(
      "browser_pair_expired",
    );
    const lapsed = backend.simulatePairRequest({
      code: "111111",
      browser: "edge",
      label: "",
      extension: "def",
    });
    now += 121_000;
    expect(
      await errorCode(
        backend.dispatch({
          command: "browser_pair_approve",
          id: lapsed,
          password: "correct horse battery",
        }),
      ),
    ).toBe("browser_pair_expired");
    const id = browser.paired[0]?.id ?? "";
    await backend.dispatch({ command: "browser_unpair", id });
    expect(await errorCode(backend.dispatch({ command: "browser_unpair", id }))).toBe(
      "browser_not_paired",
    );
    expect((await backend.getState()).browser.activity[0]?.event).toBe("unpaired");
    // The policy: less needs nothing, more needs proof.
    const policy = (await backend.getState()).browser.policy;
    await backend.dispatch({
      command: "browser_set_policy",
      policy: { ...policy, copy_otp: false },
    });
    expect(
      await errorCode(
        backend.dispatch({
          command: "browser_set_policy",
          policy: { ...policy, on_page_load: true },
        }),
      ),
    ).toBe("biometric_unavailable");
    await backend.dispatch({
      command: "browser_set_policy",
      policy: { ...policy, on_page_load: true },
      password: "correct horse battery",
    });
    expect((await backend.getState()).browser.policy.on_page_load).toBe(true);
    await backend.dispatch({ command: "browser_activity_clear" });
    expect((await backend.getState()).browser.activity).toEqual([]);
    browser = (await backend.getState()).browser;
    expect(browser.enabled).toBe(false);
  });

  it("lets a waiting fill go ahead, with proof where the login asks for it", async () => {
    const backend = new MockBackend({
      phase: "unlocked",
      passwords: true,
      browser: {
        paired: [
          {
            id: "p1",
            browser: "firefox",
            label: "",
            extension: "lockra@firlab.app",
            paired_at_ms: 1,
            last_seen_ms: null,
            connected: true,
            page_access: true,
            version: null,
            paused: true,
          },
        ],
      },
    });
    const view = {
      item_id: "i1",
      title: "Bank",
      username: "me",
      frame_url: "https://bank.example/",
      top_url: null,
      browser: "firefox",
      label: "",
      reasons: ["protected" as const],
      action: "fill" as const,
    };
    const proof = backend.simulateConfirm({ ...view, needs_proof: true });
    expect(await errorCode(backend.dispatch({ command: "browser_fill_confirm", id: proof }))).toBe(
      "biometric_unavailable",
    );
    await backend.dispatch({
      command: "browser_fill_confirm",
      id: proof,
      password: "correct horse battery",
    });
    const click = backend.simulateConfirm({
      ...view,
      reasons: ["cross_origin"],
      needs_proof: false,
    });
    await backend.dispatch({ command: "browser_fill_confirm", id: click });
    const no = backend.simulateConfirm({ ...view, needs_proof: false });
    await backend.dispatch({ command: "browser_fill_deny", id: no });
    expect(await errorCode(backend.dispatch({ command: "browser_fill_deny", id: no }))).toBe(
      "browser_pair_expired",
    );
    expect(await errorCode(backend.dispatch({ command: "browser_fill_confirm", id: no }))).toBe(
      "browser_pair_expired",
    );
    expect((await backend.getState()).browser.confirms).toEqual([]);
    expect((await backend.getState()).browser.paired[0]?.paused).toBe(true);
    await backend.dispatch({ command: "browser_resume" });
    expect((await backend.getState()).browser.paired[0]?.paused).toBe(false);
    // Locking empties what waits and ends the sessions.
    backend.simulatePairRequest({ code: "123456", browser: "chrome", label: "", extension: "x" });
    await backend.dispatch({ command: "vault_lock" });
    expect((await backend.getState()).browser).toMatchObject({
      paired: [],
      pairing: [],
      available: true,
    });
    await backend.dispatch({ command: "vault_unlock", password: "correct horse battery" });
    const state = (await backend.getState()).browser;
    expect([state.pairing, state.paired.map((p) => p.connected)]).toEqual([[], [false]]);
  });

  it("brings an extension's request to link a code to a login, with the login's title", () => {
    const adfs = mockItemView("ADFS");
    const backend = new MockBackend({
      phase: "unlocked",
      passwords: true,
      items: [{ view: adfs }],
    });
    const { notices } = recorder(backend);
    backend.simulateLinkCodeRequest(adfs.id, "chrome");
    expect(notices()).toEqual([
      { type: "browser_link_code_requested", item_id: adfs.id, browser: "chrome", title: "ADFS" },
    ]);
    expect(() => backend.simulateLinkCodeRequest("gone", "chrome")).toThrow("item_not_found");
  });

  it("offers no browser extension on the phone or without password management", async () => {
    const phone = new MockBackend({ phase: "unlocked", passwords: true, platform: "android" });
    expect((await phone.getState()).browser.available).toBe(false);
    expect(await errorCode(phone.dispatch({ command: "browser_resume" }))).toBe(
      "browser_unavailable",
    );
    const off = new MockBackend({ phase: "unlocked" });
    expect(await errorCode(off.dispatch({ command: "browser_activity_clear" }))).toBe(
      "feature_unavailable",
    );
  });

  it("opens the quick search's palette only while unlocked, and keeps its shortcut as the core does", async () => {
    const backend = new MockBackend({ phase: "unlocked", passwords: true });
    const { notices } = recorder(backend);
    const palettes = () => notices().filter((n) => n.type === "open_palette");
    expect((await backend.getState()).quick_access).toEqual({ shortcut: null, error: null });
    // As the shell asks for it: its shortcut, the tray, `lockra-desktop --palette`.
    expect(backend.summonQuickAccess()).toBe(true);
    expect(palettes()).toEqual([{ type: "open_palette" }]);
    const choose = async (quick_access_shortcut: string | null) => {
      const { settings } = await backend.getState();
      await backend.dispatch({
        command: "settings_set",
        settings: { ...settings, quick_access_shortcut },
      });
    };
    await choose("Control+Alt+KeyK");
    expect((await backend.getState()).quick_access).toEqual({
      shortcut: "Control+Alt+KeyK",
      error: null,
    });
    // The shell could not take it; another choice starts clean, and none clears it.
    backend.reportShortcut(false);
    expect((await backend.getState()).quick_access.error).toBe("shortcut_unavailable");
    backend.reportShortcut(true);
    expect((await backend.getState()).quick_access.error).toBeNull();
    backend.reportShortcut(false);
    await choose("Control+Alt+KeyL");
    expect((await backend.getState()).quick_access).toEqual({
      shortcut: "Control+Alt+KeyL",
      error: null,
    });
    await choose(null);
    backend.reportShortcut(false);
    expect((await backend.getState()).quick_access).toEqual({ shortcut: null, error: null });
    // Locked: the window shows the unlock screen, and no palette opens.
    await backend.dispatch({ command: "vault_lock" });
    expect(backend.summonQuickAccess()).toBe(false);
    expect(palettes()).toHaveLength(1);
    // The palette closes: the shell hears whether something was copied from it.
    await backend.quickAccessDone(true);
    await backend.quickAccessDone(false);
    expect(backend.quickAccessEnds).toEqual([true, false]);
  });

  it("offers no quick search on the phone or without password management", async () => {
    for (const backend of [
      new MockBackend({ phase: "unlocked", settings: { quick_access_shortcut: "Alt+KeyK" } }),
      new MockBackend({ phase: "unlocked", passwords: true, platform: "android" }),
    ]) {
      const { settings } = await backend.getState();
      await backend.dispatch({
        command: "settings_set",
        settings: { ...settings, quick_access_shortcut: "Alt+KeyK" },
      });
      expect((await backend.getState()).settings.quick_access_shortcut).toBe("Alt+KeyK");
      expect((await backend.getState()).quick_access).toEqual({ shortcut: null, error: null });
      expect(backend.summonQuickAccess()).toBe(false);
      backend.reportShortcut(false);
      expect((await backend.getState()).quick_access.error).toBeNull();
    }
    const chosen = new MockBackend({
      passwords: true,
      settings: { quick_access_shortcut: "Alt+KeyK" },
    });
    expect((await chosen.getState()).quick_access.shortcut).toBe("Alt+KeyK");
  });

  it("stand-in codes are stable and padded", () => {
    expect(fakeCode("A", 1, 6)).toBe(fakeCode("A", 1, 6));
    expect(fakeCode("A", 1, 8)).toMatch(/^\d{8}$/);
    expect(fakeCode("A", 1, 6)).not.toBe(fakeCode("A", 2, 6));
  });
});

describe("the browser preview's sample", () => {
  it("is paired with Chrome and Edge, with some activity, once unlocked", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      passwords: true,
      browser: sampleBrowser(0),
    });
    const { browser } = await backend.getState();
    expect(browser.paired.map((p) => [p.browser, p.connected])).toEqual([
      ["chrome", true],
      ["edge", false],
    ]);
    expect(browser.activity.map((a) => a.event)).toEqual([
      "filled",
      "otp_filled",
      "refused",
      "saved",
      "paused",
    ]);
    expect(browser.activity[2]?.refusal).toBe("never_fill");
    await backend.dispatch({ command: "browser_activity_clear" });
  });
});
