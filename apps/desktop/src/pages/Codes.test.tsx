import { MockBackend, mockEntry, sampleEntries } from "@lockra/shared/mock";
import { act, fireEvent, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";
import { filterEntries, sortEntries } from "./Codes";

function names(): string[] {
  return screen
    .getAllByTestId("entry-row")
    .map((row) => row.querySelector(".truncate")?.textContent ?? "");
}

describe("sortEntries / filterEntries", () => {
  const entries = [
    mockEntry("beta", "b", { at: 3, last_used_at_ms: 10 }),
    mockEntry("Alpha", "a", { at: 1 }),
    mockEntry("", "carol", { at: 2, favorite: true, group: "home", last_used_at_ms: 20 }),
  ].map((e) => e.view);

  it("puts favourites first, then the chosen order", () => {
    expect(sortEntries(entries, "name").map((e) => e.issuer || e.account)).toEqual([
      "carol",
      "Alpha",
      "beta",
    ]);
    expect(sortEntries(entries, "added").map((e) => e.issuer || e.account)).toEqual([
      "carol",
      "beta",
      "Alpha",
    ]);
    expect(sortEntries(entries, "recent").map((e) => e.issuer || e.account)).toEqual([
      "carol",
      "beta",
      "Alpha",
    ]);
  });

  it("matches issuer, account and group, case-insensitively", () => {
    expect(filterEntries(entries, "ALP", "")).toHaveLength(1);
    expect(filterEntries(entries, "home", "")).toHaveLength(1);
    expect(filterEntries(entries, "", "home")).toHaveLength(1);
    expect(filterEntries(entries, "  ", "")).toHaveLength(3);
  });
});

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
