import {
  MOCK_INVITE_CODE,
  MOCK_PASSWORD,
  MOCK_STORAGE_SECRET,
  MOCK_SYNC_KEY,
  MockBackend,
  mockSyncSpace,
  sampleEntries,
} from "@lockra/shared/mock";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

afterEach(async () => {
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
    expect(row).toHaveTextContent("经 Lockra 中继或你自己的存储，与其他设备同步");
    await user.click(row);
    await user.click(await screen.findByTestId("sync-setup-open"));
    const form = within(await screen.findByTestId("sync-create"));
    const submit = form.getByRole("button", { name: "开始同步" });
    expect(submit).toBeDisabled();
    await user.click(form.getByRole("radio", { name: "S3 兼容" }));
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

  it("joins a space on Lockra's relay from the desktop's code, with nothing else to type", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openSync(user);
    await user.click(screen.getByTestId("sync-join-open"));
    const form = within(await screen.findByTestId("sync-join"));
    // What the desktop's code holds: the relay's address and the sync key.
    backend.setScan(
      `lockra-invite:1:${btoa("mock-relay-invite|https://lockra-relay.onethinker.top\n:1")}`,
    );
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    await user.click(form.getByRole("button", { name: "扫码加入" }));
    expect(await screen.findByTestId("sync-status")).toHaveTextContent("已同步");
    expect(screen.getByText("Lockra 中继 · lockra-relay.onethinker.top")).toBeInTheDocument();
    // The phone's own invitation holds the relay's address and the sync key, no credentials.
    await user.click(screen.getByTestId("sync-invite-open"));
    const page = within(await screen.findByTestId("page-sync-invite"));
    await user.type(page.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await page.findByTestId("invite-text")).toHaveTextContent(/^lockra-invite:2:/);
    expect(
      page.getByText("邀请码包含中继地址和同步密钥，只能在你自己的设备上使用。"),
    ).toBeInTheDocument();
  });

  it("asks for this phone's own storage when the invitation holds the sync key alone", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openSync(user);
    await user.click(screen.getByTestId("sync-join-open"));
    const form = within(await screen.findByTestId("sync-join"));
    backend.setScan(`lockra-invite:1:${btoa("mock-key-only-invite:1")}`);
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    await user.click(form.getByRole("button", { name: "扫码加入" }));
    // Asked how this phone reaches the computer's folder: the drive's WebDAV first, no folder here.
    const storage = within(await form.findByTestId("sync-join-storage"));
    expect(storage.getByRole("radio", { name: "WebDAV" })).toBeChecked();
    expect(storage.queryByRole("radio", { name: "网盘文件夹" })).not.toBeInTheDocument();
    expect(form.queryByRole("alert")).not.toBeInTheDocument();
    expect(form.getByRole("button", { name: "扫码加入" })).toBeDisabled();
    await user.type(storage.getByLabelText("WebDAV 地址"), "https://dav.example.com/dav/");
    await user.type(storage.getByLabelText("用户名"), "me");
    await user.type(storage.getByLabelText("密码"), MOCK_STORAGE_SECRET);
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    await user.click(form.getByRole("button", { name: "扫码加入" }));
    expect(await screen.findByTestId("sync-status")).toHaveTextContent("已同步");
    expect(screen.getByTestId("sync-storage")).toHaveTextContent("WebDAV · dav.example.com");
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
    await user.click(form.getByRole("radio", { name: "S3 兼容" }));
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
});
