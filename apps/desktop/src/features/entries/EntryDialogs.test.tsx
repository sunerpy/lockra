import { MOCK_PASSWORD } from "@lockra/shared/mock";
import { screen, within } from "@testing-library/react";
import { ready, renderApp } from "../../test/render";
import { parseKind } from "./EntryDialogs";

async function openFromMenu(user: ReturnType<typeof renderApp>["user"], item: string, row = 0) {
  const trigger = screen.getAllByTestId("row-menu")[row];
  if (!trigger) throw new Error("no row menu");
  await user.click(trigger);
  await user.click(screen.getByRole("menuitem", { name: item }));
}

describe("parseKind", () => {
  it("accepts the core's ranges only", () => {
    expect(parseKind("totp", "30", "0")).toEqual({ type: "totp", period: 30 });
    expect(parseKind("totp", "0", "0")).toBeUndefined();
    expect(parseKind("totp", "3601", "0")).toBeUndefined();
    expect(parseKind("totp", "1.5", "0")).toBeUndefined();
    expect(parseKind("hotp", "30", "7")).toEqual({ type: "hotp", counter: 7 });
    expect(parseKind("hotp", "30", "")).toBeUndefined();
    expect(parseKind("hotp", "30", "-1")).toBeUndefined();
  });
});

describe("EntryDialogs", () => {
  it("adds an account by hand, with the secret checked by the core", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.keyboard("{Control>}n{/Control}");
    const dialog = within(screen.getByRole("dialog", { name: "手动添加账号" }));
    const add = dialog.getByRole("button", { name: "添加" });
    expect(add).toBeDisabled();
    await user.type(dialog.getByLabelText("服务名称"), "Example");
    await user.type(dialog.getByLabelText("账号"), "me@example.com");
    await user.type(dialog.getByLabelText("密钥"), "not base32!");
    await user.click(add);
    expect(await dialog.findByText("密钥无效：应为 Base32 字母和数字")).toBeInTheDocument();
    await user.clear(dialog.getByLabelText("密钥"));
    await user.type(dialog.getByLabelText("密钥"), "jbsw y3dp ehpk 3pxp");
    await user.type(dialog.getByLabelText("分组"), "家庭");
    await user.click(add);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({
      command: "entry_add_manual",
      draft: {
        issuer: "Example",
        account: "me@example.com",
        kind: { type: "totp", period: 30 },
        digits: 6,
        algorithm: "sha1",
        group: "家庭",
      },
    });
    expect(await screen.findByText("Example")).toBeInTheDocument();
  });

  it("opens the advanced settings for parameters out of range, and adds a counter-based account", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.keyboard("{Control>}n{/Control}");
    const dialog = within(screen.getByRole("dialog", { name: "手动添加账号" }));
    await user.type(dialog.getByLabelText("密钥"), "JBSWY3DPEHPK3PXP");
    await user.click(dialog.getByRole("button", { name: "高级设置" }));
    await user.clear(dialog.getByLabelText("周期（秒）"));
    await user.type(dialog.getByLabelText("周期（秒）"), "0");
    await user.click(dialog.getByRole("button", { name: "高级设置" }));
    expect(dialog.queryByTestId("advanced")).toBeNull();
    await user.click(dialog.getByRole("button", { name: "添加" }));
    expect(dialog.getByTestId("advanced")).toBeInTheDocument();
    expect(dialog.getByText("位数、周期或计数器超出范围")).toBeInTheDocument();
    await user.click(dialog.getByRole("radio", { name: "基于计数器（HOTP）" }));
    await user.clear(dialog.getByLabelText("计数器"));
    await user.type(dialog.getByLabelText("计数器"), "5");
    await user.click(dialog.getByRole("radio", { name: "8" }));
    await user.click(dialog.getByRole("radio", { name: "SHA256" }));
    await user.click(dialog.getByRole("button", { name: "添加" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "entry_add_manual",
      draft: { kind: { type: "hotp", counter: 5 }, digits: 8, algorithm: "sha256", group: null },
    });
  });

  it("adds an account from an otpauth link; Enter submits", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("add-menu"));
    await user.click(screen.getByRole("menuitem", { name: "粘贴 otpauth 链接…" }));
    const dialog = within(screen.getByRole("dialog", { name: "粘贴 otpauth 链接" }));
    await user.type(dialog.getByLabelText("otpauth 链接"), "https://example.com{Enter}");
    expect(await dialog.findByText("不是有效的 otpauth 链接")).toBeInTheDocument();
    await user.clear(dialog.getByLabelText("otpauth 链接"));
    await user.type(
      dialog.getByLabelText("otpauth 链接"),
      "otpauth://totp/Linked:me?secret=JBSWY3DPEHPK3PXP{Enter}",
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({ command: "entry_add_uri" });
    expect(await screen.findByText("Linked")).toBeInTheDocument();
  });

  it("edits the names, the group and the favourite", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openFromMenu(user, "编辑…");
    const dialog = within(screen.getByRole("dialog", { name: "编辑账号" }));
    expect(dialog.getByText(/SHA1 · 6 · 30 秒 · otpauth 链接/)).toBeInTheDocument();
    await user.clear(dialog.getByLabelText("服务名称"));
    await user.type(dialog.getByLabelText("服务名称"), "GitHub Enterprise");
    await user.click(dialog.getByRole("switch", { name: "收藏" }));
    await user.click(dialog.getByRole("button", { name: "保存" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({
      command: "entry_update",
      patch: { issuer: "GitHub Enterprise", favorite: false, group: "工作" },
    });
  });

  it("deletes after confirming", async () => {
    const { user } = renderApp();
    await ready();
    const before = screen.getAllByTestId("entry-row").length;
    await openFromMenu(user, "删除…");
    const dialog = within(screen.getByRole("dialog", { name: "删除「GitHub: octocat」" }));
    await user.click(dialog.getByRole("button", { name: "删除" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("entry-row")).toHaveLength(before - 1);
  });

  it("reveals the secret behind the master password and ends the secret view on close", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openFromMenu(user, "显示密钥…");
    const prompt = within(screen.getByRole("dialog", { name: "显示密钥" }));
    expect(
      prompt.getByText("输入主密码以显示「GitHub: octocat」的密钥和二维码。"),
    ).toBeInTheDocument();
    await user.type(prompt.getByLabelText("主密码"), "wrong{Enter}");
    expect(await prompt.findByText("密码错误")).toBeInTheDocument();
    await user.type(prompt.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    const shown = within(await screen.findByRole("dialog", { name: "GitHub: octocat" }));
    expect(shown.getByTestId("revealed-secret").textContent).toMatch(/^[A-Z2-7 ]+$/);
    expect(shown.getByRole("img", { name: "二维码" })).toBeInTheDocument();
    expect(shown.getByText("Linux 无法阻止截屏或录屏，请注意屏幕共享。")).toBeInTheDocument();
    expect(shown.getByTestId("reveal-countdown")).toHaveTextContent("二维码将在 120 秒后隐藏");
    expect(backend.calls.some((c) => c.command === "secret_view_closed")).toBe(false);
    await user.click(shown.getByRole("button", { name: "完成" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({ command: "secret_view_closed" });
  });

  it("closes a dialog whose account went away", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openFromMenu(user, "编辑…");
    const id = screen.getAllByTestId("entry-row")[0]?.dataset.entry ?? "";
    await backend.dispatch({ command: "entry_delete", id });
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
