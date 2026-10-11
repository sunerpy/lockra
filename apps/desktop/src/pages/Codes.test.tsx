import { MockBackend, mockEntry, mockSyncSpace, sampleEntries } from "@lockra/shared/mock";
import { act, fireEvent, screen, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { ready, renderApp } from "../test/render";

function names(): string[] {
  return screen
    .getAllByTestId("entry-row")
    .map((row) => row.querySelector(".truncate")?.textContent ?? "");
}

/** Picks one of the list's tools from its ⋯ menu. */
async function tool(user: UserEvent, name: string) {
  await user.click(screen.getByTestId("codes-more"));
  await user.click(screen.getByRole("menuitem", { name }));
}

/** The items of the list's ⋯ menu, opened and closed again. */
async function tools(user: UserEvent): Promise<string[]> {
  await user.click(screen.getByTestId("codes-more"));
  const menu = screen.getByRole("menu", { name: "列表选项" });
  const items = [...menu.querySelectorAll('[role^="menuitem"]')].map(
    (item) =>
      `${item.textContent ?? ""}${item.getAttribute("aria-checked") === "true" ? " ✓" : ""}`,
  );
  await user.keyboard("{Escape}");
  return items;
}

describe("Codes", () => {
  it("lists the accounts with their codes, favourites first", async () => {
    renderApp();
    await ready();
    expect(names()[0]).toBe("GitHub");
    expect(names()).toHaveLength(sampleEntries().length);
    expect(await screen.findAllByTestId("otp-code")).not.toHaveLength(0);
  });

  it("starts with its own header: the facts, the search and the add menu (board A1)", async () => {
    renderApp();
    await ready();
    const header = within(screen.getByTestId("page-header"));
    expect(header.getByRole("heading", { level: 1, name: "验证码" })).toBeInTheDocument();
    expect(screen.getByTestId("page-subtitle")).toHaveTextContent("8 个账号 · 1 个分组");
    expect(header.getByTestId("codes-search")).toBeInTheDocument();
    expect(header.getByTestId("add-menu")).toHaveAttribute("title", "添加账号 · Ctrl N");
  });

  it("shows the account used last, with the vault's state beside it", async () => {
    const { user, backend } = renderApp();
    await ready();
    // AWS was copied last of the samples.
    expect(screen.getByTestId("recent-card")).toHaveTextContent("AWS");
    const vault = screen.getByTestId("codes-vault");
    expect(vault).toHaveTextContent("已解锁 · 5 分钟后自动锁定");
    expect(vault).toHaveTextContent("还没有备份");
    expect(vault).toHaveTextContent("未开启同步");
    // Copying from the list moves the card to that account; the card copies too.
    const github = screen.getAllByTestId("entry-row")[0];
    if (!github) throw new Error("no row");
    await user.click(github);
    expect(screen.getByTestId("recent-card")).toHaveTextContent("GitHub");
    await user.click(screen.getByTestId("recent-card"));
    expect(backend.calls.filter((c) => c.command === "entry_copy")).toHaveLength(2);
    // Searching is about the list: the cards step aside.
    await user.type(screen.getByTestId("codes-search"), "a");
    expect(screen.queryByTestId("codes-hero")).toBeNull();
  });

  it("names the devices of the sync space on the vault card", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      settings: { locale: "zh-cn" },
      sync: mockSyncSpace(),
    });
    renderApp({ backend });
    await ready();
    const vault = screen.getByTestId("codes-vault");
    expect(vault).toHaveTextContent("已同步 · 2 台设备");
    expect(screen.getByTestId("codes-devices")).toHaveTextContent("本机 · Pixel 8");
    expect(screen.getByTestId("page-subtitle")).toHaveTextContent(
      "8 个账号 · 1 个分组 · 已全部同步",
    );
  });

  it("copies a code on click and Enter, and says when the clipboard is cleared", async () => {
    const { user, backend } = renderApp();
    await ready();
    const github = screen.getAllByTestId("entry-row")[0];
    if (!github) throw new Error("no row");
    await user.click(github);
    expect(
      await screen.findByText("已复制 GitHub 的验证码 · 30 秒后清空剪贴板"),
    ).toBeInTheDocument();
    github.focus();
    await user.keyboard("{Enter}");
    expect(backend.calls.filter((c) => c.command === "entry_copy")).toHaveLength(2);
  });

  it("shows on the copied row that it was copied, for a moment", async () => {
    const { user } = renderApp();
    await ready();
    const [first, second] = screen.getAllByTestId("entry-row");
    if (!first || !second) throw new Error("no rows");
    await user.click(first);
    expect(first).toHaveAttribute("data-copied", "true");
    expect(within(first).getByTestId("row-copied")).toHaveTextContent(/^已复制$/);
    await user.click(second);
    expect(first).not.toHaveAttribute("data-copied");
    expect(second).toHaveAttribute("data-copied", "true");
  });

  it("searches, copies the first match with Enter and says when nothing matches", async () => {
    const { user, backend } = renderApp();
    await ready();
    const search = screen.getByTestId("codes-search");
    await user.type(search, "cloud");
    expect(names()).toEqual(["Cloudflare"]);
    await user.keyboard("{Enter}");
    const cloudflare = backend.calls.find((c) => c.command === "entry_copy");
    expect(cloudflare).toBeDefined();
    await user.clear(search);
    await user.type(search, "zzz");
    expect(screen.getByText("没有匹配「zzz」的账号")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(search).toHaveValue("");
    expect(names()).toHaveLength(8);
  });

  it("moves between rows with the arrow keys", async () => {
    const { user } = renderApp();
    await ready();
    const search = screen.getByTestId("codes-search");
    search.focus();
    await user.keyboard("{ArrowDown}");
    const rows = screen.getAllByTestId("entry-row");
    expect(rows[0]).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(rows[1]).toHaveFocus();
    await user.keyboard("{End}");
    expect(rows.at(-1)).toHaveFocus();
    await user.keyboard("{Home}");
    expect(rows[0]).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(search).toHaveFocus();
  });

  it("filters with the chips: everything, the pinned ones, a group, those in none", async () => {
    const { user } = renderApp();
    await ready();
    const chips = within(screen.getByTestId("codes-chips"));
    expect(chips.getAllByRole("radio").map((chip) => chip.getAttribute("aria-label"))).toEqual([
      "全部 8",
      "收藏 1",
      "工作 3",
      "未分组 5",
    ]);
    await user.click(chips.getByRole("radio", { name: "收藏 1" }));
    expect(names()).toEqual(["GitHub"]);
    await user.click(chips.getByRole("radio", { name: "未分组 5" }));
    expect(names()).toHaveLength(5);
    expect(names()).not.toContain("GitHub");
    await user.click(chips.getByRole("radio", { name: "全部 8" }));
    expect(names()).toHaveLength(8);
  });

  it("filters by group and saves the order", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByRole("radio", { name: "工作 3" }));
    expect(names()).toEqual(["GitHub", "AWS", "Cloudflare"]);
    await user.selectOptions(screen.getByTestId("codes-sort"), "recent");
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { sort: "recent" },
    });
    expect(names().slice(0, 2)).toEqual(["GitHub", "AWS"]);
  });

  it("generates the next HOTP code", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByRole("button", { name: "生成下一个" }));
    expect(backend.calls).toContainEqual(expect.objectContaining({ command: "entry_hotp_next" }));
  });

  it("offers favourite, edit, reveal and delete in each row's menu", async () => {
    const { user, backend } = renderApp();
    await ready();
    const menu = () => screen.getAllByTestId("row-menu")[0];
    const open = async () => {
      const trigger = menu();
      if (!trigger) throw new Error("no menu");
      await user.click(trigger);
    };
    await open();
    await user.click(screen.getByRole("menuitem", { name: "取消收藏" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "entry_update",
      patch: { favorite: false },
    });
    expect(names()[0]).not.toBe("GitHub");
    for (const [item, dialog] of [
      ["编辑…", "编辑账号"],
      ["显示密钥…", "显示密钥"],
    ] as const) {
      await open();
      await user.click(screen.getByRole("menuitem", { name: item }));
      expect(screen.getByRole("dialog", { name: dialog })).toBeInTheDocument();
      await user.keyboard("{Escape}");
    }
    await open();
    await user.click(screen.getByRole("menuitem", { name: "删除…" }));
    expect(screen.getByRole("dialog", { name: /^删除「/ })).toBeInTheDocument();
  });

  it("hides codes until hover when asked to", async () => {
    renderApp({ mock: { settings: { hide_codes: true } } });
    await ready();
    const codes = await screen.findAllByTestId("otp-code");
    expect(codes.some((c) => c.dataset.masked === "true")).toBe(true);
  });

  it("starts empty with the ways in", async () => {
    const { user } = renderApp({ mock: { entries: [], phase: "unlocked" } });
    await ready();
    expect(screen.getByText("还没有账号")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "手动添加" }));
    expect(screen.getByRole("dialog", { name: "手动添加账号" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.click(
      within(screen.getByTestId("page-codes")).getByRole("button", { name: "导入" }),
    );
    expect(await screen.findByTestId("page-import")).toBeInTheDocument();
  });

  it("adds from the menu: by hand, a link, the clipboard or an image", async () => {
    const backend = new MockBackend({ entries: sampleEntries(), settings: { locale: "zh-cn" } });
    const { user } = renderApp({ backend });
    await ready();
    const add = async (item: string) => {
      await user.click(screen.getByTestId("add-menu"));
      await user.click(screen.getByRole("menuitem", { name: item }));
    };
    await add("手动输入…");
    expect(screen.getByRole("dialog", { name: "手动添加账号" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await add("粘贴 otpauth 链接…");
    expect(screen.getByRole("dialog", { name: "粘贴 otpauth 链接" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await add("从剪贴板导入");
    expect(await screen.findByText("剪贴板里没有二维码或 otpauth 链接")).toBeInTheDocument();
    backend.setClipboard("otpauth://totp/Example:me?secret=JBSWY3DPEHPK3PXP");
    await add("从剪贴板导入");
    expect(await screen.findByTestId("page-import")).toBeInTheDocument();
    expect(within(screen.getByTestId("import-preview")).getByText("Example")).toBeInTheDocument();
  });

  it("shows the time since the last backup", async () => {
    // Regression (2026-10-11): the page's clock was in seconds and every backup read 「刚刚」.
    const backend = new MockBackend({
      entries: sampleEntries(),
      settings: { locale: "zh-cn" },
      now: () => Date.now() - 3.5 * 3_600_000,
    });
    renderApp({ backend });
    await ready();
    await act(async () => {
      await backend.saveBackup();
    });
    expect(screen.getByTestId("codes-vault")).toHaveTextContent("上次备份 3 小时前");
  });
});

describe("Codes · groups", () => {
  const headers = () =>
    screen
      .getAllByTestId("codes-group-toggle")
      .map((h) => [h.textContent, h.getAttribute("aria-expanded")]);

  it("shows the accounts in sections that fold one by one", async () => {
    const { user, backend } = renderApp();
    await ready();
    expect(headers()).toEqual([
      ["工作3", "true"],
      ["未分组5", "true"],
    ]);
    expect(names().slice(0, 3)).toEqual(["GitHub", "AWS", "Cloudflare"]);
    await user.click(screen.getByRole("button", { name: /^工作/ }));
    expect(backend.calls.at(-1)).toEqual({ command: "view_collapse_groups", groups: ["工作"] });
    expect(headers()[0]).toEqual(["工作3", "false"]);
    expect(names()).not.toContain("GitHub");
    expect(names()).toHaveLength(5);
    // A search shows what it finds, folded or not.
    await user.type(screen.getByTestId("codes-search"), "aws");
    expect(names()).toEqual(["AWS"]);
    await user.clear(screen.getByTestId("codes-search"));
    expect(names()).not.toContain("AWS");
  });

  it("folds and unfolds every section at once, from the list's menu", async () => {
    const { user, backend } = renderApp();
    await ready();
    // Only what can happen: nothing is folded yet.
    expect(await tools(user)).toEqual(["选择", "按分组显示 ✓", "全部折叠"]);
    await tool(user, "全部折叠");
    expect(backend.calls.at(-1)).toEqual({ command: "view_collapse_groups", groups: ["工作", ""] });
    expect(screen.queryAllByTestId("entry-row")).toHaveLength(0);
    expect(await tools(user)).toEqual(["选择", "按分组显示 ✓", "全部展开"]);
    await tool(user, "全部展开");
    expect(backend.calls.at(-1)).toEqual({ command: "view_collapse_groups", groups: [] });
    expect(names()).toHaveLength(8);
  });

  it("keeps the folded sections from the vault, and turns the sections off", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      collapsedGroups: [""],
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    expect(headers()[1]).toEqual(["未分组5", "false"]);
    expect(names()).toEqual(["GitHub", "AWS", "Cloudflare"]);
    await user.click(screen.getByTestId("codes-more"));
    const toggle = screen.getByRole("menuitemcheckbox", { name: "按分组显示" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await user.click(toggle);
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { group_codes: false },
    });
    expect(screen.queryAllByTestId("codes-group-toggle")).toHaveLength(0);
    expect(await tools(user)).toEqual(["选择", "按分组显示"]);
    expect(names()).toHaveLength(8);
  });

  it("has no sections when no account has a group", async () => {
    const backend = new MockBackend({
      entries: [mockEntry("Solo", "me")],
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    expect(screen.queryAllByTestId("codes-group-toggle")).toHaveLength(0);
    expect(await tools(user)).toEqual(["选择"]);
    // One account: nothing to filter, nothing to reorder.
    expect(screen.queryByTestId("codes-chips")).toBeNull();
    expect(screen.queryByTestId("codes-reorder")).toBeNull();
    expect(names()).toEqual(["Solo"]);
  });
});

describe("Codes · row actions", () => {
  it("pins from the button beside each row's menu, and edits from the menu", async () => {
    const { user, backend } = renderApp();
    await ready();
    const first = screen.getAllByTestId("entry-row")[0];
    if (!first) throw new Error("no row");
    const pin = within(first).getByRole("button", { name: "收藏" });
    expect(pin).toHaveAttribute("aria-pressed", "true");
    await user.click(pin);
    expect(backend.calls.at(-1)).toMatchObject({
      command: "entry_update",
      patch: { favorite: false },
    });
    const row = screen.getAllByTestId("entry-row")[0];
    if (!row) throw new Error("no row");
    // Board A1: the row has the pin and the menu, no edit button of its own.
    expect(within(row).queryByRole("button", { name: "编辑…" })).toBeNull();
    await user.click(within(row).getByTestId("row-menu"));
    await user.click(screen.getByRole("menuitem", { name: "编辑…" }));
    expect(screen.getByRole("dialog", { name: "编辑账号" })).toBeInTheDocument();
    expect(backend.calls.some((c) => c.command === "entry_copy")).toBe(false);
  });

  it("opens the row's menu where it was right-clicked, and beside the row from the keyboard", async () => {
    const { user, backend } = renderApp();
    await ready();
    const row = screen.getAllByTestId("entry-row")[1];
    if (!row) throw new Error("no row");
    fireEvent.contextMenu(row, { clientX: 120, clientY: 80 });
    const menu = screen.getByRole("menu", { name: "账号操作" });
    expect(menu).toHaveStyle({ left: "120px", top: "80px" });
    expect(within(menu).getByRole("menuitem", { name: "收藏" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu", { name: "账号操作" })).toBeNull();
    expect(row).toHaveFocus();
    // The context-menu key, or Shift F10, on a focused row.
    fireEvent.keyDown(row, { key: "ContextMenu" });
    await user.click(screen.getByRole("menuitem", { name: "编辑…" }));
    expect(screen.getByRole("dialog", { name: "编辑账号" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    fireEvent.keyDown(screen.getAllByTestId("entry-row")[1] as HTMLElement, {
      key: "F10",
      shiftKey: true,
    });
    await user.click(screen.getByRole("menuitem", { name: "收藏" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "entry_update",
      patch: { favorite: true },
    });
  });
});

describe("Codes · selecting several accounts", () => {
  const rowsOf = (section: string) =>
    within(screen.getByRole("region", { name: section })).getAllByTestId("entry-row");
  const ticked = () =>
    screen
      .getAllByTestId("entry-row")
      .filter((row) => row.getAttribute("aria-checked") === "true")
      .map((row) => row.dataset.entry);

  it("ticks rows instead of copying them, a section and everything at once", async () => {
    const { user, backend } = renderApp();
    await ready();
    await tool(user, "选择");
    const bar = within(screen.getByTestId("codes-selection"));
    expect(bar.getByText("已选择 0 个账号")).toBeInTheDocument();
    expect(bar.getByRole("button", { name: "移到分组…" })).toBeDisabled();
    expect(screen.queryAllByTestId("row-favorite")).toHaveLength(0);
    const [loose] = rowsOf("未分组");
    if (!loose) throw new Error("no row");
    await user.click(loose);
    expect(loose).toHaveAttribute("aria-checked", "true");
    await user.click(loose);
    expect(loose).toHaveAttribute("aria-checked", "false");
    loose.focus();
    await user.keyboard(" ");
    expect(bar.getByText("已选择 1 个账号")).toBeInTheDocument();
    expect(backend.calls.some((c) => c.command === "entry_copy")).toBe(false);
    // A section's box ticks its accounts; a section partly ticked says so.
    const work = screen.getByRole("checkbox", { name: "选择「工作」中的全部账号" });
    await user.click(work);
    expect(work).toBeChecked();
    expect(ticked()).toHaveLength(4);
    const none = screen.getByRole("checkbox", { name: "选择「未分组」中的全部账号" });
    expect((none as HTMLInputElement).indeterminate).toBe(true);
    await user.click(bar.getByRole("button", { name: "全选" }));
    expect(ticked()).toHaveLength(8);
    await user.click(bar.getByRole("button", { name: "全不选" }));
    expect(ticked()).toHaveLength(0);
  });

  it("moves the ticked accounts to a group, or out of theirs, in one command each", async () => {
    const { user, backend } = renderApp();
    await ready();
    await tool(user, "选择");
    await user.click(screen.getByRole("checkbox", { name: "选择「工作」中的全部账号" }));
    const ids = rowsOf("工作").map((row) => row.dataset.entry);
    await user.click(screen.getByRole("button", { name: "移到分组…" }));
    const dialog = within(screen.getByRole("dialog", { name: "移到分组" }));
    expect(dialog.getByText("将 3 个账号移到下面的分组。")).toBeInTheDocument();
    await user.type(dialog.getByLabelText("分组"), "个人{Enter}");
    expect(backend.calls.at(-1)).toEqual({ command: "entries_set_group", ids, group: "个人" });
    expect(await screen.findByText("已将 3 个账号移到「个人」")).toBeInTheDocument();
    // Done: out of selection, the accounts in their new section.
    expect(screen.queryByTestId("codes-selection")).toBeNull();
    expect(rowsOf("个人").map((row) => row.dataset.entry)).toEqual(ids);
    // Empty takes them out of their group.
    await tool(user, "选择");
    await user.click(screen.getByRole("checkbox", { name: "选择「个人」中的全部账号" }));
    await user.click(screen.getByRole("button", { name: "移到分组…" }));
    await user.click(screen.getByRole("button", { name: "移动" }));
    expect(backend.calls.at(-1)).toEqual({ command: "entries_set_group", ids, group: "" });
    expect(await screen.findByText("已将 3 个账号移出分组")).toBeInTheDocument();
    expect(screen.queryAllByTestId("codes-group-toggle")).toHaveLength(0);
  });

  it("starts from a row's menu with that row ticked, and Escape leaves", async () => {
    const { user } = renderApp();
    await ready();
    const row = screen.getAllByTestId("entry-row")[1];
    if (!row) throw new Error("no row");
    fireEvent.contextMenu(row, { clientX: 100, clientY: 60 });
    await user.click(screen.getByRole("menuitem", { name: "选择" }));
    expect(ticked()).toEqual([row.dataset.entry]);
    screen.getAllByTestId("entry-row")[0]?.focus();
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("codes-selection")).toBeNull();
    expect(screen.queryAllByRole("checkbox")).toHaveLength(0);
  });

  it("selects only what a search shows", async () => {
    const { user } = renderApp();
    await ready();
    await tool(user, "选择");
    await user.type(screen.getByTestId("codes-search"), "aws");
    await user.click(screen.getByRole("button", { name: "全选" }));
    expect(screen.getByText("已选择 1 个账号")).toBeInTheDocument();
  });
});

describe("Codes · reordering", () => {
  it("moves accounts and groups with their handles, keeps the order and makes it manual", async () => {
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
    expect(screen.getByTestId("codes-search")).toBeDisabled();
    expect(screen.queryByTestId("entry-row")).toBeNull();
    const handle = (name: string) =>
      screen.getByRole("button", { name: `移动「${name}」：拖动，或按上下方向键` });
    // Work above Money, then Jira above GitHub.
    handle("Work").focus();
    await user.keyboard("{ArrowUp}");
    const last = (command: string) => backend.calls.filter((c) => c.command === command).at(-1);
    expect(last("view_order_groups")).toEqual({
      command: "view_order_groups",
      groups: ["Work", "Money"],
    });
    handle("Jira").focus();
    await user.keyboard("{ArrowUp}");
    const id = async (issuer: string) =>
      (await backend.getState()).entries.find((e) => e.issuer === issuer)?.id;
    expect(last("view_order_entries")).toEqual({
      command: "view_order_entries",
      ids: [await id("Bank"), await id("Jira"), await id("GitHub")],
    });
    await user.click(screen.getByTestId("codes-reorder-done"));
    // The list as dragged: Work first, Jira before GitHub; the order is manual now.
    expect(names()).toEqual(["Jira", "GitHub", "Bank"]);
    expect(screen.getByTestId("codes-sort")).toHaveValue("manual");
  });
});

describe("Codes · cards", () => {
  const tileNames = () =>
    screen
      .getAllByTestId("entry-tile")
      .map((tile) => tile.querySelector(".truncate")?.textContent ?? "");
  const cards = () => renderApp({ mock: { settings: { codes_view: "grid" } } });

  it("switches between the list and the cards from the toolbar and keeps the choice", async () => {
    const { user, backend } = renderApp();
    await ready();
    const view = screen.getByTestId("codes-view");
    expect(within(view).getByRole("radio", { name: "列表" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await user.click(within(view).getByRole("radio", { name: "卡片" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { codes_view: "grid" },
    });
    expect(screen.queryAllByTestId("entry-row")).toHaveLength(0);
    expect(tileNames()).toHaveLength(sampleEntries().length);
    expect(tileNames().slice(0, 3)).toEqual(["GitHub", "AWS", "Cloudflare"]);
    await user.click(within(view).getByRole("radio", { name: "列表" }));
    expect(screen.queryAllByTestId("entry-tile")).toHaveLength(0);
    expect(names()).toHaveLength(sampleEntries().length);
  });

  it("copies from a card and shows on it that it was copied", async () => {
    const { user, backend } = cards();
    await ready();
    const [first, second] = screen.getAllByTestId("entry-tile");
    if (!first || !second) throw new Error("no cards");
    await user.click(first);
    expect(backend.calls.filter((c) => c.command === "entry_copy")).toHaveLength(1);
    expect(first).toHaveAttribute("data-copied", "true");
    expect(within(first).getByTestId("tile-copied")).toHaveTextContent(
      "已复制 · 30 秒后清空剪贴板",
    );
    // The next copy moves the mark.
    second.focus();
    await user.keyboard("{Enter}");
    expect(second).toHaveAttribute("data-copied", "true");
    expect(first).not.toHaveAttribute("data-copied");
  });

  it("says only 已复制 when the clipboard is kept", async () => {
    const { user } = renderApp({
      mock: { settings: { codes_view: "grid", clipboard_clear_seconds: 0 } },
    });
    await ready();
    const first = screen.getAllByTestId("entry-tile")[0];
    if (!first) throw new Error("no cards");
    await user.click(first);
    expect(within(first).getByTestId("tile-copied")).toHaveTextContent(/^已复制$/);
  });

  it("moves between cards with the arrow keys, and back to the search", async () => {
    const { user } = cards();
    await ready();
    const search = screen.getByTestId("codes-search");
    search.focus();
    await user.keyboard("{ArrowDown}");
    const tiles = screen.getAllByTestId("entry-tile");
    expect(tiles[0]).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(tiles[1]).toHaveFocus();
    await user.keyboard("{End}");
    expect(tiles.at(-1)).toHaveFocus();
    await user.keyboard("{Home}");
    expect(tiles[0]).toHaveFocus();
    // jsdom lays nothing out: every card is in the first row, so up leaves for the search.
    await user.keyboard("{ArrowUp}");
    expect(search).toHaveFocus();
  });

  it("folds sections, pins, opens the menu and generates the next HOTP code from the cards", async () => {
    const { user, backend } = cards();
    await ready();
    await user.click(screen.getByRole("button", { name: /^工作/ }));
    expect(backend.calls.at(-1)).toEqual({ command: "view_collapse_groups", groups: ["工作"] });
    expect(tileNames()).not.toContain("GitHub");
    await user.click(screen.getByRole("button", { name: /^工作/ }));
    const github = screen.getAllByTestId("entry-tile")[0];
    if (!github) throw new Error("no cards");
    await user.click(within(github).getByRole("button", { name: "收藏" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "entry_update",
      patch: { favorite: false },
    });
    const aws = screen.getAllByTestId("entry-tile")[0];
    if (!aws) throw new Error("no cards");
    fireEvent.contextMenu(aws, { clientX: 200, clientY: 120 });
    expect(screen.getByRole("menu", { name: "账号操作" })).toHaveStyle({ left: "200px" });
    await user.keyboard("{Escape}");
    await user.click(within(aws).getByTestId("row-menu"));
    expect(screen.getByRole("menuitem", { name: "编辑…" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "生成下一个" }));
    expect(backend.calls).toContainEqual(expect.objectContaining({ command: "entry_hotp_next" }));
    expect(backend.calls.some((c) => c.command === "entry_copy")).toBe(false);
  });

  it("selects and reorders in the list, and comes back to the cards", async () => {
    const { user } = cards();
    await ready();
    await tool(user, "选择");
    expect(screen.queryAllByTestId("entry-tile")).toHaveLength(0);
    expect(screen.getAllByTestId("entry-row")[0]).toHaveAttribute("role", "checkbox");
    expect(
      within(screen.getByTestId("codes-view")).getByRole("radio", { name: "卡片" }),
    ).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "完成" }));
    expect(screen.getAllByTestId("entry-tile")).toHaveLength(sampleEntries().length);
    await user.click(screen.getByTestId("codes-reorder"));
    expect(screen.queryAllByTestId("entry-tile")).toHaveLength(0);
    await user.click(screen.getByTestId("codes-reorder-done"));
    expect(screen.getAllByTestId("entry-tile")).toHaveLength(sampleEntries().length);
  });
});
