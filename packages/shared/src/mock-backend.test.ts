import { isLockraError } from "./backend";
import {
  MOCK_INVITE_CODE,
  MOCK_PAIR_CODE,
  MOCK_PASSWORD,
  MOCK_STORAGE_SECRET,
  MOCK_SYNC_KEY,
  MockBackend,
  fakeCode,
  mockEntry,
  mockSyncSpace,
  sampleEntries,
} from "./mock-backend";
import type {
  CodesFrame,
  JoinSource,
  LanView,
  Notice,
  StorageConfig,
  SyncSpaceView,
  UiEvent,
} from "./schema";

async function errorCode(promise: Promise<unknown>): Promise<string | undefined> {
  const error = await promise.then(
    () => undefined,
    (e: unknown) => e,
  );
  return isLockraError(error) ? error.code : undefined;
}

async function hubOf(backend: MockBackend): Promise<Extract<LanView, { role: "hub" }>> {
  const lan = (await backend.getState()).sync.space?.lan;
  if (lan?.role !== "hub") throw new Error("not a hub");
  return lan;
}

async function spaceOf(backend: MockBackend): Promise<SyncSpaceView> {
  const space = (await backend.getState()).sync.space;
  if (space === null) throw new Error("no space");
  return space;
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
    expect(await create(s3(MOCK_STORAGE_SECRET, "http://127.0.0.1:9000"))).toEqual({
      sync_key: MOCK_SYNC_KEY,
    });
    let space = (await backend.getState()).sync.space;
    expect(space?.status.state).toBe("synced");
    expect(space?.device_name).toBe("Work laptop");
    expect(JSON.stringify(space)).not.toContain(MOCK_STORAGE_SECRET);
    expect(await errorCode(create(s3(MOCK_STORAGE_SECRET)))).toBe("sync_already_on");

    const invite = await backend.dispatch({ command: "sync_invite", password: MOCK_PASSWORD });
    expect(invite.invite).toMatch(/^lockra-invite:1:/);
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
    await backend.dispatch({
      command: "sync_set_storage",
      storage: webdav,
      password: MOCK_PASSWORD,
    });
    expect((await backend.getState()).sync.space?.storage).toEqual({
      kind: "webdav",
      url: "https://dav.example.com/dav/",
      prefix: "lockra",
      username: "me",
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
    // This vault's own master password is checked; the space's devices may use another one.
    expect(await errorCode(join(unlocked, invite, "a wrong password"))).toBe("wrong_password");
    expect(await errorCode(join(unlocked, invite, MOCK_PASSWORD, "a wrong password"))).toBe(
      "sync_wrong_credentials",
    );
    await join(unlocked, invite, MOCK_PASSWORD, MOCK_PASSWORD);
    expect((await unlocked.getState()).entries.map((e) => e.issuer)).toEqual(["Bank"]);
    expect(await errorCode(join(unlocked, { type: "invite", text: "lockra-invite:1:abc" }))).toBe(
      "sync_already_on",
    );
    await unlocked.dispatch({ command: "vault_lock" });
    expect(await errorCode(join(unlocked, { type: "invite", text: "lockra-invite:1:abc" }))).toBe(
      "locked",
    );
  });

  it("serves a LAN hub: offers, requests, paired devices and a storage beside it", async () => {
    const plain = new MockBackend({ entries: sampleEntries() });
    expect(
      await errorCode(
        plain.dispatch({ command: "sync_lan_enable", password: MOCK_PASSWORD, device_name: "" }),
      ),
    ).toBe("sync_lan_unavailable");

    const at = Date.UTC(2026, 9, 5);
    const backend = new MockBackend({ entries: sampleEntries(), lan: true, now: () => at });
    const enable = (password?: string) =>
      backend.dispatch({ command: "sync_lan_enable", password, device_name: " Desk " });
    // A new space on the LAN alone is sealed under the master password: no biometric check.
    expect(await errorCode(enable())).toBe("wrong_password");
    expect(await errorCode(enable("a wrong password"))).toBe("wrong_password");
    await enable(MOCK_PASSWORD);
    let space = await spaceOf(backend);
    expect(space.storage).toBeNull();
    expect(space.key_saved).toBe(false);
    expect(space.transports.map((t) => [t.kind, t.status.state])).toEqual([["lan", "synced"]]);
    expect(await hubOf(backend)).toMatchObject({ serving: true, peers: [], request: null });
    expect(await errorCode(enable(MOCK_PASSWORD))).toBe("sync_already_on");

    const offer = await backend.dispatch({ command: "sync_lan_offer", password: MOCK_PASSWORD });
    expect(offer.text).toMatch(/^lockra-pair:1:/);
    expect(offer.expires_at_ms).toBe(at + 120_000);
    expect((await hubOf(backend)).offer_until_ms).toBe(offer.expires_at_ms);

    const answer = (approve: boolean) => backend.dispatch({ command: "sync_lan_answer", approve });
    expect(await errorCode(answer(true))).toBe("sync_pairing_expired");
    backend.lanAsk("Pixel 8", "android");
    expect((await hubOf(backend)).request).toEqual({
      name: "Pixel 8",
      platform: "android",
      code: MOCK_PAIR_CODE,
    });
    await answer(false);
    expect((await hubOf(backend)).request).toBeNull();
    backend.lanAsk("Pixel 8", "android");
    await answer(true);
    let hub = await hubOf(backend);
    expect(hub.peers.map((p) => [p.name, p.platform])).toEqual([["Pixel 8", "android"]]);
    expect(hub).toMatchObject({ request: null, offer_until_ms: null });
    expect((await spaceOf(backend)).devices.map((d) => d.name)).toEqual(["Desk", "Pixel 8"]);

    // A storage of the user's own beside the LAN: another device's invitation, with its code.
    const add = (code: string) =>
      backend.dispatch({
        command: "sync_add_storage",
        source: { type: "invite", text: "lockra-invite:2:abc", code },
        password: MOCK_PASSWORD,
      });
    expect(await errorCode(add("WRONG"))).toBe("sync_invite_code_wrong");
    await add(MOCK_INVITE_CODE);
    space = await spaceOf(backend);
    expect(space.storage?.kind).toBe("webdav");
    expect(space.transports.map((t) => t.kind)).toEqual(["lan", "cloud"]);
    expect(await errorCode(add(MOCK_INVITE_CODE))).toBe("sync_already_on");
    await backend.dispatch({ command: "sync_remove_storage" });
    space = await spaceOf(backend);
    expect(space.storage).toBeNull();
    expect(space.transports.map((t) => t.kind)).toEqual(["lan"]);
    expect(await errorCode(backend.dispatch({ command: "sync_remove_storage" }))).toBe(
      "sync_no_storage",
    );

    const remove = (peer_id: string) =>
      backend.dispatch({ command: "sync_lan_remove_peer", peer_id });
    await remove(hub.peers[0]?.peer_id ?? "");
    expect((await hubOf(backend)).peers).toEqual([]);
    expect((await spaceOf(backend)).devices.map((d) => d.name)).toEqual(["Desk"]);
    expect(await errorCode(remove("00000000-0000-4000-8000-000000000000"))).toBe("internal");

    // A full hub turns the next device away.
    for (let n = 0; n < 32; n++) {
      backend.lanAsk(`Phone ${n}`, "android");
      await answer(true);
    }
    backend.lanAsk("One too many", "android");
    expect(await errorCode(answer(true))).toBe("sync_lan_full");
    hub = await hubOf(backend);
    expect(hub.peers).toHaveLength(32);
    expect(hub.request).toBeNull();

    // A space on the LAN alone goes with it.
    await backend.dispatch({ command: "sync_lan_disable" });
    expect((await backend.getState()).sync.space).toBeNull();
    expect(await errorCode(backend.dispatch({ command: "sync_lan_disable" }))).toBe("sync_off");
    expect(await errorCode(backend.dispatch({ command: "sync_lan_offer" }))).toBe("sync_off");
  });

  it("puts the LAN beside a space on storage, and takes it off again", async () => {
    const backend = new MockBackend({ entries: sampleEntries(), sync: mockSyncSpace(), lan: true });
    // An existing space needs the user here: without biometrics, the password.
    expect(
      await errorCode(backend.dispatch({ command: "sync_lan_enable", device_name: "Desk" })),
    ).toBe("biometric_unavailable");
    await backend.dispatch({
      command: "sync_lan_enable",
      password: MOCK_PASSWORD,
      device_name: "",
    });
    let space = await spaceOf(backend);
    expect(space.transports.map((t) => t.kind)).toEqual(["lan", "cloud"]);
    expect(space.storage).not.toBeNull();
    await backend.dispatch({ command: "sync_lan_disable" });
    space = await spaceOf(backend);
    expect(space.lan).toBeNull();
    expect(space.transports.map((t) => t.kind)).toEqual(["cloud"]);
    await backend.dispatch({ command: "sync_remove_storage" });
    expect((await backend.getState()).sync.space).toBeNull();
  });

  it("pairs with a LAN hub, making a vault where there was none", async () => {
    const join = (backend: MockBackend, password = MOCK_PASSWORD, text = "lockra-pair:1:abc") =>
      backend.dispatch({ command: "sync_lan_join", text, password, device_name: "Phone" });
    const fresh = new MockBackend({ lan: true });
    expect(await errorCode(join(fresh, MOCK_PASSWORD, "lockra-invite:1:abc"))).toBe(
      "sync_pairing_invalid",
    );
    expect(await errorCode(join(fresh, "short"))).toBe("password_too_short");

    // The code shows while the user at the hub decides; a refusal leaves nothing behind.
    const refused = join(fresh);
    expect((await fresh.getState()).sync.joining).toEqual({
      hub_name: "Desktop",
      code: MOCK_PAIR_CODE,
    });
    fresh.lanWelcome(false);
    expect(await errorCode(refused)).toBe("sync_pairing_refused");
    let state = await fresh.getState();
    expect(state.sync.joining).toBeNull();
    expect(state.phase).toBe("no_vault");

    const welcomed = join(fresh);
    fresh.lanWelcome(true);
    await welcomed;
    state = await fresh.getState();
    expect(state.phase).toBe("unlocked");
    expect(state.entries.length).toBeGreaterThan(0);
    expect(state.sync.space?.lan).toEqual({ role: "client", hub_name: "Desktop" });
    expect(state.sync.space?.storage).toBeNull();
    expect(state.sync.space?.devices.map((d) => [d.name, d.this_device])).toEqual([
      ["Phone", true],
      ["Desktop", false],
    ]);
    expect(await errorCode(join(fresh))).toBe("sync_already_on");

    // A device of the space gets the LAN beside its storage; this vault's password is checked.
    const member = new MockBackend({ entries: sampleEntries(), sync: mockSyncSpace(), lan: true });
    expect(await errorCode(join(member, "a wrong password"))).toBe("wrong_password");
    const beside = join(member);
    member.lanWelcome(true);
    await beside;
    expect((await spaceOf(member)).transports.map((t) => t.kind)).toEqual(["lan", "cloud"]);
    await member.dispatch({ command: "vault_lock" });
    expect(await errorCode(join(member))).toBe("locked");
    expect(await errorCode(join(new MockBackend(), MOCK_PASSWORD))).toBe("sync_lan_unavailable");
  });

  it("pairs with the hub whose pairing code the camera reads, or says why it could not", async () => {
    const texts = { prompt: "Point at the code", cancel: "Cancel" };
    const pair = { password: MOCK_PASSWORD, deviceName: "Phone" };
    expect(await new MockBackend({ lan: true }).scanPair(texts, pair)).toBe(false);
    const denied = new MockBackend({ lan: true, scan: { error: "camera_denied" } });
    expect(await errorCode(denied.scanPair(texts, pair))).toBe("camera_denied");
    const invite = new MockBackend({ lan: true, scan: "lockra-invite:1:abc" });
    expect(await errorCode(invite.scanPair(texts, pair))).toBe("sync_pairing_invalid");
    const fresh = new MockBackend({ lan: true, scan: "lockra-pair:1:abc" });
    const paired = fresh.scanPair(texts, pair);
    expect((await fresh.getState()).sync.joining?.code).toBe(MOCK_PAIR_CODE);
    fresh.lanWelcome(true);
    expect(await paired).toBe(true);
    expect((await fresh.getState()).sync.space?.lan).toEqual({
      role: "client",
      hub_name: "Desktop",
    });
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

  it("stand-in codes are stable and padded", () => {
    expect(fakeCode("A", 1, 6)).toBe(fakeCode("A", 1, 6));
    expect(fakeCode("A", 1, 8)).toMatch(/^\d{8}$/);
    expect(fakeCode("A", 1, 6)).not.toBe(fakeCode("A", 2, 6));
  });
});
