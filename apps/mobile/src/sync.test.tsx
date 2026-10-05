import {
  MOCK_INVITE_CODE,
  MOCK_PASSWORD,
  MOCK_STORAGE_SECRET,
  MOCK_SYNC_KEY,
  MockBackend,
  mockSyncSpace,
  sampleEntries,
} from "@lockra/shared/mock";
import type { ScanPair, ScanTexts } from "@lockra/shared";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

/** A camera that reads the pairing code when the test says. */
class SlowPairCamera extends MockBackend {
  read: (() => void) | undefined;
  override async scanPair(texts: ScanTexts, pair: ScanPair): Promise<boolean> {
    await new Promise<void>((resolve) => {
      this.read = resolve;
    });
    return super.scanPair(texts, pair);
  }
}

/** The page shown or hidden, once the effects that listen for it have run. */
async function setVisibility(state: "visible" | "hidden") {
  await act(async () => {});
  Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
  act(() => {
    document.dispatchEvent(new Event("visibilitychange"));
  });
}

afterEach(async () => {
  Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
  cleanup();
  await waitFor(() => {
    if (depthOf(history.state) !== 0) throw new Error("the history still holds pages");
  });
});

type User = ReturnType<typeof renderApp>["user"];

async function openSync(user: User) {
  await user.click(screen.getByTestId("codes-settings"));
  await user.click(await screen.findByTestId("settings-sync"));
  return screen.findByTestId("page-sync");
}

/** An unlocked vault in a sync space (the sample space, this device "Desktop"). */
function inSpace() {
  return new MockBackend({
    entries: sampleEntries(),
    sync: mockSyncSpace(),
    settings: { locale: "zh-cn" },
  });
}

describe("sync on the phone", () => {
  it("sets up a space on the user's storage and shows its sync key once", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("codes-settings"));
    const row = await screen.findByTestId("settings-sync");
    expect(row).toHaveTextContent("设置同步");
    expect(row).toHaveTextContent("在你自己的存储上与其他设备同步");
    await user.click(row);
    await user.click(await screen.findByTestId("sync-setup-open"));
    const form = within(await screen.findByTestId("sync-create"));
    const submit = form.getByRole("button", { name: "开始同步" });
    expect(submit).toBeDisabled();
    await user.type(form.getByLabelText("服务地址"), "https://s3.eu-central-1.amazonaws.com");
    await user.type(form.getByLabelText("区域"), "eu-central-1");
    await user.type(form.getByLabelText("存储桶"), "my-lockra");
    await user.type(form.getByLabelText("访问密钥 ID"), "AKIAEXAMPLE");
    await user.type(form.getByLabelText("访问密钥"), "wrong secret");
    expect(form.getByLabelText("这台设备的名称")).toHaveValue("Android 手机");
    await user.type(form.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(submit);
    expect(await form.findByRole("alert")).toHaveTextContent(
      "存储服务拒绝访问，请检查访问密钥或密码",
    );
    expect(form.getByLabelText("主密码")).toHaveValue("");
    await user.clear(form.getByLabelText("访问密钥"));
    await user.type(form.getByLabelText("访问密钥"), MOCK_STORAGE_SECRET);
    await user.type(form.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(submit);
    const page = within(await screen.findByTestId("page-sync-key"));
    expect(page.getByTestId("sync-key")).toHaveTextContent(MOCK_SYNC_KEY);
    expect(page.getByTestId("sync-key-countdown")).toHaveTextContent("120");
    expect(backend.calls.find((c) => c.command === "sync_create")).toMatchObject({
      storage: { kind: "s3", bucket: "my-lockra", prefix: "lockra" },
      device_name: "Android 手机",
    });
    // Saved to a file with the password the space was just made with: no other prompt.
    await user.click(page.getByRole("button", { name: "保存到文件…" }));
    expect(await page.findByTestId("sync-key-saved")).toHaveTextContent("同步密钥已保存到文件。");
    expect(backend.savedSyncKeys).toEqual([
      { fileName: "Lockra 同步密钥.txt", text: expect.stringContaining(MOCK_SYNC_KEY) as string },
    ]);
    expect(backend.biometricReasons).toEqual([]);
    await user.click(page.getByRole("button", { name: "我已保存" }));
    // Back on the sync page, now with the space; the key went with its page.
    expect(await screen.findByTestId("sync-status")).toHaveTextContent("已同步");
    expect(screen.queryByTestId("sync-key-reminder")).not.toBeInTheDocument();
    expect(screen.queryByTestId("sync-key")).not.toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({ command: "secret_view_closed" });
    expect(screen.getByTestId("sync-storage")).toHaveTextContent(
      "S3 兼容 · s3.eu-central-1.amazonaws.com / my-lockra / lockra",
    );
    expect(screen.getAllByTestId("sync-device")).toHaveLength(1);
    expect(document.body.textContent).not.toContain(MOCK_STORAGE_SECRET);
  });

  it("joins a space from the invitation the camera reads, which never reaches the page", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openSync(user);
    await user.click(screen.getByTestId("sync-join-open"));
    const form = within(await screen.findByTestId("sync-join"));
    expect(form.getByText(/邀请其他设备/)).toBeInTheDocument();
    const scan = form.getByRole("button", { name: "扫码加入" });
    expect(scan).toBeDisabled();
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    // Left without a code: nothing happens, and the password stays.
    await user.click(scan);
    expect(screen.getByTestId("page-sync-join")).toBeInTheDocument();
    expect(form.queryByRole("alert")).not.toBeInTheDocument();
    backend.setScan("otpauth://totp/Cam:era?secret=MFRGGZDF");
    await user.click(scan);
    expect(await form.findByRole("alert")).toHaveTextContent("不是有效的 Lockra 同步邀请");
    backend.setScan("lockra-invite:1:bW9jaw");
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    await user.click(scan);
    // Joined: the sync page again, with this phone and the space's other device.
    expect(await screen.findByTestId("sync-status")).toHaveTextContent("已同步");
    const names = screen.getAllByTestId("sync-device").map((d) => d.textContent);
    expect(names).toEqual([
      expect.stringContaining("Android 手机"),
      expect.stringContaining("Pixel 8"),
    ]);
    expect(backend.calls.some((c) => c.command === "sync_join")).toBe(false);
  });

  it("joins with a pasted invitation, or the storage and the sync key", async () => {
    // This phone's master password is not the one the space's devices use.
    const { user, backend } = renderApp({ mock: { password: "this phone's password" } });
    await ready();
    await openSync(user);
    await user.click(screen.getByTestId("sync-join-open"));
    const form = within(await screen.findByTestId("sync-join"));
    await user.click(form.getByRole("radio", { name: "同步密钥" }));
    const join = form.getByRole("button", { name: "加入" });
    await user.type(form.getByLabelText("服务地址"), "https://s3.example.com");
    await user.type(form.getByLabelText("区域"), "auto");
    await user.type(form.getByLabelText("存储桶"), "vault");
    await user.type(form.getByLabelText("访问密钥 ID"), "AKID");
    await user.type(form.getByLabelText("访问密钥"), MOCK_STORAGE_SECRET);
    await user.type(form.getByLabelText("同步密钥"), "LKS1-not-a-key");
    await user.type(form.getByLabelText("这台设备的主密码"), "this phone's password");
    await user.click(join);
    expect(await form.findByRole("alert")).toHaveTextContent("同步密钥");
    await user.click(form.getByRole("radio", { name: "邀请码" }));
    await user.type(form.getByLabelText("邀请码"), "  lockra-invite:1:bW9jaw  ");
    // One password until the space turns out to use another one.
    expect(form.queryByLabelText("同步空间的主密码")).not.toBeInTheDocument();
    await user.type(form.getByLabelText("这台设备的主密码"), "this phone's password");
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await form.findByRole("alert")).toHaveTextContent("打不开同步空间");
    await user.type(form.getByLabelText("同步空间的主密码"), MOCK_PASSWORD);
    await user.type(form.getByLabelText("这台设备的主密码"), "this phone's password");
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await screen.findByTestId("sync-status")).toHaveTextContent("已同步");
    expect(
      backend.calls.filter((c) => c.command === "sync_join" && c.source.type === "invite").at(-1),
    ).toEqual({
      command: "sync_join",
      source: { type: "invite", text: "lockra-invite:1:bW9jaw" },
      password: "this phone's password",
      device_name: "Android 手机",
      space_password: MOCK_PASSWORD,
    });
  });

  it("makes a new phone's vault from the space it joins on the welcome screen", async () => {
    const backend = new MockBackend({ phase: "no_vault", settings: { locale: "zh-cn" } });
    const { user } = renderApp({ backend });
    await ready();
    const welcome = within(screen.getByTestId("welcome-join"));
    expect(welcome.getByText(/这部手机会得到同样的账号/)).toBeInTheDocument();
    await user.click(welcome.getByRole("button", { name: "加入同步…" }));
    const form = within(await screen.findByTestId("sync-join"));
    expect(form.getByText(/这部手机上还没有保险库/)).toBeInTheDocument();
    // One password: on a new phone, the space's becomes the vault's.
    expect(form.getAllByLabelText(/主密码/)).toHaveLength(1);
    await user.type(form.getByLabelText("同步空间的主密码"), MOCK_PASSWORD);
    backend.setScan("lockra-invite:1:bW9jaw");
    await user.click(form.getByRole("button", { name: "扫码加入" }));
    // The vault is the space's: the codes, with its accounts.
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect((await backend.getState()).sync.space?.device_name).toBe("Android 手机");
  });

  it("runs now, renames this phone, changes the storage and removes a device", async () => {
    const backend = inSpace();
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("codes-settings"));
    const row = await screen.findByTestId("settings-sync");
    expect(row).toHaveTextContent("已同步");
    expect(row).toHaveTextContent("S3 兼容 · s3.eu-central-1.amazonaws.com / my-lockra / lockra");
    await user.click(row);
    await user.click(await screen.findByTestId("sync-now"));
    expect(backend.calls.at(-1)).toEqual({ command: "sync_now" });
    // Renamed in a dialog.
    await user.click(screen.getByTestId("sync-rename"));
    const rename = within(await screen.findByRole("dialog", { name: "重命名这台设备" }));
    await user.clear(rename.getByLabelText("这台设备的名称"));
    await user.type(rename.getByLabelText("这台设备的名称"), "Pixel 9{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByTestId("sync-device-name")).toHaveTextContent("Pixel 9");
    // The storage: its own page, the secret typed in again.
    await user.click(screen.getByTestId("sync-storage-edit"));
    const storage = within(await screen.findByTestId("sync-storage-form"));
    expect(storage.getByLabelText("存储桶")).toHaveValue("my-lockra");
    expect(storage.getByLabelText("访问密钥")).toHaveValue("");
    await user.type(storage.getByLabelText("访问密钥"), MOCK_STORAGE_SECRET);
    await user.clear(storage.getByLabelText("存储桶"));
    await user.type(storage.getByLabelText("存储桶"), "other-bucket");
    await user.type(storage.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(storage.getByRole("button", { name: "保存" }));
    expect(await screen.findByTestId("sync-storage")).toHaveTextContent("other-bucket");
    // Removing another device asks first.
    const [, other] = screen.getAllByTestId("sync-device");
    if (!other) throw new Error("no other device");
    await user.click(within(other).getByRole("button", { name: "移除" }));
    const remove = within(await screen.findByRole("dialog", { name: "移除「Pixel 8」？" }));
    await user.click(remove.getByRole("button", { name: "移除" }));
    await waitFor(() => expect(screen.getAllByTestId("sync-device")).toHaveLength(1));
  });

  it("shows an invitation behind the master password, and hides it again", async () => {
    const backend = inSpace();
    const { user } = renderApp({ backend });
    await ready();
    await openSync(user);
    await user.click(screen.getByTestId("sync-invite-open"));
    const page = within(await screen.findByTestId("page-sync-invite"));
    await user.type(page.getByLabelText("主密码"), "wrong{Enter}");
    expect(await page.findByText("密码错误")).toBeInTheDocument();
    await user.type(page.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    // The text to send is sealed; its code is shown apart, for another channel.
    expect(await page.findByTestId("invite-text")).toHaveTextContent(/^lockra-invite:2:/);
    expect(page.getByTestId("invite-code")).toHaveTextContent(MOCK_INVITE_CODE);
    expect(page.getByTestId("sync-key")).toHaveTextContent(MOCK_SYNC_KEY);
    expect(page.getByTestId("invite-countdown")).toHaveTextContent("120");
    expect(page.getByRole("img", { name: "二维码" })).toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "完成" }));
    expect(await screen.findByTestId("page-sync")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({ command: "secret_view_closed" });
  });

  it("turns sync off on this phone, keeping its accounts", async () => {
    const backend = inSpace();
    const { user } = renderApp({ backend });
    await ready();
    await openSync(user);
    await user.click(screen.getByTestId("sync-disable-open"));
    const dialog = within(await screen.findByRole("dialog", { name: "关闭这台设备的同步？" }));
    await user.click(dialog.getByRole("button", { name: "关闭同步" }));
    expect(await screen.findByTestId("sync-setup-open")).toBeInTheDocument();
    expect((await backend.getState()).entries.length).toBeGreaterThan(0);
  });

  it("reminds of a sync key left unsaved, and saves it with the fingerprint", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      sync: mockSyncSpace({ key_saved: false }),
      biometric: "fingerprint",
      biometricUnlock: true,
      deviceUnlock: true,
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    await openSync(user);
    const reminder = await screen.findByTestId("sync-key-reminder");
    expect(reminder).toHaveTextContent("同步密钥还没有保存");
    await user.click(screen.getByRole("button", { name: "保存同步密钥…" }));
    await waitFor(() => expect(screen.queryByTestId("sync-key-reminder")).not.toBeInTheDocument());
    expect(backend.biometricReasons).toEqual(["保存同步密钥"]);
  });

  it("shows an invitation after the fingerprint that unlocks the vault", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      sync: mockSyncSpace(),
      biometric: "fingerprint",
      biometricUnlock: true,
      deviceUnlock: true,
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    await openSync(user);
    await user.click(screen.getByTestId("sync-invite-open"));
    const page = within(await screen.findByTestId("page-sync-invite"));
    await user.click(page.getByRole("button", { name: "使用指纹验证" }));
    expect(await page.findByTestId("invite-code")).toHaveTextContent(MOCK_INVITE_CODE);
    expect(backend.biometricReasons.at(-1)).toBe("显示同步邀请码");
  });

  it("joins with a pasted sealed invitation and its code", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openSync(user);
    await user.click(screen.getByTestId("sync-join-open"));
    const form = within(await screen.findByTestId("sync-join"));
    await user.click(form.getByRole("radio", { name: "邀请码" }));
    await user.type(form.getByLabelText("邀请码"), "lockra-invite:2:TEtTSU5WVDI");
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    expect(form.getByRole("button", { name: "加入" })).toBeDisabled();
    await user.type(form.getByLabelText("口令"), "AAAAA-BBBBB");
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await form.findByRole("alert")).toHaveTextContent("口令不正确");
    await user.clear(form.getByLabelText("口令"));
    await user.type(form.getByLabelText("口令"), MOCK_INVITE_CODE);
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await screen.findByTestId("sync-status")).toHaveTextContent("已同步");
    expect(backend.calls.filter((c) => c.command === "sync_join").at(-1)).toMatchObject({
      source: { type: "invite", text: "lockra-invite:2:TEtTSU5WVDI", code: MOCK_INVITE_CODE },
    });
  });

  it("pairs a new phone with a computer on the welcome screen, under a new master password", async () => {
    const backend = new MockBackend({
      phase: "no_vault",
      lan: true,
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const welcome = within(screen.getByTestId("welcome-pair"));
    await user.click(welcome.getByTestId("welcome-pair-open"));
    const form = within(await screen.findByTestId("sync-pair"));
    expect(form.getByText(/这部手机上还没有保险库，配对后/)).toBeInTheDocument();
    const scan = form.getByRole("button", { name: "扫码配对" });
    await user.type(form.getByLabelText("主密码"), "a new master password");
    await user.type(form.getByLabelText("再输入一次"), "a new master passwor");
    expect(form.getByText("两次输入的密码不一致")).toBeInTheDocument();
    expect(scan).toBeDisabled();
    await user.type(form.getByLabelText("再输入一次"), "d");
    backend.setScan("lockra-pair:1:bW9jaw");
    await user.click(scan);
    // The code to compare on the computer, while its user decides.
    expect(await form.findByTestId("sync-lan-joining")).toHaveTextContent("等待「Desktop」确认");
    expect(form.getByTestId("sync-lan-joining-code")).toHaveTextContent("246 813");
    act(() => backend.lanWelcome(true));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    // The pairing code went from the camera to the core, not through the page.
    expect(backend.calls.some((c) => c.command === "sync_lan_join")).toBe(false);
    expect((await backend.getState()).sync.space?.lan).toEqual({
      role: "client",
      hub_name: "Desktop",
    });
  });

  it("pairs a phone's vault with a computer from a pasted code, shows the computer, stops", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      lan: true,
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    await openSync(user);
    await user.click(screen.getByTestId("sync-pair-open"));
    const form = within(await screen.findByTestId("sync-pair"));
    await user.click(form.getByRole("radio", { name: "粘贴配对码" }));
    const pair = form.getByRole("button", { name: "配对" });
    await user.type(form.getByLabelText("配对码"), "lockra-invite:1:bW9jaw");
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    // Not a pairing code: nothing to send.
    expect(pair).toBeDisabled();
    await user.clear(form.getByLabelText("配对码"));
    await user.type(form.getByLabelText("配对码"), "lockra-pair:1:bW9jaw");
    await user.click(pair);
    await form.findByTestId("sync-lan-joining");
    act(() => backend.lanWelcome(false));
    expect(await form.findByRole("alert")).toHaveTextContent("电脑上的用户拒绝了配对");
    expect(form.queryByTestId("sync-lan-joining")).not.toBeInTheDocument();
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    await user.click(pair);
    await form.findByTestId("sync-lan-joining");
    act(() => backend.lanWelcome(true));
    // Back on the sync page: the computer it syncs through, and no invitation (no storage).
    expect(await screen.findByTestId("sync-lan-client")).toHaveTextContent(
      "在同一网络里时经「Desktop」同步。",
    );
    expect(screen.queryByTestId("sync-invite-open")).not.toBeInTheDocument();
    expect(screen.queryByTestId("sync-pair-open")).not.toBeInTheDocument();
    expect(backend.calls).toContainEqual({
      command: "sync_lan_join",
      text: "lockra-pair:1:bW9jaw",
      password: MOCK_PASSWORD,
      device_name: "Android 手机",
    });
    await user.click(screen.getByTestId("sync-lan-disable"));
    const stop = within(await screen.findByRole("dialog", { name: "停止局域网同步？" }));
    expect(
      stop.getByText("这台设备不再同步，账号保留在本机。之后可以重新配对。"),
    ).toBeInTheDocument();
    await user.click(stop.getByRole("button", { name: "停止局域网同步" }));
    expect(await screen.findByTestId("sync-setup-open")).toBeInTheDocument();
  });

  it("a space on storage connects to a computer, each storage then saying how it is doing", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      sync: mockSyncSpace(),
      lan: true,
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    await openSync(user);
    expect(screen.queryAllByTestId("sync-transport")).toHaveLength(0);
    await user.click(screen.getByTestId("sync-pair-open"));
    const form = within(await screen.findByTestId("sync-pair"));
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    backend.setScan("lockra-pair:1:bW9jaw");
    await user.click(form.getByRole("button", { name: "扫码配对" }));
    await form.findByTestId("sync-lan-joining");
    act(() => backend.lanWelcome(true));
    expect(await screen.findByTestId("sync-lan-client")).toBeInTheDocument();
    expect(screen.getAllByTestId("sync-transport").map((row) => row.textContent)).toEqual([
      expect.stringMatching(/^局域网已同步/),
      expect.stringMatching(/^存储已同步/),
    ]);
    await user.click(screen.getByTestId("sync-lan-disable"));
    const stop = within(await screen.findByRole("dialog", { name: "停止局域网同步？" }));
    expect(
      stop.getByText("这台设备不再经「Desktop」同步。经存储的同步不受影响。"),
    ).toBeInTheDocument();
  });

  it("leaving while the camera reads keeps the vault open, and locks it once the computer is asked", async () => {
    const backend = new SlowPairCamera({
      entries: sampleEntries(),
      lan: true,
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    await openSync(user);
    await user.click(screen.getByTestId("sync-pair-open"));
    const form = within(await screen.findByTestId("sync-pair"));
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    backend.setScan("lockra-pair:1:bW9jaw");
    await user.click(form.getByRole("button", { name: "扫码配对" }));
    // The camera is the phone's own screen over the app: not leaving.
    await setVisibility("hidden");
    await setVisibility("visible");
    expect(backend.calls.some((c) => c.command === "vault_lock")).toBe(false);
    act(() => backend.read?.());
    await form.findByTestId("sync-lan-joining");
    await setVisibility("hidden");
    expect(backend.calls.some((c) => c.command === "vault_lock")).toBe(true);
    act(() => backend.lanWelcome(false));
  });
});
