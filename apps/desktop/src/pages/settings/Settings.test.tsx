import { MOCK_PASSWORD } from "@lockra/shared/mock";
import { screen, within } from "@testing-library/react";
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

  it("applies the theme, accent, density, font size and motion at once", async () => {
    const { user, backend } = renderApp();
    await ready();
    const dialog = await openSettings(user, "外观");
    // Following the system: the tiles wait until that is off.
    expect(dialog.getByRole("radio", { name: /暖纸/ })).toBeDisabled();
    await user.click(dialog.getByRole("switch", { name: "跟随系统" }));
    expect(backend.calls.at(-1)).toMatchObject({
      settings: { follow_system_theme: false, theme: "light" },
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
  });
});
