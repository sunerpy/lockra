import { MOCK_PASSWORD, MockBackend, sampleEntries } from "@lockra/shared/mock";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

afterEach(async () => {
  cleanup();
  await waitFor(() => {
    if (depthOf(history.state) !== 0) throw new Error("the history still holds pages");
  });
});

function settingsSent(backend: MockBackend) {
  const calls = backend.calls.filter((c) => c.command === "settings_set");
  const last = calls.at(-1);
  if (last?.command !== "settings_set") throw new Error("no settings were sent");
  return last.settings;
}

describe("the phone's settings", () => {
  it("apply every choice at once", async () => {
    const backend = new MockBackend({ entries: sampleEntries(), settings: { locale: "zh-cn" } });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("dock-settings"));
    expect(await screen.findByTestId("page-settings")).toBeInTheDocument();
    expect(screen.getByTestId("about-version")).toHaveTextContent("Lockra 0.1.0");

    await user.click(within(screen.getByTestId("settings-groups")).getByRole("switch"));
    expect(settingsSent(backend).group_codes).toBe(false);
    await user.click(within(screen.getByTestId("settings-hide-codes")).getByRole("switch"));
    expect(settingsSent(backend).hide_codes).toBe(true);
    await user.selectOptions(screen.getByRole("combobox", { name: "自动锁定" }), "从不");
    expect(settingsSent(backend).auto_lock_minutes).toBe(0);
    // A theme of its own, then the system's again.
    const themes = screen.getByRole("radiogroup", { name: "主题" });
    const following = within(screen.getByTestId("settings-follow-system")).getByRole("switch");
    if (following.getAttribute("aria-checked") === "true") await user.click(following);
    await user.click(within(themes).getByRole("radio", { name: /暖纸/ }));
    expect(settingsSent(backend)).toMatchObject({ theme: "warm", follow_system_theme: false });
    expect(within(themes).getByRole("radio", { name: /暖纸/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await user.click(following);
    expect(settingsSent(backend).follow_system_theme).toBe(true);
    // 暖纸 has no pair: following, the system's light or dark shows, and every tile stays open.
    expect(within(themes).getByRole("radio", { name: "明亮" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(within(themes).getByRole("radio", { name: /暖纸/ })).toBeEnabled();
    // The language last: the page speaks it at once.
    await user.click(screen.getByRole("radio", { name: "English" }));
    expect(settingsSent(backend).locale).toBe("en");
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
  });

  it("lay the themes out light beside dark, pick a column while following, and choose the code view", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("dock-settings"));
    await screen.findByTestId("page-settings");
    const themes = within(screen.getByRole("radiogroup", { name: "主题" }));
    expect(themes.getAllByRole("radio").map((r) => r.getAttribute("aria-label"))).toEqual([
      "明亮",
      "暗黑",
      "澄空",
      "澄空 · 夜",
      "墨白",
      "墨白 · 夜",
      "暖纸",
      "石墨",
    ]);
    expect(screen.getByText(/^跟随系统时，同一列的浅色与深色自动切换/)).toBeInTheDocument();
    await user.click(themes.getByRole("radio", { name: "澄空" }));
    expect(settingsSent(backend)).toMatchObject({ theme: "glass", follow_system_theme: true });
    expect(themes.getByTestId("theme-pair")).toHaveAttribute("data-column", "glass");
    await user.click(themes.getByRole("radio", { name: "墨白 · 夜" }));
    expect(settingsSent(backend)).toMatchObject({
      theme: "paper_night",
      follow_system_theme: false,
    });
    expect(document.documentElement.dataset.theme).toBe("paper_night");
    expect(themes.queryByTestId("theme-pair")).toBeNull();
    const view = within(screen.getByRole("radiogroup", { name: "验证码视图" }));
    await user.click(view.getByRole("radio", { name: "卡片" }));
    expect(settingsSent(backend).codes_view).toBe("grid");
    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(screen.getAllByTestId("entry-tile").length).toBeGreaterThan(0);
  });

  it("change the master password, the current one checked first", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("dock-settings"));
    await user.click(await screen.findByTestId("settings-password"));
    expect(await screen.findByTestId("page-password")).toBeInTheDocument();
    const submit = screen.getByRole("button", { name: "修改主密码" });
    await user.type(screen.getByLabelText("当前主密码"), "wrong password");
    await user.type(screen.getByLabelText("新主密码"), "a new pass phrase");
    await user.type(screen.getByLabelText("再输入一次"), "a new pass phras");
    expect(screen.getByText("两次输入的密码不一致")).toBeInTheDocument();
    expect(submit).toBeDisabled();
    await user.type(screen.getByLabelText("再输入一次"), "e");
    await user.click(submit);
    expect(await screen.findByText("密码错误")).toBeInTheDocument();
    await user.type(screen.getByLabelText("当前主密码"), MOCK_PASSWORD);
    await user.click(submit);
    expect(await screen.findByTestId("page-settings")).toBeInTheDocument();
    expect(await screen.findByRole("status")).toHaveTextContent("主密码已修改");
    expect(backend.calls.at(-1)).toEqual({
      command: "vault_change_password",
      current: MOCK_PASSWORD,
      new: "a new pass phrase",
    });
  });
});

describe("importing files on the phone", () => {
  it("opens a Lockra backup in the preview with its password", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-files"));
    expect(await screen.findByTestId("page-preview")).toBeInTheDocument();
    expect(backend.calls.some((c) => c.command === "import_backup_password")).toBe(false);
    const before = screen.getAllByTestId("candidate").length;
    backend.awaitBackup("old.lockrabackup");
    const form = within(await screen.findByTestId("backup-password"));
    await user.type(form.getByLabelText("「old.lockrabackup」需要备份密码"), "wrong");
    await user.click(form.getByRole("button", { name: "打开备份" }));
    expect(await form.findByText("密码错误")).toBeInTheDocument();
    await user.type(form.getByLabelText("「old.lockrabackup」需要备份密码"), MOCK_PASSWORD);
    await user.click(form.getByRole("button", { name: "打开备份" }));
    await waitFor(() => expect(screen.queryByTestId("backup-password")).not.toBeInTheDocument());
    expect(screen.getAllByTestId("candidate").length).toBeGreaterThan(before);
  });
});
