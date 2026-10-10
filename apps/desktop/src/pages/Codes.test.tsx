import { MockBackend, mockEntry, sampleEntries } from "@lockra/shared/mock";
import { act, fireEvent, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";

function names(): string[] {
  return screen
    .getAllByTestId("entry-row")
    .map((row) => row.querySelector(".truncate")?.textContent ?? "");
}

describe("Codes", () => {
  it("lists the accounts with their codes, favourites first", async () => {
    renderApp();
    await ready();
    expect(names()[0]).toBe("GitHub");
    expect(names()).toHaveLength(sampleEntries().length);
    expect(await screen.findAllByTestId("otp-code")).not.toHaveLength(0);
    expect(screen.getByTestId("codes-status")).toHaveTextContent("8 个账号 · 还没有备份");
  });

  it("copies a code on click and Enter, and says when the clipboard is cleared", async () => {
    const { user, backend } = renderApp();
    await ready();
    const github = screen.getAllByTestId("entry-row")[0];
    if (!github) throw new Error("no row");
    await user.click(github);
    expect(await screen.findByText("已复制 · 30 秒后清空剪贴板")).toBeInTheDocument();
    github.focus();
    await user.keyboard("{Enter}");
    expect(backend.calls.filter((c) => c.command === "entry_copy")).toHaveLength(2);
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

  it("filters by group and saves the order", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.selectOptions(screen.getByTestId("codes-group"), "工作");
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
    const { backend } = renderApp();
    await ready();
    await act(async () => {
      await backend.saveBackup();
    });
    expect(screen.getByTestId("codes-status")).toHaveTextContent("上次备份 刚刚");
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

  it("folds and unfolds every section at once", async () => {
    const { user, backend } = renderApp();
    await ready();
    const expandAll = screen.getByRole("button", { name: "全部展开" });
    expect(expandAll).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "全部折叠" }));
    expect(backend.calls.at(-1)).toEqual({ command: "view_collapse_groups", groups: ["工作", ""] });
    expect(screen.queryAllByTestId("entry-row")).toHaveLength(0);
    expect(screen.getByRole("button", { name: "全部折叠" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "全部展开" }));
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
    const toggle = screen.getByRole("button", { name: "按分组显示" });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    await user.click(toggle);
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { group_codes: false },
    });
    expect(screen.queryAllByTestId("codes-group-toggle")).toHaveLength(0);
    expect(screen.queryByRole("button", { name: "全部折叠" })).toBeNull();
    expect(names()).toHaveLength(8);
  });

  it("has no sections when no account has a group", async () => {
    const backend = new MockBackend({
      entries: [mockEntry("Solo", "me")],
      settings: { locale: "zh-cn" },
    });
    renderApp({ backend });
    await ready();
    expect(screen.queryAllByTestId("codes-group-toggle")).toHaveLength(0);
    expect(screen.queryByRole("button", { name: "按分组显示" })).toBeNull();
    expect(names()).toEqual(["Solo"]);
  });
});

describe("Codes · row actions", () => {
  it("pins and edits from the buttons beside each row's menu", async () => {
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
    await user.click(within(row).getByRole("button", { name: "编辑…" }));
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
    await user.click(screen.getByRole("button", { name: "选择" }));
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
    await user.click(screen.getByRole("button", { name: "选择" }));
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
    await user.click(screen.getByRole("button", { name: "选择" }));
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
    await user.click(screen.getByRole("button", { name: "选择" }));
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
