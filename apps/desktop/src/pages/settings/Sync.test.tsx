import {
  MOCK_PASSWORD,
  MOCK_STORAGE_SECRET,
  MOCK_SYNC_KEY,
  MockBackend,
  mockSyncSpace,
  sampleEntries,
} from "@lockra/shared/mock";
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
    // The space: synced, this device listed, the storage shown without its secret.
    expect(pane.getByTestId("sync-status")).toHaveTextContent("已同步");
    expect(pane.getByTestId("sync-storage")).toHaveTextContent(
      "S3 兼容 · s3.eu-central-1.amazonaws.com / my-lockra / lockra",
    );
    expect(pane.getAllByTestId("sync-device")).toHaveLength(1);
    expect(document.body.textContent).not.toContain(MOCK_STORAGE_SECRET);
  });

  it("joins a space with the storage settings and the sync key", async () => {
    const { user, backend } = renderApp();
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
    await user.type(form.getByLabelText("同步空间的主密码"), "a wrong password");
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await form.findByText("主密码或同步密钥不正确")).toBeInTheDocument();
    await user.type(form.getByLabelText("同步空间的主密码"), MOCK_PASSWORD);
    await user.click(form.getByRole("button", { name: "加入" }));
    expect(await pane.findByTestId("sync-status")).toHaveTextContent("已同步");
    expect(
      backend.calls.find((c) => c.command === "sync_join" && c.password === MOCK_PASSWORD),
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
        "新的主密码将在下次同步时写入同步空间",
      );
      expect(pane.getByTestId("sync-unreadable")).toHaveTextContent("1 个同步对象无法读取");
      act(() => backend.simulateSync({ state: "failed", code: "sync_network", at_ms: Date.now() }));
      expect(pane.getByTestId("sync-status")).toHaveTextContent("同步失败：无法连接存储服务");
      await user.click(pane.getByTestId("sync-now"));
      expect(pane.getByTestId("sync-status")).toHaveTextContent("已同步");
      expect(pane.getByTestId("sync-status-row")).not.toHaveTextContent("新的主密码");

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
      expect(within(invite).getByTestId("invite-text")).toHaveTextContent(/^lockra-invite:1:/);
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
});
