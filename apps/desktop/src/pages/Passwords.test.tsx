import {
  MOCK_PASSWORD,
  MockBackend,
  type MockOptions,
  sampleEntries,
  sampleItems,
} from "@lockra/shared/mock";
import { act, fireEvent, screen, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { ready, renderApp } from "../test/render";

/** The app with password management on and the sample logins (twelve, three favourites). */
function renderPasswords(options: MockOptions = {}) {
  const entries = sampleEntries();
  const backend = new MockBackend({
    entries,
    passwords: true,
    items: sampleItems(entries),
    ...options,
    settings: { locale: "zh-cn", ...options.settings },
  });
  return renderApp({ backend });
}

async function openPage(user: UserEvent) {
  await ready();
  await user.click(
    within(screen.getByRole("navigation", { name: "主导航" })).getByRole("button", {
      name: /^密码/,
    }),
  );
  return within(await screen.findByTestId("page-passwords"));
}

function rows(): HTMLElement[] {
  return screen.getAllByTestId("password-row");
}

function names(): string[] {
  return rows().map((r) => r.querySelector(".truncate")?.textContent ?? "");
}

function row(name: string): HTMLElement {
  const found = rows().find((r) => r.querySelector(".truncate")?.textContent === name);
  if (!found) throw new Error(`no row ${name}`);
  return found;
}

function detail() {
  return within(screen.getByTestId("item-detail"));
}

function lastCall(backend: MockBackend, command: string) {
  return backend.calls.filter((c) => c.command === command).at(-1);
}

describe("Passwords", () => {
  it("is offered only where password management is", async () => {
    const { user } = renderApp();
    await ready();
    const nav = within(screen.getByRole("navigation", { name: "主导航" }));
    expect(nav.queryByRole("button", { name: /^密码/ })).toBeNull();
    await user.keyboard("{Control>}k{/Control}");
    const palette = within(await screen.findByRole("dialog", { name: "命令菜单" }));
    expect(palette.queryByText("密码")).toBeNull();
  });

  it("lists the logins in their groups, with the facts, the filters and the tags (board 01)", async () => {
    const { user } = renderPasswords();
    const page = await openPage(user);
    expect(page.getByRole("heading", { level: 1, name: "密码" })).toBeInTheDocument();
    expect(page.getByTestId("page-subtitle")).toHaveTextContent("12 个登录项 · 2 个分组");
    expect(
      screen.getAllByTestId("passwords-section").map((s) => s.getAttribute("aria-label")),
    ).toEqual(["个人", "工作", "未分组"]);
    // Favourites first in each group, then by name.
    expect(names().slice(4, 9)).toEqual(["GitHub", "AWS 控制台", "Cloudflare", "Figma", "Notion"]);
    expect(within(row("GitHub")).getByTestId("row-favorite-mark")).toBeInTheDocument();
    expect(within(row("招商银行")).getByTestId("row-protected")).toBeInTheDocument();
    expect(row("GitHub")).toHaveTextContent("验证码");
    expect(row("V2EX")).toHaveTextContent("冲突");

    const chips = within(page.getByTestId("passwords-chips"));
    await user.click(chips.getByRole("radio", { name: "收藏 3" }));
    expect(names()).toEqual(["GitHub", "Google", "招商银行"]);
    expect(screen.queryAllByTestId("passwords-section")).toHaveLength(0);
    await user.click(chips.getByRole("radio", { name: /^全部/ }));

    await user.click(page.getByTestId("passwords-tags-menu"));
    await user.click(screen.getByRole("menuitemcheckbox", { name: "开发 · 3" }));
    expect(names()).toEqual(["GitHub", "AWS 控制台", "Cloudflare"]);
    await user.click(chips.getByRole("radio", { name: "个人 4" }));
    expect(screen.queryAllByTestId("password-row")).toHaveLength(0);
    await user.click(page.getByTestId("passwords-tags-clear"));
    expect(names()).toHaveLength(4);
  });

  it("searches, and offers a new login named after what found nothing", async () => {
    const { user } = renderPasswords();
    const page = await openPage(user);
    const search = page.getByTestId("passwords-search");
    await user.type(search, "acme");
    // The user names and the email addresses are searched too.
    expect(names()).toEqual(["AWS 控制台", "Cloudflare", "Figma", "Notion"]);
    await user.clear(search);
    await user.type(search, "paypal");
    expect(page.getByText("没有匹配「paypal」的登录项")).toBeInTheDocument();
    await user.click(page.getByTestId("passwords-create-from-search"));
    const form = within(screen.getByRole("dialog", { name: "新建登录项" }));
    expect(form.getByTestId("item-form-title")).toHaveValue("paypal");
  });

  it("selects with a click or the arrows, shows the details and copies with Enter", async () => {
    const { user, backend } = renderPasswords();
    const page = await openPage(user);
    expect(page.getByTestId("item-detail-empty")).toHaveTextContent("选择一个登录项");
    await user.click(row("GitHub"));
    expect(row("GitHub")).toHaveAttribute("aria-selected", "true");
    expect(detail().getByRole("heading", { level: 2, name: "GitHub" })).toBeInTheDocument();
    expect(detail().getByText("octocat")).toBeInTheDocument();
    expect(detail().getByTestId("detail-code")).toHaveTextContent("关联「GitHub: octocat」");
    expect(detail().getByText("https://github.com/login")).toBeInTheDocument();
    expect(detail().getByText("2 条")).toBeInTheDocument();
    await user.keyboard("{ArrowDown}");
    expect(row("AWS 控制台")).toHaveAttribute("aria-selected", "true");
    expect(detail().getByRole("heading", { level: 2, name: "AWS 控制台" })).toBeInTheDocument();
    await user.keyboard("{ArrowUp}{Enter}");
    expect(lastCall(backend, "item_copy")).toMatchObject({ part: { type: "password" } });
    expect(within(row("GitHub")).getByTestId("row-copied")).toHaveTextContent("已复制密码");
    await user.click(within(row("Figma")).getByTestId("row-copy-username"));
    expect(lastCall(backend, "item_copy")).toMatchObject({ part: { type: "email" } });
  });

  it("shows the notes and hidden fields on request and hides them after two minutes", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const { user, backend } = renderPasswords();
      await openPage(user);
      await user.click(row("GitHub"));
      await user.click(detail().getByTestId("detail-notes-show"));
      expect(detail().getByTestId("detail-notes")).toHaveTextContent(
        "公司组织 acme-corp 的管理员。",
      );
      expect(lastCall(backend, "item_reveal")).toMatchObject({ parts: { notes: true } });
      await user.click(detail().getByTestId("detail-field-show"));
      expect(detail().getByText("a1b2-c3d4-e5f6")).toBeInTheDocument();
      expect(backend.calls.some((c) => c.command === "secret_view_closed")).toBe(false);
      act(() => {
        vi.advanceTimersByTime(121_000);
      });
      expect(detail().queryByTestId("detail-notes")).toBeNull();
      expect(detail().queryByText("a1b2-c3d4-e5f6")).toBeNull();
      expect(lastCall(backend, "secret_view_closed")).toBeDefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it("asks a 「填充前验证」 login for the master password before its secret leaves", async () => {
    const { user, backend } = renderPasswords();
    await openPage(user);
    await user.click(within(row("招商银行")).getByTestId("row-copy-password"));
    const verify = within(screen.getByRole("dialog", { name: "验证身份" }));
    expect(verify.getByText("输入主密码以复制「招商银行」。")).toBeInTheDocument();
    await user.type(verify.getByLabelText("主密码"), "wrong{Enter}");
    expect(await verify.findByText("密码错误")).toBeInTheDocument();
    await user.type(verify.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(lastCall(backend, "item_copy")).toMatchObject({
      part: { type: "password" },
      password: MOCK_PASSWORD,
    });
    // The user name is no secret: it goes without asking.
    await user.click(row("招商银行"));
    await user.click(detail().getByTestId("detail-copy-username"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows a password in its own dialog, then ends the secret view (board 08)", async () => {
    const { user, backend } = renderPasswords({ platform: "linux" });
    await openPage(user);
    await user.click(within(row("GitHub")).getByTestId("password-row-menu"));
    await user.click(screen.getByRole("menuitem", { name: "显示密码…" }));
    const shown = within(await screen.findByRole("dialog", { name: "GitHub · octocat" }));
    expect(shown.getByTestId("item-revealed-password")).toHaveTextContent("x7#Kq9!vLm2@Rt4z");
    expect(shown.getByText("16 个字符 · 大小写字母、数字和符号")).toBeInTheDocument();
    expect(shown.getByText("Linux 无法阻止截屏或录屏，请注意屏幕共享。")).toBeInTheDocument();
    expect(shown.getByTestId("item-reveal-countdown")).toHaveTextContent("密码将在 120 秒后隐藏");
    await user.click(shown.getByTestId("item-reveal-copy"));
    expect(lastCall(backend, "item_copy")).toMatchObject({ part: { type: "password" } });
    expect(backend.calls.some((c) => c.command === "secret_view_closed")).toBe(false);
    await user.click(shown.getByRole("button", { name: "完成" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(backend.calls.at(-1)).toEqual({ command: "secret_view_closed" });
  });

  it("adds a login with a generated password and selects it (boards 06, 09)", async () => {
    const { user, backend } = renderPasswords();
    await openPage(user);
    await user.keyboard("{Control>}n{/Control}");
    const dialog = screen.getByRole("dialog", { name: "新建登录项" });
    const form = within(dialog);
    expect(form.getByTestId("item-form-save")).toBeDisabled();
    await user.type(form.getByTestId("item-form-title"), "Example");
    await user.type(form.getByTestId("item-form-username"), "me");
    await user.type(form.getByRole("textbox", { name: "网站" }), "not a url");
    await user.click(form.getByTestId("item-form-save"));
    expect(form.getByTestId("item-form-errors")).toHaveTextContent("有 1 处需要修改");
    expect(form.getByText("这不是一个网址")).toBeInTheDocument();
    const site = form.getByRole("textbox", { name: "网站" });
    expect(site).toHaveFocus();
    await user.clear(site);
    await user.type(site, "example.com");

    await user.click(form.getByTestId("item-form-generate"));
    const generator = within(screen.getByRole("dialog", { name: "生成密码" }));
    const made = await generator.findByTestId("generator-password");
    await vi.waitFor(() => expect(made.textContent).not.toBe("—"));
    const password = made.textContent ?? "";
    expect(lastCall(backend, "generator_generate")).toMatchObject({ domain: "example.com" });
    await user.click(generator.getByTestId("generator-use"));
    expect(form.getByTestId("item-form-password")).toHaveValue(password);

    await user.click(form.getByTestId("item-form-add-field"));
    const field = within(form.getByTestId("item-form-field"));
    await user.type(field.getByRole("textbox", { name: "名称" }), "PIN");
    await user.click(field.getByRole("radio", { name: "隐藏" }));
    await user.type(field.getByLabelText("值"), "1234");
    await user.click(form.getByRole("radio", { name: /填充前验证/ }));
    await user.click(form.getByTestId("item-form-save"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(lastCall(backend, "item_add")).toMatchObject({
      draft: {
        title: "Example",
        username: "me",
        password: { type: "generated" },
        urls: [{ url: "example.com", matching: "domain" }],
        protection: "confirm",
        fields: [{ label: "PIN", value: { type: "set", value: "1234" }, hidden: true }],
      },
    });
    expect(row("Example")).toHaveAttribute("aria-selected", "true");
    expect(detail().getByRole("heading", { level: 2, name: "Example" })).toBeInTheDocument();
  });

  it("edits only what changed, keeps the secrets it did not show, and deletes after confirming", async () => {
    const { user, backend } = renderPasswords();
    await openPage(user);
    await user.click(row("GitHub"));
    await user.click(detail().getByTestId("item-edit"));
    let form = within(screen.getByRole("dialog", { name: "编辑登录项" }));
    expect(form.getByTestId("item-form-password")).toHaveValue("");
    expect(form.getByTestId("item-form-password")).toHaveAttribute("placeholder", "（不变）");
    expect(form.getByTestId("item-form-code")).toHaveTextContent("GitHub");
    const title = form.getByTestId("item-form-title");
    await user.clear(title);
    await user.type(title, "GitHub 公司");
    // Typing a password and taking it back keeps the old one.
    await user.type(form.getByTestId("item-form-password"), "x");
    await user.clear(form.getByTestId("item-form-password"));
    await user.click(form.getByTestId("item-form-notes-load"));
    expect(form.getByTestId("item-form-notes")).toHaveValue(
      "公司组织 acme-corp 的管理员。\n恢复码打印件在抽屉里。",
    );
    await user.click(form.getByTestId("item-form-code-unlink"));
    await user.click(form.getByTestId("item-form-save"));
    const patch = lastCall(backend, "item_update");
    expect(patch).toMatchObject({ patch: { title: "GitHub 公司", totp_entry: null } });
    expect(patch).not.toHaveProperty("patch.password");
    expect(patch).toHaveProperty("patch.notes", {
      type: "set",
      value: "公司组织 acme-corp 的管理员。\n恢复码打印件在抽屉里。",
    });
    expect(backend.calls.at(-1)).toEqual({ command: "secret_view_closed" });

    await user.click(detail().getByTestId("item-edit"));
    form = within(screen.getByRole("dialog", { name: "编辑登录项" }));
    await user.click(form.getByTestId("item-form-delete"));
    const confirm = within(screen.getByRole("dialog", { name: "删除登录项？" }));
    expect(confirm.getByTestId("item-delete")).toHaveTextContent(
      "「GitHub 公司」会从这台设备上删除",
    );
    await user.click(confirm.getByRole("button", { name: "取消" }));
    expect(screen.getByRole("dialog", { name: "编辑登录项" })).toBeInTheDocument();
    await user.click(form.getByTestId("item-form-delete"));
    await user.click(
      within(screen.getByRole("dialog", { name: "删除登录项？" })).getByTestId(
        "item-delete-confirm",
      ),
    );
    expect(lastCall(backend, "item_delete")).toMatchObject({ ids: [expect.any(String)] });
    await vi.waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(names()).not.toContain("GitHub 公司");
    expect(screen.getByTestId("item-detail-empty")).toBeInTheDocument();
  });

  it("settles a conflict with the other value (board 05-c)", async () => {
    const { user, backend } = renderPasswords();
    await openPage(user);
    await user.click(row("V2EX"));
    const conflict = within(detail().getByTestId("item-conflict"));
    expect(conflict.getByTestId("item-conflict-take")).toBeDisabled();
    await user.click(conflict.getByTestId("item-conflict-show"));
    expect(conflict.getByText("v2ex-Other#99")).toBeInTheDocument();
    await user.click(conflict.getByTestId("item-conflict-take"));
    expect(lastCall(backend, "item_update")).toMatchObject({
      patch: { settle: [{ field: "password", take: "v2-1" }] },
    });
    await vi.waitFor(() => expect(detail().queryByTestId("item-conflict")).toBeNull());
    expect(row("V2EX")).not.toHaveTextContent("冲突");
  });

  it("folds a group and keeps it folded (the core keeps it per device)", async () => {
    const { user, backend } = renderPasswords();
    const page = await openPage(user);
    const work = screen
      .getAllByTestId("passwords-section")
      .find((s) => s.getAttribute("aria-label") === "工作");
    if (!work) throw new Error("no 工作");
    await user.click(within(work).getByTestId("passwords-group-toggle"));
    expect(lastCall(backend, "view_collapse_items")).toEqual({
      command: "view_collapse_items",
      groups: ["工作"],
    });
    expect(within(work).queryAllByTestId("password-row")).toHaveLength(0);
    await user.click(page.getByTestId("passwords-more"));
    await user.click(screen.getByRole("menuitem", { name: "全部展开" }));
    expect(lastCall(backend, "view_collapse_items")).toEqual({
      command: "view_collapse_items",
      groups: [],
    });
    expect(names()).toHaveLength(12);
  });

  it("sorts as chosen, the choice kept in the settings", async () => {
    const { user, backend } = renderPasswords();
    const page = await openPage(user);
    await user.selectOptions(page.getByTestId("passwords-sort"), "recent");
    expect(lastCall(backend, "settings_set")).toMatchObject({ settings: { item_sort: "recent" } });
    // Figma was used last; the favourites stay first.
    const work = screen
      .getAllByTestId("passwords-section")
      .find((s) => s.getAttribute("aria-label") === "工作");
    expect(
      within(work ?? document.body)
        .getAllByTestId("password-row")
        .map((r) => r.querySelector(".truncate")?.textContent),
    ).toEqual(["GitHub", "Figma", "AWS 控制台", "Cloudflare", "Notion"]);
  });

  it("opens a login's menu with a right click", async () => {
    const { user } = renderPasswords();
    await openPage(user);
    fireEvent.contextMenu(row("Notion"), { clientX: 200, clientY: 300 });
    const menu = within(screen.getByTestId("password-context"));
    await user.click(menu.getByRole("menuitem", { name: "编辑…" }));
    expect(
      within(screen.getByRole("dialog", { name: "编辑登录项" })).getByTestId("item-form-title"),
    ).toHaveValue("Notion");
  });

  it("copies from the command palette: Enter the password, Tab to the other actions (board 11)", async () => {
    const { user, backend } = renderPasswords();
    await ready();
    await user.keyboard("{Control>}k{/Control}");
    const palette = within(await screen.findByRole("dialog", { name: "命令菜单" }));
    await user.keyboard("Figma");
    expect(palette.getByTitle("Figma · alex@acme.dev")).toBeInTheDocument();
    await user.keyboard("{Tab}{Enter}");
    expect(lastCall(backend, "item_copy")).toMatchObject({ part: { type: "email" } });
    await user.keyboard("{Control>}k{/Control}");
    await user.keyboard("Figma{Enter}");
    expect(lastCall(backend, "item_copy")).toMatchObject({ part: { type: "password" } });
  });

  it("opens the generator with Ctrl G: a new password on request, copied by Rust", async () => {
    const { user, backend } = renderPasswords();
    await ready();
    await user.keyboard("{Control>}g{/Control}");
    const generator = within(await screen.findByRole("dialog", { name: "生成密码" }));
    await vi.waitFor(() =>
      expect(generator.getByTestId("generator-password").textContent).not.toBe("—"),
    );
    const before = backend.calls.filter((c) => c.command === "generator_generate").length;
    await user.click(generator.getByTestId("generator-regenerate"));
    await vi.waitFor(() =>
      expect(backend.calls.filter((c) => c.command === "generator_generate").length).toBe(
        before + 1,
      ),
    );
    await user.click(generator.getByRole("radio", { name: "口令" }));
    await vi.waitFor(() =>
      expect(lastCall(backend, "generator_generate")).toMatchObject({
        options: { kind: "passphrase" },
      }),
    );
    await user.click(generator.getByTestId("generator-copy"));
    expect(lastCall(backend, "generator_copy")).toBeDefined();
    expect(lastCall(backend, "settings_set")).toMatchObject({
      settings: { generator: { kind: "passphrase" } },
    });
    await user.click(generator.getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(backend.calls.at(-1)).toEqual({ command: "secret_view_closed" });
  });
});
