import { MOCK_PASSWORD, MockBackend } from "@lockra/shared/mock";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

const LINKS = [
  "otpauth://totp/Example:alice@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example",
  "otpauth://totp/Sample:bob@example.com?secret=KRSXG5CTMVRXEZLU&issuer=Sample",
].join("\n");

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
    // A group in use is a tap away.
    await user.click(form.getByRole("button", { name: "工作" }));
    expect(form.getByLabelText("分组")).toHaveValue("工作");
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
      screen.getByText("点按 + 添加：粘贴 otpauth 链接、从剪贴板导入，或手动输入密钥。"),
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
