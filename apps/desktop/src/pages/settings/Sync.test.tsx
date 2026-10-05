import {
  MOCK_INVITE_CODE,
  MOCK_PASSWORD,
  MOCK_STORAGE_SECRET,
  MOCK_SYNC_KEY,
  MockBackend,
  mockSyncSpace,
  sampleEntries,
} from "@lockra/shared/mock";
import type { CommandName, CommandOf, ResultOf } from "@lockra/shared";
import { act, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../../test/render";

async function openSync(user: ReturnType<typeof renderApp>["user"]) {
  await user.keyboard("{Control>},{/Control}");
  const dialog = await screen.findByRole("dialog", { name: "设置" });
  await user.click(within(dialog).getByRole("tab", { name: "同步" }));
  return within(dialog);
}

describe("Settings › Sync", () => {
  it("sets up a space on S3 and shows the sync key once, as a secret view", async () => {
    const { user, backend } = renderApp();
    await ready();
    const pane = await openSync(user);
    await user.click(pane.getByTestId("sync-create-open"));
    const form = within(pane.getByTestId("sync-create"));
    const submit = form.getByRole("button", { name: "开始同步" });
    expect(submit).toBeDisabled();
    await user.type(form.getByLabelText("服务地址"), "https://s3.eu-central-1.amazonaws.com");
    await user.type(form.getByLabelText("区域"), "eu-central-1");
    await user.type(form.getByLabelText("存储桶"), "my-lockra");
    await user.type(form.getByLabelText("访问密钥 ID"), "AKIAEXAMPLE");
    await user.type(form.getByLabelText("访问密钥"), "wrong secret");
    expect(form.getByLabelText("这台设备的名称")).toHaveValue("Linux 电脑");
    await user.type(form.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(submit);
    expect(await form.findByText("存储服务拒绝访问，请检查访问密钥或密码")).toBeInTheDocument();
    expect(form.getByLabelText("主密码")).toHaveValue("");

    await user.clear(form.getByLabelText("访问密钥"));
    await user.type(form.getByLabelText("访问密钥"), MOCK_STORAGE_SECRET);
    await user.type(form.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(submit);
    const created = await screen.findByTestId("sync-created");
    expect(within(created).getByTestId("sync-key")).toHaveTextContent(MOCK_SYNC_KEY);
    expect(
      backend.calls.find(
        (c) =>
          c.command === "sync_create" &&
          c.storage.kind === "s3" &&
          c.storage.secret_access_key === MOCK_STORAGE_SECRET,
      ),
    ).toMatchObject({
      storage: {
        kind: "s3",
        bucket: "my-lockra",
        prefix: "lockra",
        secret_access_key: MOCK_STORAGE_SECRET,
      },
      device_name: "Linux 电脑",
    });
    await user.click(screen.getByRole("button", { name: "我已保存" }));
    expect(screen.queryByTestId("sync-created")).not.toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({ command: "secret_view_closed" });
    // "I have kept it" says so to the core: no reminder follows.
    expect(backend.calls).toContainEqual({ command: "sync_key_acknowledge" });
    expect(pane.queryByTestId("sync-key-reminder")).not.toBeInTheDocument();
    // The space: synced, this device listed, the storage shown without its secret.
    expect(pane.getByTestId("sync-status")).toHaveTextContent("已同步");
    expect(pane.getByTestId("sync-storage")).toHaveTextContent(
      "S3 兼容 · s3.eu-central-1.amazonaws.com / my-lockra / lockra",
    );
    expect(pane.getAllByTestId("sync-device")).toHaveLength(1);
    expect(document.body.textContent).not.toContain(MOCK_STORAGE_SECRET);
  });

  it("joins a space with the storage settings and the sync key", async () => {
    // This vault's master password is not the one the space's devices use.
    const { user, backend } = renderApp({ mock: { password: "this vault's password" } });
    await ready();
    const pane = await openSync(user);
    await user.click(pane.getByTestId("sync-join-open"));
    const form = within(pane.getByTestId("sync-join"));
    await user.click(form.getByRole("radio", { name: "同步密钥" }));
    await user.click(form.getByRole("radio", { name: "WebDAV" }));
    await user.type(form.getByLabelText("WebDAV 地址"), "https://dav.example.com/dav/");
    await user.type(form.getByLabelText("用户名"), "me");
    await user.type(form.getByLabelText("密码"), MOCK_STORAGE_SECRET);
    await user.type(form.getByLabelText("同步密钥"), MOCK_SYNC_KEY);
    // This vault's master password is checked first.
    await user.type(form.getByLabelText("这台设备的主密码"), "a wrong password");
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await form.findByText("密码错误")).toBeInTheDocument();
    // Only one password is asked until the space turns out to use another one.
    expect(form.queryByLabelText("同步空间的主密码")).not.toBeInTheDocument();
    await user.type(form.getByLabelText("这台设备的主密码"), "this vault's password");
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(
      await form.findByText("这台设备的主密码打不开同步空间，请再输入同步空间中任一设备的主密码"),
    ).toBeInTheDocument();
    // Asked for now, and said wrong there.
    await user.type(form.getByLabelText("这台设备的主密码"), "this vault's password");
    await user.type(form.getByLabelText("同步空间的主密码"), "a wrong password");
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await form.findByText("主密码或同步密钥不正确")).toBeInTheDocument();
    await user.type(form.getByLabelText("这台设备的主密码"), "this vault's password");
    await user.type(form.getByLabelText("同步空间的主密码"), MOCK_PASSWORD);
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await pane.findByTestId("sync-status")).toHaveTextContent("已同步");
    expect(
      backend.calls.find(
        (c) =>
          c.command === "sync_join" &&
          c.password === "this vault's password" &&
          c.space_password === MOCK_PASSWORD,
      ),
    ).toMatchObject({
      source: {
        type: "manual",
        storage: { kind: "webdav", url: "https://dav.example.com/dav/", username: "me" },
        sync_key: MOCK_SYNC_KEY,
      },
    });
    expect(pane.getAllByTestId("sync-device").map((d) => d.textContent)).toEqual([
      expect.stringContaining("此设备"),
      expect.stringContaining("Pixel 8"),
    ]);
  });

  it("runs, renames, moves the storage, removes a device, invites and turns off", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const backend = new MockBackend({
        entries: sampleEntries(),
        sync: mockSyncSpace({
          keyring_pending: true,
          unreadable: ["ffeeddccbbaa99887766554433221100"],
        }),
        settings: { locale: "zh-cn" },
      });
      const { user } = renderApp({ backend });
      await ready();
      const pane = await openSync(user);
      expect(pane.getByTestId("sync-status-row")).toHaveTextContent(
        "这台设备的新主密码将在下次同步时写入同步空间",
      );
      expect(pane.getByTestId("sync-unreadable")).toHaveTextContent("1 个同步对象无法读取");
      act(() => backend.simulateSync({ state: "failed", code: "sync_network", at_ms: Date.now() }));
      expect(pane.getByTestId("sync-status")).toHaveTextContent("同步失败：无法连接存储服务");
      await user.click(pane.getByTestId("sync-now"));
      expect(pane.getByTestId("sync-status")).toHaveTextContent("已同步");
      expect(pane.getByTestId("sync-status-row")).not.toHaveTextContent("新主密码");

      await user.click(pane.getByTestId("sync-rename"));
      const name = pane.getByRole("textbox", { name: "这台设备的名称" });
      await user.clear(name);
      await user.type(name, "Work desktop{Enter}");
      expect(pane.getByTestId("sync-device-name")).toHaveTextContent("Work desktop");

      await user.click(pane.getByTestId("sync-storage-edit"));
      const storage = within(pane.getByTestId("sync-storage-form"));
      expect(storage.getByLabelText("存储桶")).toHaveValue("my-lockra");
      expect(storage.getByLabelText("访问密钥")).toHaveValue("");
      await user.type(storage.getByLabelText("访问密钥"), MOCK_STORAGE_SECRET);
      await user.type(storage.getByLabelText("主密码"), "wrong password");
      await user.click(storage.getByRole("button", { name: "保存" }));
      expect(await storage.findByText("密码错误")).toBeInTheDocument();
      await user.type(storage.getByLabelText("主密码"), MOCK_PASSWORD);
      await user.click(storage.getByRole("button", { name: "保存" }));
      expect(pane.queryByTestId("sync-storage-form")).not.toBeInTheDocument();

      await user.click(
        within(pane.getByTestId("sync-unreadable")).getByRole("button", {
          name: "移除无法读取的对象",
        }),
      );
      expect(pane.queryByTestId("sync-unreadable")).not.toBeInTheDocument();
      await user.click(pane.getByRole("button", { name: "移除" }));
      await user.click(
        within(await screen.findByRole("dialog", { name: "移除「Pixel 8」？" })).getByRole(
          "button",
          { name: "移除" },
        ),
      );
      expect(pane.getAllByTestId("sync-device")).toHaveLength(1);

      await user.click(pane.getByTestId("sync-invite-open"));
      const prompt = await screen.findByRole("dialog", { name: "邀请其他设备" });
      await user.type(within(prompt).getByLabelText("主密码"), MOCK_PASSWORD);
      await user.click(within(prompt).getByRole("button", { name: "显示邀请码" }));
      const invite = await screen.findByTestId("sync-invite");
      // The text to send is sealed; its code is shown apart, for another channel.
      expect(within(invite).getByTestId("invite-text")).toHaveTextContent(/^lockra-invite:2:/);
      expect(within(invite).getByTestId("invite-code")).toHaveTextContent(MOCK_INVITE_CODE);
      expect(within(invite).getByTestId("sync-key")).toHaveTextContent(MOCK_SYNC_KEY);
      expect(screen.getByTestId("invite-countdown")).toHaveTextContent("120");
      // It hides itself after two minutes, and the secret view ends.
      await act(async () => {
        vi.advanceTimersByTime(121_000);
      });
      expect(screen.queryByTestId("sync-invite")).not.toBeInTheDocument();
      expect(backend.calls.at(-1)).toEqual({ command: "secret_view_closed" });

      await user.click(pane.getByTestId("sync-disable-open"));
      await user.click(
        within(await screen.findByRole("dialog", { name: "关闭这台设备的同步？" })).getByRole(
          "button",
          { name: "关闭同步" },
        ),
      );
      expect(await pane.findByTestId("sync-create-open")).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("a rolled back device is named, and the space is not shown while locked", async () => {
    const space = mockSyncSpace();
    const phone = space.devices[1]?.tag ?? "";
    const backend = new MockBackend({
      entries: sampleEntries(),
      sync: mockSyncSpace({ rolled_back: [phone, "0123456789abcdef0123456789abcdef"] }),
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openSync(user);
    expect(pane.getByTestId("sync-rolled-back")).toHaveTextContent(
      "Pixel 8, 01234567… 的同步数据比之前读到的旧",
    );
    expect((await backend.getState()).sync.space).not.toBeNull();
    await backend.dispatch({ command: "vault_lock" });
    expect((await backend.getState()).sync.space).toBeNull();
  });

  it("a secret that arrives after Settings closed ends the secret view at once", async () => {
    /** A core whose `slow` command answers only when told to. */
    class SlowBackend extends MockBackend {
      go: () => void = () => undefined;
      constructor(private readonly slow: CommandName) {
        super({
          entries: sampleEntries(),
          sync: slow === "sync_invite" ? mockSyncSpace() : null,
          settings: { locale: "zh-cn" },
        });
      }
      override dispatch<C extends CommandName>(command: CommandOf<C>): Promise<ResultOf<C>> {
        if (command.command !== this.slow) return super.dispatch(command);
        return new Promise<void>((resolve) => {
          this.go = resolve;
        }).then(() => super.dispatch(command));
      }
    }
    // The invitation.
    const inviting = new SlowBackend("sync_invite");
    const first = renderApp({ backend: inviting });
    await ready();
    let pane = await openSync(first.user);
    await first.user.click(pane.getByTestId("sync-invite-open"));
    const prompt = await screen.findByRole("dialog", { name: "邀请其他设备" });
    await first.user.type(within(prompt).getByLabelText("主密码"), MOCK_PASSWORD);
    await first.user.click(within(prompt).getByRole("button", { name: "显示邀请码" }));
    await first.user.keyboard("{Escape}");
    await first.user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await act(async () => inviting.go());
    expect(inviting.calls.map((c) => c.command).slice(-2)).toEqual([
      "sync_invite",
      "secret_view_closed",
    ]);
    first.unmount();

    // The new space's sync key.
    const creating = new SlowBackend("sync_create");
    const second = renderApp({ backend: creating });
    await ready();
    pane = await openSync(second.user);
    await second.user.click(pane.getByTestId("sync-create-open"));
    const form = within(pane.getByTestId("sync-create"));
    await second.user.type(form.getByLabelText("服务地址"), "https://s3.example.com");
    await second.user.type(form.getByLabelText("区域"), "auto");
    await second.user.type(form.getByLabelText("存储桶"), "b");
    await second.user.type(form.getByLabelText("访问密钥 ID"), "a");
    await second.user.type(form.getByLabelText("访问密钥"), MOCK_STORAGE_SECRET);
    await second.user.type(form.getByLabelText("主密码"), MOCK_PASSWORD);
    await second.user.click(form.getByRole("button", { name: "开始同步" }));
    await second.user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await act(async () => creating.go());
    expect(creating.calls.map((c) => c.command).slice(-2)).toEqual([
      "sync_create",
      "secret_view_closed",
    ]);
    expect(screen.queryByTestId("sync-created")).not.toBeInTheDocument();
  });

  it("saves the new key with the password just typed, and reminds of a key left unsaved", async () => {
    const { user, backend } = renderApp();
    await ready();
    let pane = await openSync(user);
    const create = async () => {
      await user.click(pane.getByTestId("sync-create-open"));
      const form = within(pane.getByTestId("sync-create"));
      await user.selectOptions(form.getByLabelText("服务商"), "aws");
      await user.type(form.getByLabelText("区域"), "eu-central-1");
      await user.type(form.getByLabelText("存储桶"), "b");
      await user.type(form.getByLabelText("访问密钥 ID"), "a");
      await user.type(form.getByLabelText("访问密钥"), MOCK_STORAGE_SECRET);
      await user.type(form.getByLabelText("主密码"), MOCK_PASSWORD);
      await user.click(form.getByRole("button", { name: "开始同步" }));
      return screen.findByTestId("sync-created");
    };
    await create();
    await user.click(screen.getByRole("button", { name: "保存到文件…" }));
    expect(await screen.findByTestId("sync-key-saved")).toHaveTextContent("同步密钥已保存到文件。");
    expect(backend.savedSyncKeys).toEqual([
      { fileName: "Lockra 同步密钥.txt", text: expect.stringContaining(MOCK_SYNC_KEY) as string },
    ]);
    // Saved with the master password typed for the space: no other prompt.
    expect(backend.biometricReasons).toEqual([]);
    await user.keyboard("{Escape}");
    expect(pane.queryByTestId("sync-key-reminder")).not.toBeInTheDocument();

    // Made again and closed without saving: Settings › Sync reminds of the key.
    await user.click(pane.getByTestId("sync-disable-open"));
    await user.click(
      within(await screen.findByRole("dialog", { name: "关闭这台设备的同步？" })).getByRole(
        "button",
        { name: "关闭同步" },
      ),
    );
    pane = within(await screen.findByRole("dialog", { name: "设置" }));
    await create();
    await user.keyboard("{Escape}");
    const reminder = await pane.findByTestId("sync-key-reminder");
    expect(reminder).toHaveTextContent("同步密钥还没有保存");
    await user.click(pane.getByRole("button", { name: "我已记下" }));
    expect(pane.queryByTestId("sync-key-reminder")).not.toBeInTheDocument();
  });

  it("shows the invitation after the biometric check that unlocks the vault", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      sync: mockSyncSpace(),
      biometric: "touch_id",
      biometricUnlock: true,
      deviceUnlock: true,
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openSync(user);
    await user.click(pane.getByTestId("sync-invite-open"));
    const prompt = await screen.findByRole("dialog", { name: "邀请其他设备" });
    expect(prompt).toHaveTextContent("验证身份以显示邀请码，或输入主密码。");
    await user.click(within(prompt).getByRole("button", { name: "使用 Touch ID 验证" }));
    expect(await screen.findByTestId("sync-invite")).toBeInTheDocument();
    expect(backend.biometricReasons.at(-1)).toBe("显示同步邀请码");
    expect(backend.calls.find((c) => c.command === "sync_invite")).toEqual({
      command: "sync_invite",
      reason: "显示同步邀请码",
    });
  });

  it("joins with a sealed invitation once its code is right", async () => {
    const { user, backend } = renderApp();
    await ready();
    const pane = await openSync(user);
    await user.click(pane.getByTestId("sync-join-open"));
    const form = within(pane.getByTestId("sync-join"));
    expect(form.queryByLabelText("口令")).not.toBeInTheDocument();
    await user.type(form.getByLabelText("邀请码"), "lockra-invite:2:TEtTSU5WVDI");
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    // A sealed text needs its code before it can join.
    expect(form.getByRole("button", { name: "加入" })).toBeDisabled();
    await user.type(form.getByLabelText("口令"), "AAAAA-BBBBB");
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await form.findByText("口令不正确，请核对邀请码旁显示的口令")).toBeInTheDocument();
    await user.clear(form.getByLabelText("口令"));
    await user.type(form.getByLabelText("口令"), MOCK_INVITE_CODE.toLowerCase());
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await pane.findByTestId("sync-status")).toHaveTextContent("已同步");
    expect(backend.calls.filter((c) => c.command === "sync_join").at(-1)).toMatchObject({
      source: { type: "invite", text: "lockra-invite:2:TEtTSU5WVDI", code: "7k2qm-xw4fd" },
    });
  });

  it("a computer on the LAN alone: turns on, pairs a phone once the codes match, unpairs it, stops", async () => {
    const { user, backend } = renderApp({ mock: { lan: true } });
    await ready();
    const pane = await openSync(user);
    await user.click(pane.getByTestId("sync-lan-open"));
    const form = within(pane.getByTestId("sync-lan-create"));
    expect(form.getByLabelText("这台设备的名称")).toHaveValue("Linux 电脑");
    await user.type(form.getByLabelText("主密码"), "a wrong password");
    await user.click(form.getByRole("button", { name: "开启局域网同步" }));
    expect(await form.findByText("密码错误")).toBeInTheDocument();
    await user.type(form.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(form.getByRole("button", { name: "开启局域网同步" }));
    const lan = within(await pane.findByTestId("sync-lan"));
    expect(lan.getByText("端口 47100")).toBeInTheDocument();
    expect(lan.getByText("还没有配对的设备。")).toBeInTheDocument();
    // On the LAN alone: no storage row, no invitation (it carries a storage), the key reminder.
    expect(pane.queryByTestId("sync-storage")).not.toBeInTheDocument();
    expect(pane.queryByTestId("sync-invite-open")).not.toBeInTheDocument();
    expect(pane.getByTestId("sync-add-storage")).toBeInTheDocument();
    expect(pane.getByTestId("sync-key-reminder")).toBeInTheDocument();

    // The pairing code, once the user proved to be here, as a secret view.
    await user.click(lan.getByTestId("sync-lan-pair"));
    const presence = within(await screen.findByRole("dialog", { name: "配对新设备" }));
    await user.type(presence.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(presence.getByRole("button", { name: "显示配对码" }));
    const offer = await screen.findByTestId("lan-offer");
    expect(within(offer).getByTestId("lan-offer-text")).toHaveTextContent(/^lockra-pair:1:/);
    expect(screen.getByTestId("lan-offer-countdown")).toHaveTextContent(
      /配对码将在 1[12]\d 秒后失效/,
    );
    expect(backend.calls).toContainEqual({ command: "sync_lan_offer", password: MOCK_PASSWORD });

    // A device asks: the code shows instead of the pairing code, the safe answer focused.
    act(() => backend.lanAsk("Pixel 8", "android"));
    expect(screen.queryByTestId("lan-offer")).not.toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({ command: "secret_view_closed" });
    let request = within(await screen.findByRole("dialog", { name: "「Pixel 8」请求加入" }));
    expect(request.getByTestId("lan-request-code")).toHaveTextContent("246 813");
    expect(request.getByText(/^Android · /)).toBeInTheDocument();
    expect(request.getByRole("button", { name: "拒绝" })).toHaveFocus();
    await user.click(request.getByRole("button", { name: "拒绝" }));
    expect(backend.calls).toContainEqual({ command: "sync_lan_answer", approve: false });
    expect(screen.queryByTestId("lan-request")).not.toBeInTheDocument();
    act(() => backend.lanAsk("Pixel 8", "android"));
    request = within(await screen.findByRole("dialog", { name: "「Pixel 8」请求加入" }));
    await user.click(request.getByRole("button", { name: "允许加入" }));
    expect(await lan.findByTestId("sync-lan-peer")).toHaveTextContent("Pixel 8Android");
    expect(screen.queryByTestId("lan-request")).not.toBeInTheDocument();

    await user.click(lan.getByRole("button", { name: "取消配对" }));
    const unpair = within(await screen.findByRole("dialog", { name: "取消与「Pixel 8」的配对？" }));
    await user.click(unpair.getByRole("button", { name: "取消配对" }));
    expect(await lan.findByText("还没有配对的设备。")).toBeInTheDocument();

    await user.click(lan.getByTestId("sync-lan-disable"));
    const stop = within(await screen.findByRole("dialog", { name: "停止局域网同步？" }));
    expect(
      stop.getByText("这台设备不再同步，账号保留在本机。已配对的设备需要重新配对。"),
    ).toBeInTheDocument();
    await user.click(stop.getByRole("button", { name: "停止局域网同步" }));
    expect(await pane.findByTestId("sync-create-section")).toBeInTheDocument();
  });

  it("a full hub says so when the next device is allowed", async () => {
    const { user, backend } = renderApp({ mock: { lan: true } });
    await ready();
    const pane = await openSync(user);
    await user.click(pane.getByTestId("sync-lan-open"));
    const form = within(pane.getByTestId("sync-lan-create"));
    await user.type(form.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(form.getByRole("button", { name: "开启局域网同步" }));
    await pane.findByTestId("sync-lan");
    await act(async () => {
      for (let n = 0; n < 32; n++) {
        backend.lanAsk(`Phone ${n}`, "android");
        await backend.dispatch({ command: "sync_lan_answer", approve: true });
      }
    });
    act(() => backend.lanAsk("One too many", "android"));
    const request = within(await screen.findByRole("dialog", { name: "「One too many」请求加入" }));
    await user.click(request.getByRole("button", { name: "允许加入" }));
    expect(
      await pane.findByText("这台电脑已配对 32 台设备，请先移除不用的设备"),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("lan-request")).not.toBeInTheDocument();
  });

  it("puts the LAN beside a storage, then takes the storage off and adds it back", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      sync: mockSyncSpace(),
      lan: true,
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openSync(user);
    await user.click(within(pane.getByTestId("sync-lan")).getByTestId("sync-lan-enable"));
    const presence = within(await screen.findByRole("dialog", { name: "局域网同步" }));
    await user.type(presence.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(presence.getByRole("button", { name: "开启" }));
    expect(await pane.findByText("端口 47100")).toBeInTheDocument();
    // Two storages: each one's status under the space's.
    expect(pane.getAllByTestId("sync-transport").map((row) => row.textContent)).toEqual([
      expect.stringMatching(/^局域网已同步/),
      expect.stringMatching(/^存储已同步/),
    ]);

    await user.click(pane.getByTestId("sync-storage-remove"));
    const remove = within(await screen.findByRole("dialog", { name: "移除存储？" }));
    await user.click(remove.getByRole("button", { name: "移除" }));
    expect(await pane.findByTestId("sync-add-storage")).toBeInTheDocument();
    expect(pane.queryByTestId("sync-storage")).not.toBeInTheDocument();
    expect(pane.queryByTestId("sync-invite-open")).not.toBeInTheDocument();
    expect(pane.queryAllByTestId("sync-transport")).toHaveLength(0);

    await user.click(pane.getByTestId("sync-add-storage-open"));
    const add = within(pane.getByTestId("sync-add-storage-form"));
    await user.click(add.getByRole("radio", { name: "邀请码" }));
    await user.type(add.getByLabelText("邀请码"), "lockra-invite:2:abc");
    await user.type(add.getByLabelText("口令"), "WRONG");
    await user.type(add.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(add.getByRole("button", { name: "添加" }));
    expect(await add.findByText("口令不正确，请核对邀请码旁显示的口令")).toBeInTheDocument();
    await user.clear(add.getByLabelText("口令"));
    await user.type(add.getByLabelText("口令"), MOCK_INVITE_CODE);
    await user.type(add.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(add.getByRole("button", { name: "添加" }));
    expect(await pane.findByTestId("sync-storage")).toHaveTextContent("WebDAV");
    expect(backend.calls).toContainEqual({
      command: "sync_add_storage",
      source: { type: "invite", text: "lockra-invite:2:abc", code: MOCK_INVITE_CODE },
      password: MOCK_PASSWORD,
    });
  });

  it("connects a space to another computer, showing the code to compare there", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      sync: mockSyncSpace(),
      lan: true,
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openSync(user);
    await user.click(pane.getByTestId("sync-lan-connect"));
    const form = within(pane.getByTestId("sync-lan-connect-form"));
    const connect = form.getByRole("button", { name: "连接" });
    await user.type(form.getByLabelText("配对码"), "lockra-invite:1:abc");
    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    // Not a pairing code: nothing to send.
    expect(connect).toBeDisabled();
    await user.clear(form.getByLabelText("配对码"));
    await user.type(form.getByLabelText("配对码"), "lockra-pair:1:abc");
    await user.click(connect);
    expect(await form.findByTestId("sync-lan-joining")).toHaveTextContent("等待「Desktop」确认");
    expect(form.getByTestId("sync-lan-joining-code")).toHaveTextContent("246 813");
    act(() => backend.lanWelcome(false));
    expect(await form.findByText("电脑上的用户拒绝了配对")).toBeInTheDocument();
    expect(form.queryByTestId("sync-lan-joining")).not.toBeInTheDocument();

    await user.type(form.getByLabelText("这台设备的主密码"), MOCK_PASSWORD);
    await user.click(connect);
    await form.findByTestId("sync-lan-joining");
    act(() => backend.lanWelcome(true));
    // The section turns into the client's: found again once it has.
    expect(await pane.findByText("在同一网络里时经「Desktop」同步。")).toBeInTheDocument();
    const lan = within(pane.getByTestId("sync-lan"));
    expect(backend.calls).toContainEqual({
      command: "sync_lan_join",
      text: "lockra-pair:1:abc",
      password: MOCK_PASSWORD,
      device_name: "Desktop",
    });
    await user.click(lan.getByTestId("sync-lan-disable"));
    const stop = within(await screen.findByRole("dialog", { name: "停止局域网同步？" }));
    expect(
      stop.getByText("这台设备不再经「Desktop」同步。经存储的同步不受影响。"),
    ).toBeInTheDocument();
    await user.click(stop.getByRole("button", { name: "停止局域网同步" }));
    expect(await pane.findByTestId("sync-lan-enable")).toBeInTheDocument();
  });
});
