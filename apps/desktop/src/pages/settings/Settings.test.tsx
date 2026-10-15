import { MOCK_PASSWORD, MockBackend, sampleEntries } from "@lockra/shared/mock";
import { act, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../../test/render";

async function openSettings(user: ReturnType<typeof renderApp>["user"], section?: string) {
  await user.keyboard("{Control>},{/Control}");
  const dialog = await screen.findByRole("dialog", { name: "设置" });
  if (section) await user.click(within(dialog).getByRole("tab", { name: section }));
  return within(dialog);
}

describe("SettingsDialog", () => {
  it("moves between groups with the arrow keys and closes on Esc and the scrim", async () => {
    const { user } = renderApp();
    await ready();
    const dialog = await openSettings(user);
    expect(dialog.getByRole("tab", { name: "通用" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(dialog.getByRole("tab", { name: "外观" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(dialog.getByRole("tab", { name: "关于" })).toHaveAttribute("aria-selected", "true");
    expect(dialog.getByTestId("settings-version")).toHaveTextContent("Lockra 0.1.0");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await openSettings(user);
    await user.click(screen.getByTestId("settings-scrim"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await openSettings(user);
    await user.click(screen.getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("switches the language and the code order", async () => {
    const { user, backend } = renderApp();
    await ready();
    const dialog = await openSettings(user);
    await user.click(dialog.getByRole("radio", { name: "最近使用" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { sort: "recent" },
    });
    await user.click(dialog.getByRole("radio", { name: "English" }));
    expect(await screen.findByRole("dialog", { name: "Settings" })).toBeInTheDocument();
    expect(document.documentElement.lang).toBe("en-US");
  });

  it("pins a group at the top of the codes, and the page turns to it", async () => {
    const { user, backend } = renderApp();
    await ready();
    const dialog = await openSettings(user);
    const pin = dialog.getByRole("combobox", { name: "置顶分组" });
    expect(pin).toHaveDisplayValue("不置顶");
    expect(dialog.getByTestId("settings-pinned-group")).toHaveTextContent("打开验证码页时默认显示");
    await user.selectOptions(pin, "收藏");
    expect(backend.calls.at(-1)).toEqual({
      command: "view_pin_group",
      pinned: { type: "favorites" },
    });
    expect(pin).toHaveDisplayValue("收藏");
    await user.keyboard("{Escape}");
    expect(screen.getByRole("radio", { name: "收藏 1" })).toHaveAttribute("aria-checked", "true");
    await openSettings(user);
    await user.selectOptions(screen.getByRole("combobox", { name: "置顶分组" }), "不置顶");
    expect(backend.calls.at(-1)).toEqual({ command: "view_pin_group", pinned: null });
  });

  it("chooses the quick search's shortcut, and says what to bind where it cannot be taken", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      passwords: true,
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const dialog = await openSettings(user);
    const row = within(dialog.getByTestId("quick-access"));
    const field = row.getByRole("button", { name: "快捷搜索的快捷键" });
    expect(field).toHaveAccessibleDescription("未设置");
    await user.click(row.getByRole("button", { name: "使用 Ctrl+Shift+Space" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { quick_access_shortcut: "CommandOrControl+Shift+Space" },
    });
    expect(field).toHaveAccessibleDescription("Ctrl Shift Space");
    expect(row.queryByRole("button", { name: "使用 Ctrl+Shift+Space" })).toBeNull();
    // Recorded as it is pressed.
    await user.click(field);
    await user.keyboard("{Control>}{Alt>}[KeyK]{/Alt}{/Control}");
    expect(backend.calls.at(-1)).toMatchObject({
      settings: { quick_access_shortcut: "Control+Alt+KeyK" },
    });
    expect(dialog.getByRole("tab", { name: "通用" })).toBeInTheDocument();
    // The shell could not take it: why, and the command to bind in the desktop instead.
    act(() => backend.reportShortcut(false));
    expect(
      row.getByText(
        "无法注册这个快捷键：可能已被其他应用占用，或者桌面环境不允许应用注册全局快捷键",
      ),
    ).toBeInTheDocument();
    expect(row.getByText("lockra-desktop --palette")).toBeInTheDocument();
    expect(field).toHaveAttribute("aria-invalid", "true");
    // Turned off: nothing to register, nothing to say.
    await user.click(row.getByRole("button", { name: "清除快捷键" }));
    expect(backend.calls.at(-1)).toMatchObject({ settings: { quick_access_shortcut: null } });
    expect(row.queryByText("lockra-desktop --palette")).toBeNull();
    expect(field).toHaveAccessibleDescription("未设置");
  });

  it("offers no quick search where password management is not offered", async () => {
    const { user } = renderApp();
    await ready();
    const dialog = await openSettings(user);
    expect(dialog.queryByTestId("quick-access")).toBeNull();
    expect(dialog.queryByText("快捷搜索")).toBeNull();
  });

  it("applies the theme, accent, density, font size and motion at once", async () => {
    const { user, backend } = renderApp();
    await ready();
    const dialog = await openSettings(user, "外观");
    await user.click(dialog.getByRole("switch", { name: "跟随系统" }));
    expect(backend.calls.at(-1)).toMatchObject({
      settings: { follow_system_theme: false, theme: "glass" },
    });
    await user.click(dialog.getByRole("radio", { name: /暖纸/ }));
    expect(document.documentElement.dataset.theme).toBe("warm");
    await user.click(dialog.getByRole("radio", { name: "绿" }));
    expect(document.documentElement.dataset.accent).toBe("green");
    await user.click(dialog.getByRole("radio", { name: "紧凑" }));
    expect(document.documentElement.dataset.density).toBe("compact");
    await user.click(dialog.getByRole("button", { name: "放大字号" }));
    expect(dialog.getByTestId("font-size")).toHaveTextContent("15 px");
    await user.click(dialog.getByRole("button", { name: "缩小字号" }));
    await user.click(dialog.getByRole("button", { name: "缩小字号" }));
    expect(dialog.getByTestId("font-size")).toHaveTextContent("13 px");
    await user.click(dialog.getByRole("switch", { name: "减弱动效" }));
    expect(document.documentElement.dataset.reduceMotion).toBe("true");
    await user.click(dialog.getByRole("switch", { name: "跟随系统" }));
    expect(backend.calls.at(-1)).toMatchObject({ settings: { follow_system_theme: true } });
  });

  it("lays the themes out light above dark, and following the system picks a column", async () => {
    const { user, backend } = renderApp();
    await ready();
    const dialog = await openSettings(user, "外观");
    const grid = within(dialog.getByTestId("theme-grid"));
    const radios = grid.getAllByRole("radio").map((r) => r.getAttribute("aria-label"));
    expect(radios).toEqual([
      "明亮",
      "澄空",
      "墨白",
      "暖纸",
      "暗黑",
      "澄空 · 夜",
      "墨白 · 夜",
      "石墨",
    ]);
    expect(grid.getByText("浅色")).toBeInTheDocument();
    expect(grid.getByText("深色")).toBeInTheDocument();
    expect(dialog.getByText(/^跟随系统时，同一列的浅色与深色自动切换/)).toBeInTheDocument();
    // jsdom's system is light: following it, the light column picked (澄空, the default) is what
    // shows, linked to its dark one.
    expect(grid.getByRole("radio", { name: "澄空" })).toHaveAttribute("aria-checked", "true");
    expect(grid.getByTestId("theme-pair")).toHaveAttribute("data-column", "glass");
    await user.click(grid.getByRole("radio", { name: "明亮" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { theme: "light", follow_system_theme: true },
    });
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(dialog.getByText("跟随系统 · 现在是明亮")).toBeInTheDocument();
    expect(grid.getByTestId("theme-pair")).toHaveAttribute("data-column", "light");
    // The other scheme is a choice of its own: following stops, and it shows.
    await user.click(grid.getByRole("radio", { name: "墨白 · 夜" }));
    expect(backend.calls.at(-1)).toMatchObject({
      settings: { theme: "paper_night", follow_system_theme: false },
    });
    expect(document.documentElement.dataset.theme).toBe("paper_night");
    expect(dialog.getByRole("switch", { name: "跟随系统" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    expect(grid.queryByTestId("theme-pair")).toBeNull();
    // Following again: the column of the theme shown, in the system's scheme.
    await user.click(dialog.getByRole("switch", { name: "跟随系统" }));
    expect(backend.calls.at(-1)).toMatchObject({
      settings: { theme: "paper_night", follow_system_theme: true },
    });
    expect(document.documentElement.dataset.theme).toBe("paper");
    // 暖纸 and 石墨 do not follow the system.
    await user.click(grid.getByRole("radio", { name: "石墨" }));
    expect(backend.calls.at(-1)).toMatchObject({
      settings: { theme: "graphite", follow_system_theme: false },
    });
    expect(document.documentElement.dataset.theme).toBe("graphite");
  });

  it("chooses the code view, whatever the theme", async () => {
    const { user, backend } = renderApp();
    await ready();
    const dialog = await openSettings(user, "外观");
    const view = within(dialog.getByTestId("settings-codes-view"));
    expect(view.getByRole("radio", { name: "列表" })).toHaveAttribute("aria-checked", "true");
    await user.click(view.getByRole("radio", { name: "卡片" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { codes_view: "grid" },
    });
    expect(view.getByRole("radio", { name: "卡片" })).toHaveAttribute("aria-checked", "true");
  });

  it("sets auto-lock, clipboard clearing and hidden codes", async () => {
    const { user, backend } = renderApp();
    await ready();
    const dialog = await openSettings(user, "安全");
    await user.selectOptions(dialog.getByRole("combobox", { name: "自动锁定" }), "0");
    expect(backend.calls.at(-1)).toMatchObject({ settings: { auto_lock_minutes: 0 } });
    expect(dialog.getByRole("combobox", { name: "自动锁定" })).toHaveDisplayValue("从不");
    await user.selectOptions(dialog.getByRole("combobox", { name: "清空剪贴板" }), "60");
    expect(backend.calls.at(-1)).toMatchObject({ settings: { clipboard_clear_seconds: 60 } });
    await user.click(dialog.getByRole("switch", { name: "隐藏验证码" }));
    expect(backend.calls.at(-1)).toMatchObject({ settings: { hide_codes: true } });
  });

  it("remembers the vault on this device, and asks for the password to stop", async () => {
    const { user, backend } = renderApp();
    await ready();
    const dialog = await openSettings(user, "安全");
    const toggle = dialog.getByRole("switch", { name: "在本机记住" });
    await user.click(toggle);
    expect(backend.calls.at(-1)).toEqual({ command: "device_unlock_enable" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await user.click(toggle);
    const form = within(dialog.getByTestId("device-disable"));
    await user.click(form.getByRole("button", { name: "取消" }));
    expect(dialog.queryByTestId("device-disable")).toBeNull();
    await user.click(toggle);
    const again = within(dialog.getByTestId("device-disable"));
    await user.type(again.getByLabelText("主密码"), "wrong{Enter}");
    expect(await again.findByText("密码错误")).toBeInTheDocument();
    await user.type(again.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await dialog.findByRole("switch", { name: "在本机记住" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    expect(dialog.queryByTestId("device-disable")).toBeNull();
  });

  it("asks for Touch ID before the remembered key unlocks, and for the password to stop", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      deviceUnlock: true,
      biometric: "touch_id",
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const dialog = await openSettings(user, "安全");
    const toggle = dialog.getByRole("switch", { name: "使用 Touch ID 解锁" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(
      dialog.getByText(
        "锁定后用指纹解锁，不用输入主密码；开启时会一并开启「在本机记住」。主密码始终可以解锁。",
      ),
    ).toBeInTheDocument();
    backend.answerBiometric("biometric_cancelled");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-checked", "false");
    backend.answerBiometric(null);
    await user.click(toggle);
    expect(backend.calls.at(-1)).toEqual({
      command: "device_biometric_enable",
      reason: "开启 Touch ID 解锁",
    });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await user.click(toggle);
    const form = within(dialog.getByTestId("biometric-disable"));
    await user.type(form.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await dialog.findByRole("switch", { name: "使用 Touch ID 解锁" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    expect(backend.calls.at(-1)).toEqual({
      command: "device_biometric_disable",
      password: MOCK_PASSWORD,
    });
  });

  it("chooses whether Windows Hello or the master password comes first, once it is on", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      deviceUnlock: true,
      biometric: "windows_hello",
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const dialog = await openSettings(user, "安全");
    // Off: nothing to choose.
    expect(dialog.queryByRole("radiogroup", { name: "默认解锁方式" })).toBeNull();
    await user.click(dialog.getByRole("switch", { name: "使用 Windows Hello 解锁" }));
    const choice = within(await dialog.findByRole("radiogroup", { name: "默认解锁方式" }));
    expect(choice.getByRole("radio", { name: "Windows Hello" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(dialog.getByText(/在前台自动锁定或切回时会自动请求 Windows Hello/)).toBeInTheDocument();
    await user.click(choice.getByRole("radio", { name: "主密码" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { default_unlock: "password" },
    });
    expect(choice.getByRole("radio", { name: "主密码" })).toHaveAttribute("aria-checked", "true");
  });

  it("offers no fingerprint check where there is none", async () => {
    const { user } = renderApp({ mock: { deviceUnlock: true } });
    await ready();
    const dialog = await openSettings(user, "安全");
    expect(
      dialog.queryByRole("switch", { name: /^使用 (Touch ID|Windows Hello) 解锁$/ }),
    ).toBeNull();
  });

  it("offers Touch ID before the vault is remembered, and one check turns both on", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      biometric: "touch_id",
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const dialog = await openSettings(user, "安全");
    const remember = dialog.getByRole("switch", { name: "在本机记住" });
    const touchId = dialog.getByRole("switch", { name: "使用 Touch ID 解锁" });
    expect([remember, touchId].map((s) => s.getAttribute("aria-checked"))).toEqual([
      "false",
      "false",
    ]);
    await user.click(touchId);
    expect(backend.calls.at(-1)).toEqual({
      command: "device_biometric_enable",
      reason: "开启 Touch ID 解锁",
    });
    expect(backend.biometricReasons).toEqual(["开启 Touch ID 解锁"]);
    expect([remember, touchId].map((s) => s.getAttribute("aria-checked"))).toEqual([
      "true",
      "true",
    ]);
  });

  it("cannot turn Windows Hello on without a keychain to keep the key", async () => {
    const { user } = renderApp({ mock: { biometric: "windows_hello", keychainAvailable: false } });
    await ready();
    const dialog = await openSettings(user, "安全");
    expect(dialog.getByRole("switch", { name: "使用 Windows Hello 解锁" })).toBeDisabled();
  });

  it("cannot remember without a keychain", async () => {
    const { user } = renderApp({ mock: { keychainAvailable: false } });
    await ready();
    const dialog = await openSettings(user, "安全");
    expect(dialog.getByRole("switch", { name: "在本机记住" })).toBeDisabled();
    expect(dialog.getByText("这台电脑上没有可用的系统钥匙串")).toBeInTheDocument();
  });

  it("changes the master password", async () => {
    const { user, backend } = renderApp();
    await ready();
    const dialog = await openSettings(user, "安全");
    const section = within(dialog.getByTestId("change-password"));
    const submit = section.getByRole("button", { name: "修改主密码" });
    await user.type(section.getByLabelText("当前主密码"), "wrong password");
    await user.type(section.getByLabelText("新主密码"), "a brand new pass phrase");
    await user.type(section.getByLabelText("再输入一次"), "a brand new");
    expect(section.getByText("两次输入的密码不一致")).toBeInTheDocument();
    expect(submit).toBeDisabled();
    await user.type(section.getByLabelText("再输入一次"), " pass phrase");
    await user.click(submit);
    expect(await section.findByText("密码错误")).toBeInTheDocument();
    expect(section.getByLabelText("当前主密码")).toHaveValue("");
    await user.type(section.getByLabelText("当前主密码"), MOCK_PASSWORD);
    await user.click(submit);
    expect(await screen.findByText("主密码已修改")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({
      command: "vault_change_password",
      current: MOCK_PASSWORD,
      new: "a brand new pass phrase",
    });
    expect(section.getByLabelText("新主密码")).toHaveValue("");
  });

  it("tells the version, the data folder and the licences", async () => {
    const { user } = renderApp();
    await ready();
    const dialog = await openSettings(user, "关于");
    expect(dialog.getByTestId("about-version")).toHaveTextContent("Lockra 0.1.0");
    expect(dialog.getByTestId("about-data-dir")).toHaveTextContent(
      "/home/user/.local/share/dev.lockra.desktop",
    );
    expect(dialog.getByText("Apache-2.0")).toBeInTheDocument();
    expect(dialog.getByText(/SIL OFL 1\.1/)).toBeInTheDocument();
    expect(dialog.getByText(/Voltip/)).toBeInTheDocument();
    // The sample core cannot update itself: it says why, and nothing can be checked.
    expect(dialog.getByTestId("update")).toHaveTextContent("不是通过安装包安装的");
    expect(dialog.getByTestId("update-check")).toBeDisabled();
  });

  it("checks for an update in General and walks through it in the update dialog", async () => {
    const { user, backend } = renderApp({ mock: { updateMethod: "deb" } });
    await ready();
    const dialog = await openSettings(user, "通用");
    const section = within(dialog.getByTestId("update-section"));
    expect(section.getByTestId("update-status")).toHaveTextContent("尚未检查更新");
    await user.click(section.getByRole("button", { name: "检查更新" }));
    expect(backend.calls.at(-1)).toEqual({ command: "update_check" });
    expect(await section.findByText(/^已是最新 · 0\.1\.0 · 检查于/)).toBeInTheDocument();

    backend.setRelease({
      version: "0.2.0",
      notes:
        "## [0.2.0](https://github.com/sunerpy/lockra/compare/v0.1.1...v0.2.0) (2026-10-02)\n\n### Features\n\n* **update:** install updates ([#9](https://github.com/sunerpy/lockra/issues/9))\n",
      date: "2026-10-02T08:00:00Z",
      size: 2048,
    });
    await user.click(section.getByRole("button", { name: "检查更新" }));
    expect(await section.findByText("有新版本 0.2.0 · 当前 0.1.0")).toBeInTheDocument();
    await user.click(section.getByRole("button", { name: "查看新版本" }));
    const update = within(screen.getByRole("dialog", { name: "发现新版本 0.2.0" }));
    expect(update.getByTestId("update-current")).toHaveTextContent("当前 0.1.0");
    expect(update.getByTestId("update-published")).toHaveTextContent("发布于");
    expect(update.getByTestId("update-method")).toHaveTextContent("安装时系统会要求输入管理员密码");
    const notes = update.getByTestId("release-notes");
    expect(update.getByRole("heading", { name: "Features" })).toBeInTheDocument();
    expect(notes).toHaveTextContent("update: install updates (#9)");
    expect(notes).not.toHaveTextContent("https://");
    expect(notes).not.toHaveTextContent("2026-10-02)");
    expect(notes.querySelector("a")).toBeNull();

    await user.click(update.getByRole("button", { name: "立即更新" }));
    expect(backend.calls.at(-1)).toEqual({ command: "update_install" });
    expect(await screen.findByRole("dialog", { name: "正在安装 0.2.0" })).toBeInTheDocument();
    expect(section.getByTestId("update-status")).toHaveTextContent("正在安装 0.2.0…");
  });

  it("an update that fails says why in the dialog and retries", async () => {
    const { user, backend } = renderApp({
      mock: {
        updateMethod: "nsis",
        release: { version: "0.2.0", notes: null, date: null, size: 10 },
        updateFailure: { step: "download", code: "update_signature" },
      },
    });
    await ready();
    const dialog = await openSettings(user, "通用");
    const section = within(dialog.getByTestId("update-section"));
    await user.click(section.getByRole("button", { name: "检查更新" }));
    await user.click(await section.findByRole("button", { name: "查看新版本" }));
    const update = within(screen.getByRole("dialog", { name: "发现新版本 0.2.0" }));
    expect(update.getByText("此版本未提供更新说明。")).toBeInTheDocument();
    await user.click(update.getByRole("button", { name: "立即更新" }));
    const failed = within(await screen.findByRole("dialog", { name: "更新失败" }));
    expect(failed.getByRole("alert")).toHaveTextContent(
      "更新失败：安装包未使用 Lockra 的密钥签名，已拒绝安装",
    );
    await user.click(failed.getByRole("button", { name: "重试" }));
    expect(backend.calls.at(-1)).toEqual({ command: "update_check" });
    expect(section.getByTestId("update-status")).toHaveTextContent("有新版本 0.2.0 · 当前 0.1.0");
  });

  it("automatic updates download in the background; the restart installs", async () => {
    const { user, backend } = renderApp({
      mock: {
        updateMethod: "appimage",
        release: { version: "0.3.0", notes: null, date: null, size: 10 },
      },
    });
    await ready();
    const dialog = await openSettings(user, "通用");
    const toggle = dialog.getByRole("switch", { name: "自动更新" });
    expect(toggle).not.toBeChecked();
    expect(dialog.getByTestId("update-auto")).toHaveTextContent("启动 10 秒后检查更新并在后台下载");
    await user.click(toggle);
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { auto_update: true },
    });
    expect(dialog.getByRole("switch", { name: "自动更新" })).toBeChecked();
    const section = within(dialog.getByTestId("update-section"));
    expect(await section.findByText("0.3.0 已下载 · 重启后生效")).toBeInTheDocument();
    // The title bar says so too.
    expect(screen.getByTestId("update-badge")).toHaveTextContent("重启以更新");
    await user.click(section.getByRole("button", { name: "重启并更新" }));
    expect(backend.calls.at(-1)).toEqual({ command: "update_install" });
    expect(section.getByTestId("update-status")).toHaveTextContent("正在安装 0.3.0…");
  });

  it("About shows the same status, compact, and points to General for the switch", async () => {
    const { user } = renderApp({ mock: { updateMethod: "msi" } });
    await ready();
    const dialog = await openSettings(user, "关于");
    const row = within(dialog.getByTestId("update"));
    expect(row.getByTestId("update-status")).toHaveTextContent("尚未检查更新");
    expect(row.getByText("自动更新开关位于「通用」分组。")).toBeInTheDocument();
    await user.click(row.getByRole("button", { name: "检查更新" }));
    expect(await row.findByText(/^已是最新 · 0\.1\.0/)).toBeInTheDocument();
  });
});
