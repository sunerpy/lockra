import { MOCK_PASSWORD, MockBackend, mockEntry } from "@lockra/shared/mock";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

const LINKS = [
  "otpauth://totp/Example:alice@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example",
  "otpauth://totp/Sample:bob@example.com?secret=KRSXG5CTMVRXEZLU&issuer=Sample",
].join("\n");

/** The fingerprint unlocks the vault and is the default unlock (the mock's default settings). */
const FINGERPRINT = {
  biometric: "fingerprint",
  biometricUnlock: true,
  deviceUnlock: true,
} as const;

function names(): string[] {
  return screen
    .getAllByTestId("entry-row")
    .map((row) => row.querySelector(".truncate")?.textContent ?? "");
}

function depth(): number {
  return depthOf(history.state);
}

/** The back gesture: the shell goes back in the webview's history. */
function goBack() {
  act(() => history.back());
}

// A test's pages leave the history as it unmounts; the next one starts on the codes.
afterEach(async () => {
  cleanup();
  await waitFor(() => {
    if (depth() !== 0) throw new Error(`the history is still ${depth()} pages deep`);
  });
});

describe("adding accounts on the phone", () => {
  it("adds one by hand from +, the core checking the secret, and lands on the codes", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByRole("button", { name: "添加" }));
    await user.click(await screen.findByTestId("add-manual"));
    const form = within(await screen.findByTestId("manual-form"));
    const add = form.getByRole("button", { name: "添加" });
    expect(add).toBeDisabled();
    await user.type(form.getByLabelText("服务名称"), "Example");
    await user.type(form.getByLabelText("密钥"), "not base32!");
    await user.click(add);
    expect(await form.findByText("密钥无效：应为 Base32 字母和数字")).toBeInTheDocument();
    // A period out of range opens the section that holds it.
    await user.clear(form.getByLabelText("密钥"));
    await user.type(form.getByLabelText("密钥"), "JBSWY3DPEHPK3PXP");
    await user.click(form.getByRole("button", { name: "高级设置" }));
    await user.clear(form.getByLabelText("周期（秒）"));
    await user.type(form.getByLabelText("周期（秒）"), "0");
    await user.click(form.getByRole("button", { name: "高级设置" }));
    await user.click(add);
    expect(form.getByTestId("advanced")).toBeInTheDocument();
    expect(form.getByRole("alert")).toHaveTextContent("位数、周期或计数器超出范围");
    await user.clear(form.getByLabelText("周期（秒）"));
    await user.type(form.getByLabelText("周期（秒）"), "60");
    await user.click(
      within(form.getByRole("radiogroup", { name: "位数" })).getByRole("radio", { name: "8" }),
    );
    // A group in use is a tap away, in a sheet from the bottom; the back gesture closes the sheet
    // and leaves the page; a new group's name goes in the sheet's field.
    await user.click(form.getByTestId("group-field"));
    let sheet = within(await screen.findByTestId("group-sheet"));
    expect(sheet.getAllByRole("option").map((o) => o.textContent)).toEqual(["不分组", "工作"]);
    act(() => history.back());
    await waitFor(() => expect(screen.queryByTestId("group-sheet")).toBeNull());
    expect(screen.getByTestId("manual-form")).toBeInTheDocument();
    await user.click(form.getByTestId("group-field"));
    sheet = within(await screen.findByTestId("group-sheet"));
    await user.type(sheet.getByLabelText("新建分组"), "个人");
    await user.click(sheet.getByRole("button", { name: "使用" }));
    expect(form.getByTestId("group-field")).toHaveTextContent("个人");
    await user.click(form.getByTestId("group-field"));
    sheet = within(await screen.findByTestId("group-sheet"));
    await user.click(sheet.getByRole("option", { name: "工作" }));
    expect(screen.queryByTestId("group-sheet")).toBeNull();
    expect(form.getByTestId("group-field")).toHaveTextContent("工作");
    await user.click(add);
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({
      command: "entry_add_manual",
      draft: { issuer: "Example", kind: { type: "totp", period: 60 }, digits: 8, group: "工作" },
    });
    expect(names()).toContain("Example");
    await waitFor(() => expect(depth()).toBe(0));
  });

  it("adds it with Enter in the secret field too", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-manual"));
    const form = within(await screen.findByTestId("manual-form"));
    await user.type(form.getByLabelText("服务名称"), "Example");
    await user.type(form.getByLabelText("密钥"), "JBSWY3DPEHPK3PXP{Enter}");
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({ command: "entry_add_manual" });
    expect(names()).toContain("Example");
  });

  it("offers + on an empty vault too", async () => {
    const { user } = renderApp({
      backend: new MockBackend({ phase: "unlocked", settings: { locale: "zh-cn" } }),
    });
    await ready();
    expect(
      screen.getByText("点按 + 添加：扫描二维码、读取截图、粘贴 otpauth 链接，或手动输入密钥。"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "添加账号" }));
    expect(await screen.findByTestId("page-add")).toBeInTheDocument();
  });

  it("reads pasted links into the preview and imports what the user keeps", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    const links = await screen.findByTestId("add-links");
    await user.click(links);
    await user.paste(LINKS);
    await user.click(screen.getByRole("button", { name: "读取" }));
    expect(await screen.findByTestId("page-preview")).toBeInTheDocument();
    const found = screen.getAllByTestId("candidate");
    expect(found).toHaveLength(2);
    expect(
      within(found[0] as HTMLElement).getByText("粘贴的文本 · SHA1 · 6 · 30 秒"),
    ).toBeInTheDocument();
    // The second one stays out.
    const second = within(found[1] as HTMLElement);
    await user.click(second.getByRole("radio", { name: "跳过" }));
    await user.click(screen.getByRole("button", { name: "导入 1 个账号" }));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({ command: "import_commit" });
    expect(names()).toContain("Example");
    expect(names()).not.toContain("Sample");
    await waitFor(() => expect(depth()).toBe(0));
  });

  it("drops the import when the preview is discarded or left with the back gesture", async () => {
    const { user, backend } = renderApp({ mock: { clipboard: LINKS } });
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-clipboard"));
    expect(await screen.findByTestId("page-preview")).toBeInTheDocument();
    expect(
      within(screen.getAllByTestId("candidate")[0] as HTMLElement).getByText(/^剪贴板/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "放弃这次导入" }));
    expect(await screen.findByTestId("page-add")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({ command: "import_cancel" });
    expect((await backend.getState()).import).toBeNull();

    await user.click(screen.getByTestId("add-clipboard"));
    expect(await screen.findByTestId("page-preview")).toBeInTheDocument();
    goBack();
    expect(await screen.findByTestId("page-add")).toBeInTheDocument();
    await waitFor(() => expect(backend.calls.at(-1)).toEqual({ command: "import_cancel" }));
    expect((await backend.getState()).import).toBeNull();
  });
});

describe("an account's actions on the phone", () => {
  it("open with a long press or ⋯, and pin the account", async () => {
    const { user, backend } = renderApp();
    await ready();
    const { entries } = await backend.getState();
    const row = screen
      .getAllByTestId("entry-row")
      .find((r) => entries.some((e) => e.id === r.dataset.entry && !e.favorite));
    if (!row) throw new Error("no account that is not pinned");
    const id = row.dataset.entry;
    fireEvent.contextMenu(row);
    expect(await screen.findByTestId("page-account")).toBeInTheDocument();
    expect(backend.calls.some((c) => c.command === "entry_copy")).toBe(false);
    expect(screen.getByTestId("account-pin")).toHaveTextContent("收藏");
    await user.click(screen.getByTestId("account-pin"));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({
      command: "entry_update",
      id,
      patch: { favorite: true },
    });
    // ⋯ opens the same page, now offering to unpin; pinned, the account went to the top.
    await user.click(screen.getAllByTestId("row-more")[0] as HTMLElement);
    expect(await screen.findByTestId("account-pin")).toHaveTextContent("取消收藏");
  });

  it("edit the names, group and look, then come back", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getAllByTestId("row-more")[0] as HTMLElement);
    await user.click(await screen.findByTestId("account-edit"));
    expect(await screen.findByTestId("page-edit")).toBeInTheDocument();
    await user.clear(screen.getByLabelText("服务名称"));
    await user.type(screen.getByLabelText("服务名称"), "Renamed");
    await user.click(
      within(screen.getByRole("radiogroup", { name: "颜色" })).getByRole("radio", { name: "紫色" }),
    );
    await user.click(screen.getByRole("button", { name: "保存" }));
    // Saved: one page back, to the account's actions.
    expect(await screen.findByTestId("page-account")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({
      command: "entry_update",
      patch: { issuer: "Renamed", color: "purple" },
    });
    goBack();
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(names()).toContain("Renamed");
  });

  it("delete after asking", async () => {
    const { user } = renderApp();
    await ready();
    const before = names();
    await user.click(screen.getAllByTestId("row-more")[0] as HTMLElement);
    await user.click(await screen.findByTestId("account-delete"));
    const dialog = within(await screen.findByRole("dialog"));
    expect(dialog.getByText(/删除后无法恢复/)).toBeInTheDocument();
    await user.click(dialog.getByRole("button", { name: "删除" }));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(names()).toHaveLength(before.length - 1);
    await waitFor(() => expect(depth()).toBe(0));
  });

  it("show the secret behind the master password, and end the secret view when done", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getAllByTestId("row-more")[0] as HTMLElement);
    await user.click(await screen.findByTestId("account-reveal"));
    const field = await screen.findByLabelText("主密码");
    await user.type(field, "wrong{Enter}");
    expect(await screen.findByText("密码错误")).toBeInTheDocument();
    await user.type(field, `${MOCK_PASSWORD}{Enter}`);
    expect((await screen.findByTestId("revealed-secret")).textContent).toMatch(/^[A-Z2-7 ]+$/);
    expect(screen.getByRole("img", { name: "二维码" })).toBeInTheDocument();
    expect(screen.getByTestId("reveal-countdown")).toHaveTextContent("120");
    expect(backend.calls.some((c) => c.command === "secret_view_closed")).toBe(false);
    await user.click(screen.getByRole("button", { name: "完成" }));
    expect(await screen.findByTestId("page-account")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({ command: "secret_view_closed" });
  });

  it("show the secret after the fingerprint, asked by itself where it is the default unlock", async () => {
    const { user, backend } = renderApp({ mock: FINGERPRINT });
    await ready();
    await user.click(screen.getAllByTestId("row-more")[0] as HTMLElement);
    await user.click(await screen.findByTestId("account-reveal"));
    expect((await screen.findByTestId("revealed-secret")).textContent).toMatch(/^[A-Z2-7 ]+$/);
    expect(backend.biometricReasons).toEqual(["显示账号的密钥"]);
    expect(backend.calls.find((c) => c.command === "entry_reveal")).not.toHaveProperty("password");
    await user.click(screen.getByRole("button", { name: "完成" }));
    expect(await screen.findByTestId("page-account")).toBeInTheDocument();
  });

  it("leave the password after a cancelled fingerprint, saying nothing", async () => {
    const { user, backend } = renderApp({
      mock: { ...FINGERPRINT, settings: { default_unlock: "password" } },
    });
    await ready();
    await user.click(screen.getAllByTestId("row-more")[0] as HTMLElement);
    await user.click(await screen.findByTestId("account-reveal"));
    // The password is the default unlock: the fingerprint waits for its button.
    expect(await screen.findByText(/验证身份以显示「.+」的密钥和二维码/)).toBeInTheDocument();
    expect(backend.biometricReasons).toEqual([]);
    backend.answerBiometric("biometric_cancelled");
    await user.click(screen.getByRole("button", { name: "使用指纹验证" }));
    await waitFor(() => expect(backend.biometricReasons).toEqual(["显示账号的密钥"]));
    expect(screen.queryByText("验证已取消")).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await screen.findByTestId("revealed-secret")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "完成" }));
    expect(await screen.findByTestId("page-account")).toBeInTheDocument();
  });

  it("closes a page whose account went away", async () => {
    const { user, backend } = renderApp();
    await ready();
    const id = screen.getAllByTestId("entry-row")[0]?.dataset.entry ?? "";
    await user.click(screen.getAllByTestId("row-more")[0] as HTMLElement);
    await user.click(await screen.findByTestId("account-edit"));
    await screen.findByTestId("page-edit");
    await act(() => backend.dispatch({ command: "entry_delete", id }));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    await waitFor(() => expect(depth()).toBe(0));
  });
});

describe("the pages and the back gesture", () => {
  it("close one at a time, and all of them when the vault locks", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-manual"));
    await screen.findByTestId("page-manual");
    goBack();
    expect(await screen.findByTestId("page-add")).toBeInTheDocument();
    // The page's own back button does the same.
    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();

    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-manual"));
    await screen.findByTestId("page-manual");
    await user.click(screen.getByRole("button", { name: "返回" }));
    await screen.findByTestId("page-add");
    await user.click(screen.getByRole("button", { name: "返回" }));
    await screen.findByTestId("page-codes");
    await user.click(screen.getByTestId("codes-add"));
    await screen.findByTestId("page-add");
    await act(() => backend.dispatch({ command: "vault_lock" }));
    expect(await screen.findByTestId("page-unlock")).toBeInTheDocument();
    await waitFor(() => expect(depth()).toBe(0));
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
  });
});

describe("reordering on the phone", () => {
  it("moves the accounts and the groups by their handles into an order this phone keeps", async () => {
    const backend = new MockBackend({
      entries: [
        mockEntry("GitHub", "me", { group: "Work", at: 1 }),
        mockEntry("Jira", "me", { group: "Work", at: 2 }),
        mockEntry("Bank", "me", { group: "Money", at: 3 }),
      ],
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("codes-reorder"));
    expect(screen.queryByTestId("codes-search")).toBeNull();
    const handle = (name: string) =>
      screen.getByRole("button", { name: `移动「${name}」：拖动，或按上下方向键` });
    // A 44 px handle for the finger.
    expect(handle("Jira").className).toContain("size-11");
    handle("Work").focus();
    await user.keyboard("{ArrowUp}");
    handle("Jira").focus();
    await user.keyboard("{ArrowUp}");
    const last = (command: string) => backend.calls.filter((c) => c.command === command).at(-1);
    expect(last("view_order_groups")).toEqual({
      command: "view_order_groups",
      groups: ["Work", "Money"],
    });
    expect(last("view_order_entries")?.command).toBe("view_order_entries");
    await user.click(screen.getByTestId("codes-reorder-done"));
    expect(names()).toEqual(["Jira", "GitHub", "Bank"]);
  });
});
