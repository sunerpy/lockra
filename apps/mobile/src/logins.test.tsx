import {
  MOCK_PASSWORD,
  MockBackend,
  type MockOptions,
  sampleEntries,
  sampleItems,
} from "@lockra/shared/mock";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { ready, renderApp } from "./test/render";

/** The phone with password management on and the sample logins (twelve, three favourites). */
function phone(options: MockOptions = {}) {
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

async function openLogins(user: UserEvent) {
  await ready();
  await user.click(screen.getByTestId("dock-logins"));
  return within(await screen.findByTestId("page-logins"));
}

function card(name: string): HTMLElement {
  const found = screen
    .getAllByTestId("password-row")
    .find((r) => r.querySelector(".truncate")?.textContent === name);
  if (!found) throw new Error(`no login ${name}`);
  return found;
}

function names(): string[] {
  return screen
    .getAllByTestId("password-row")
    .map((r) => r.querySelector(".truncate")?.textContent ?? "");
}

function lastCall(backend: MockBackend, command: string) {
  return backend.calls.filter((c) => c.command === command).at(-1);
}

describe("the phone's logins", () => {
  it("are in the dock only where password management is", async () => {
    renderApp();
    await ready();
    const dock = within(screen.getByTestId("codes-dock"));
    expect(dock.queryByTestId("dock-logins")).toBeNull();
    expect(dock.getByTestId("dock-scan")).toBeInTheDocument();
  });

  it("list the logins in their groups behind the dock's 密码 (board 21)", async () => {
    const { user } = phone();
    const page = await openLogins(user);
    expect(page.getByRole("heading", { level: 1, name: "密码" })).toBeInTheDocument();
    expect(page.getByTestId("logins-subtitle")).toHaveTextContent("已解锁 · 12 个登录项");
    const dock = within(screen.getByTestId("codes-dock"));
    expect(dock.getByTestId("dock-logins")).toHaveAttribute("aria-current", "page");
    expect(dock.getByTestId("dock-new-login")).toHaveAccessibleName("新建登录项");
    expect(
      screen.getAllByTestId("logins-section").map((s) => s.getAttribute("aria-label")),
    ).toEqual(["个人", "工作", "未分组"]);
    await user.click(
      within(page.getByTestId("logins-chips")).getByRole("radio", { name: "收藏 3" }),
    );
    expect(names()).toEqual(["GitHub", "Google", "招商银行"]);
    await user.click(
      within(page.getByTestId("logins-chips")).getByRole("radio", { name: /^全部/ }),
    );
    await user.type(page.getByTestId("logins-search"), "paypal");
    await user.click(page.getByTestId("logins-create-from-search"));
    expect(await screen.findByTestId("page-login-edit")).toBeInTheDocument();
    expect(screen.getByTestId("login-title")).toHaveValue("paypal");
    // Back: the list again; 验证码 goes back to the codes.
    act(() => history.back());
    await screen.findByTestId("page-logins");
    await user.click(within(screen.getByTestId("codes-dock")).getByTestId("dock-codes"));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
  });

  it("folds a group and keeps it folded", async () => {
    const { user, backend } = phone();
    await openLogins(user);
    const work = screen
      .getAllByTestId("logins-section")
      .find((s) => s.getAttribute("aria-label") === "工作");
    if (!work) throw new Error("no 工作");
    await user.click(within(work).getByTestId("logins-group-toggle"));
    expect(lastCall(backend, "view_collapse_items")).toEqual({
      command: "view_collapse_items",
      groups: ["工作"],
    });
    await waitFor(() => expect(within(work).queryAllByTestId("password-row")).toHaveLength(0));
  });

  it("copies a password from its card, and opens a login's actions with ⋯ (board 23)", async () => {
    const { user, backend } = phone();
    await openLogins(user);
    await user.click(within(card("Figma")).getByTestId("row-copy-password"));
    expect(lastCall(backend, "item_copy")).toMatchObject({ part: { type: "password" } });
    expect(within(card("Figma")).getByTestId("row-copied")).toHaveTextContent("已复制密码");
    await user.click(within(card("GitHub")).getByTestId("login-more"));
    const sheet = within(await screen.findByTestId("login-sheet"));
    expect(sheet.getByRole("heading", { name: "GitHub · octocat" })).toBeInTheDocument();
    expect(sheet.getByTestId("sheet-copy-username")).toHaveTextContent("octocat");
    expect(sheet.getByTestId("sheet-copy-code").textContent).toMatch(/\d{3} ?\d{3}/);
    await user.click(sheet.getByTestId("sheet-copy-code"));
    expect(lastCall(backend, "item_copy")).toMatchObject({ part: { type: "totp" } });
    await waitFor(() => expect(screen.queryByTestId("login-sheet")).toBeNull());
    // A long press opens the same; 编辑 goes to the form once the sheet has gone.
    fireEvent.contextMenu(card("Notion"));
    await user.click(within(await screen.findByTestId("login-sheet")).getByTestId("sheet-edit"));
    expect(await screen.findByTestId("page-login-edit")).toBeInTheDocument();
    expect(screen.getByTestId("login-title")).toHaveValue("Notion");
  });

  it("deletes a login after asking", async () => {
    const { user, backend } = phone();
    await openLogins(user);
    await user.click(within(card("Netflix")).getByTestId("login-more"));
    await user.click(within(await screen.findByTestId("login-sheet")).getByTestId("sheet-delete"));
    const dialog = within(await screen.findByRole("dialog", { name: "删除登录项？" }));
    await user.click(dialog.getByTestId("login-delete-confirm"));
    expect(lastCall(backend, "item_delete")).toMatchObject({ ids: [expect.any(String)] });
    await waitFor(() => expect(names()).not.toContain("Netflix"));
  });

  it("asks a 「填充前验证」 login for the master password before a copy (board 29)", async () => {
    const { user, backend } = phone();
    await openLogins(user);
    await user.click(within(card("招商银行")).getByTestId("row-copy-password"));
    const verify = within(await screen.findByTestId("verify-sheet"));
    expect(verify.getByText("输入主密码以复制「招商银行」。")).toBeInTheDocument();
    await user.type(verify.getByLabelText("主密码"), "wrong{Enter}");
    expect(await verify.findByText("密码错误")).toBeInTheDocument();
    await user.type(verify.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    await waitFor(() => expect(screen.queryByTestId("verify-sheet")).toBeNull());
    expect(lastCall(backend, "item_copy")).toMatchObject({
      part: { type: "password" },
      password: MOCK_PASSWORD,
    });
  });

  it("takes the fingerprint that unlocks the vault as the proof", async () => {
    const { user, backend } = phone({
      biometric: "fingerprint",
      deviceUnlock: true,
      biometricUnlock: true,
      settings: { default_unlock: "password" },
    });
    await openLogins(user);
    await user.click(within(card("招商银行")).getByTestId("row-copy-password"));
    const verify = within(await screen.findByTestId("verify-sheet"));
    expect(verify.getByText("留空则用指纹验证。")).toBeInTheDocument();
    // Without the password, the fingerprint.
    await user.click(verify.getByTestId("verify-submit"));
    await waitFor(() => expect(screen.queryByTestId("verify-sheet")).toBeNull());
    expect(lastCall(backend, "item_copy")).toMatchObject({
      part: { type: "password" },
      reason: "复制「招商银行」",
    });
  });
});

describe("a login's page", () => {
  it("shows its parts, its notes and hidden fields on request, hidden again after two minutes (boards 24, 25)", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const { user, backend } = phone();
      await openLogins(user);
      await user.click(card("GitHub"));
      const page = within(await screen.findByTestId("page-login"));
      expect(page.getByRole("heading", { level: 2, name: "GitHub" })).toBeInTheDocument();
      expect(page.getByText("工作 · 开发、常用")).toBeInTheDocument();
      expect(page.getByTestId("login-code-row")).toHaveTextContent("关联「GitHub: octocat」");
      expect(page.getByText("https://github.com/login")).toBeInTheDocument();
      expect(page.getByText("跟随桌面设置")).toBeInTheDocument();
      await user.click(page.getByTestId("login-copy-username"));
      expect(lastCall(backend, "item_copy")).toMatchObject({ part: { type: "username" } });
      await user.click(page.getByTestId("login-notes-show"));
      expect(page.getByTestId("login-notes")).toHaveTextContent("公司组织 acme-corp 的管理员。");
      await user.click(page.getByTestId("login-field-show"));
      expect(page.getByText("a1b2-c3d4-e5f6")).toBeInTheDocument();
      act(() => {
        vi.advanceTimersByTime(121_000);
      });
      expect(page.queryByTestId("login-notes")).toBeNull();
      expect(page.queryByText("a1b2-c3d4-e5f6")).toBeNull();
      expect(lastCall(backend, "secret_view_closed")).toBeDefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it("settles a conflict with the other value", async () => {
    const { user, backend } = phone();
    await openLogins(user);
    await user.click(card("V2EX"));
    const conflict = within(await screen.findByTestId("login-conflict"));
    await user.click(conflict.getByTestId("login-conflict-show"));
    expect(conflict.getByText("v2ex-Other#99")).toBeInTheDocument();
    await user.click(conflict.getByTestId("login-conflict-take"));
    expect(lastCall(backend, "item_update")).toMatchObject({
      patch: { settle: [{ field: "password", take: "v2-1" }] },
    });
    await waitFor(() => expect(screen.queryByTestId("login-conflict")).toBeNull());
  });

  it("shows the password on a page of its own, at once or after the proof (boards 28, 29)", async () => {
    const { user, backend } = phone();
    await openLogins(user);
    await user.click(card("GitHub"));
    await user.click(await screen.findByTestId("login-reveal"));
    const shown = within(await screen.findByTestId("login-revealed"));
    expect(shown.getByTestId("login-revealed-password")).toHaveTextContent("x7#Kq9!vLm2@Rt4z");
    expect(shown.getByText(/^16 个字符/)).toBeInTheDocument();
    expect(shown.getByTestId("login-reveal-countdown")).toHaveTextContent("密码将在 120 秒后隐藏");
    await user.click(shown.getByTestId("login-reveal-copy"));
    expect(lastCall(backend, "item_copy")).toMatchObject({ part: { type: "password" } });
    await user.click(shown.getByRole("button", { name: "完成" }));
    expect(await screen.findByTestId("page-login")).toBeInTheDocument();
    expect(lastCall(backend, "secret_view_closed")).toBeDefined();
    // A 「填充前验证」 login asks first.
    act(() => history.back());
    await screen.findByTestId("page-logins");
    const bank = card("招商银行").dataset.item;
    await user.click(card("招商银行"));
    await user.click(await screen.findByTestId("login-reveal"));
    const prove = within(await screen.findByTestId("login-prove"));
    expect(backend.calls.some((c) => c.command === "item_reveal" && c.id === bank)).toBe(false);
    await user.type(prove.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await screen.findByTestId("login-revealed-password")).toHaveTextContent("Cmb#880613");
  });
});

describe("a login's form on the phone", () => {
  it("adds a login with a generated password, then shows it (boards 26, 27)", async () => {
    const { user, backend } = phone();
    await openLogins(user);
    await user.click(screen.getByTestId("dock-new-login"));
    await screen.findByTestId("page-login-edit");
    expect(screen.getByTestId("login-save")).toBeDisabled();
    await user.type(screen.getByTestId("login-title"), "Example");
    await user.type(screen.getByTestId("login-username"), "me");
    const site = screen.getByRole("textbox", { name: "网站" });
    await user.type(site, "not a url");
    await user.click(screen.getByTestId("login-save"));
    expect(screen.getByTestId("login-errors")).toHaveTextContent("有 1 处需要修改");
    expect(site).toHaveFocus();
    await user.clear(site);
    await user.type(site, "example.com");
    await user.click(screen.getByTestId("login-matching"));
    await user.click(
      within(await screen.findByTestId("matching-sheet")).getByRole("option", { name: /^主机/ }),
    );
    await waitFor(() => expect(screen.queryByTestId("matching-sheet")).toBeNull());
    expect(screen.getByTestId("login-matching")).toHaveTextContent("匹配：主机");
    await user.click(screen.getByTestId("login-code-link"));
    await user.click(
      within(await screen.findByTestId("code-sheet")).getAllByTestId("code-sheet-option")[0] ??
        document.body,
    );
    await waitFor(() => expect(screen.getByTestId("login-code-card")).toBeInTheDocument());
    await user.click(screen.getByTestId("login-generate"));
    const generator = within(await screen.findByTestId("generator-sheet"));
    await waitFor(() =>
      expect(generator.getByTestId("generator-password").textContent).not.toBe("—"),
    );
    expect(lastCall(backend, "generator_generate")).toMatchObject({ domain: "example.com" });
    await user.click(generator.getByTestId("generator-use"));
    await waitFor(() => expect(screen.queryByTestId("generator-sheet")).toBeNull());
    await user.click(
      within(screen.getByTestId("login-protection")).getByRole("radio", { name: /填充前验证/ }),
    );
    await user.click(screen.getByTestId("login-save"));
    expect(lastCall(backend, "item_add")).toMatchObject({
      draft: {
        title: "Example",
        username: "me",
        password: { type: "generated" },
        urls: [{ url: "example.com", matching: "host" }],
        protection: "confirm",
        totp_entry: expect.any(String),
      },
    });
    const page = within(await screen.findByTestId("page-login"));
    expect(page.getByRole("heading", { level: 2, name: "Example" })).toBeInTheDocument();
  });

  it("edits what changed, keeps the secrets it did not show, and deletes back to the list", async () => {
    const { user, backend } = phone();
    await openLogins(user);
    await user.click(card("GitHub"));
    await user.click(await screen.findByTestId("login-edit"));
    await screen.findByTestId("page-login-edit");
    expect(screen.getByTestId("login-password")).toHaveAttribute("placeholder", "（不变）");
    const title = screen.getByTestId("login-title");
    await user.clear(title);
    await user.type(title, "GitHub 公司");
    await user.click(screen.getByTestId("login-favorite").querySelector("button") ?? document.body);
    await user.click(screen.getByTestId("login-save"));
    const patch = lastCall(backend, "item_update");
    expect(patch).toMatchObject({ patch: { title: "GitHub 公司", favorite: false } });
    expect(patch).not.toHaveProperty("patch.password");
    expect(patch).not.toHaveProperty("patch.notes");
    expect(await screen.findByTestId("page-login")).toBeInTheDocument();
    await user.click(screen.getByTestId("login-edit"));
    await user.click(await screen.findByTestId("login-delete"));
    await user.click(
      within(await screen.findByRole("dialog", { name: "删除登录项？" })).getByTestId(
        "login-delete-confirm",
      ),
    );
    expect(await screen.findByTestId("page-logins")).toBeInTheDocument();
    expect(names()).not.toContain("GitHub 公司");
  });

  it("generates and copies from the list's own 生成密码", async () => {
    const { user, backend } = phone();
    const page = await openLogins(user);
    await user.click(page.getByTestId("logins-generate"));
    const generator = within(await screen.findByTestId("generator-sheet"));
    await waitFor(() =>
      expect(generator.getByTestId("generator-password").textContent).not.toBe("—"),
    );
    await user.click(generator.getByRole("radio", { name: "口令" }));
    await waitFor(() =>
      expect(lastCall(backend, "generator_generate")).toMatchObject({
        options: { kind: "passphrase" },
      }),
    );
    await user.click(generator.getByTestId("generator-copy"));
    expect(lastCall(backend, "generator_copy")).toBeDefined();
    expect(lastCall(backend, "settings_set")).toMatchObject({
      settings: { generator: { kind: "passphrase" } },
    });
  });
});
