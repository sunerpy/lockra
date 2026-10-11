import { MockBackend, sampleEntries } from "@lockra/shared/mock";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

const SCANNED = "otpauth://totp/Scanned:me@example.com?secret=MFRGGZDF&issuer=Scanned";

function names(): string[] {
  return screen
    .getAllByTestId("entry-row")
    .map((row) => row.querySelector(".truncate")?.textContent ?? "");
}

afterEach(async () => {
  cleanup();
  await waitFor(() => {
    if (depthOf(history.state) !== 0) throw new Error("the history still holds pages");
  });
});

describe("the codes screen (board A2)", () => {
  it("has a large title with the vault's state, and add and lock beside it", async () => {
    const { user } = renderApp();
    await ready();
    expect(screen.getByRole("heading", { level: 1, name: "验证码" })).toHaveClass("text-[30px]");
    expect(screen.getByTestId("codes-subtitle")).toHaveTextContent("已解锁 · 8 个账号");
    await user.click(screen.getByRole("button", { name: "添加账号" }));
    expect(await screen.findByTestId("page-add")).toBeInTheDocument();
    history.back();
    await user.click(await screen.findByRole("button", { name: "锁定" }));
    expect(await screen.findByTestId("page-unlock")).toBeInTheDocument();
  });

  it("shows the account copied last in its own card, not while searching", async () => {
    const { user, backend } = renderApp();
    await ready();
    const recent = screen.getByTestId("recent-card");
    expect(recent).toHaveTextContent("AWS");
    expect(recent).toHaveTextContent("轻点复制");
    await user.click(recent);
    expect(backend.calls.at(-1)).toMatchObject({ command: "entry_copy" });
    await user.type(screen.getByTestId("codes-search"), "git");
    expect(screen.queryByTestId("recent-card")).toBeNull();
    expect(names()).toEqual(["GitHub"]);
  });

  it("filters with chips that scroll sideways, and keeps reorder beside them", async () => {
    const { user } = renderApp();
    await ready();
    const chips = screen.getByTestId("codes-chips");
    expect(chips).toHaveClass("overflow-x-auto");
    await user.click(within(chips).getByRole("radio", { name: "收藏 1" }));
    expect(names()).toEqual(["GitHub"]);
    await user.click(within(chips).getByRole("radio", { name: "工作 3" }));
    expect(names()).toEqual(["GitHub", "AWS", "Cloudflare"]);
    await user.click(screen.getByTestId("codes-reorder"));
    expect(screen.getByTestId("codes-reordering")).toBeInTheDocument();
  });

  it("draws each row as a card of its own, its ⋯ always there", async () => {
    renderApp();
    await ready();
    const [row] = screen.getAllByTestId("entry-row");
    expect(row).toHaveClass("rounded-14", "bg-surface", "panel");
    const more = within(row as HTMLElement).getByTestId("row-more");
    expect(within(row as HTMLElement).getByTestId("row-actions")).not.toHaveClass(
      "pointer-fine:opacity-0",
    );
    expect(more).toBeInTheDocument();
  });

  it("scans and opens the settings from the dock", async () => {
    const { user } = renderApp({ mock: { scan: SCANNED } });
    await ready();
    const dock = within(screen.getByRole("navigation", { name: "主导航" }));
    expect(dock.getByTestId("dock-codes")).toHaveAttribute("aria-current", "page");
    // jsdom scrolls nothing: the call is what counts.
    const scrollTo = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollTo", {
      value: scrollTo,
      configurable: true,
    });
    await user.click(dock.getByTestId("dock-codes"));
    expect(scrollTo).toHaveBeenCalledWith({ top: 0, behavior: "smooth" });
    Reflect.deleteProperty(HTMLElement.prototype, "scrollTo");
    await user.click(dock.getByRole("button", { name: "扫码" }));
    expect(await screen.findByTestId("page-preview")).toBeInTheDocument();
    const [found] = screen.getAllByTestId("candidate");
    expect(within(found as HTMLElement).getByText("Scanned")).toBeInTheDocument();
    history.back();
    await user.click(await screen.findByTestId("dock-settings"));
    expect(await screen.findByTestId("page-settings")).toBeInTheDocument();
  });

  it("shows a filter as one flat list, and 全部 in sections", async () => {
    const { user } = renderApp();
    await ready();
    const headings = () => screen.queryAllByTestId("codes-group-toggle").map((h) => h.textContent);
    expect(headings()).toEqual(["工作3", "未分组5"]);
    for (const chip of ["收藏 1", "工作 3", "未分组 5"]) {
      await user.click(screen.getByRole("radio", { name: chip }));
      expect(headings()).toEqual([]);
    }
  });

  it("opens on the pinned group, first of the chips; in 全部 收藏 gathers the pinned accounts", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      settings: { locale: "zh-cn" },
      pinnedGroup: { type: "favorites" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const chips = within(screen.getByTestId("codes-chips")).getAllByRole("radio");
    expect(chips[0]).toHaveAttribute("aria-label", "收藏 1");
    expect(chips[0]).toHaveAttribute("aria-checked", "true");
    expect(names()).toEqual(["GitHub"]);
    await user.click(screen.getByRole("radio", { name: "全部 8" }));
    expect(screen.getByTestId("codes-pinned-heading")).toHaveTextContent("收藏1");
    const work = screen.getByRole("region", { name: "工作" });
    expect(within(work).queryByText("GitHub")).toBeNull();
  });

  it("pins a group in Settings, and the codes open on it", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("dock-settings"));
    await screen.findByTestId("page-settings");
    await user.selectOptions(screen.getByRole("combobox", { name: "置顶分组" }), "工作");
    expect(backend.calls.at(-1)).toEqual({
      command: "view_pin_group",
      pinned: { type: "group", name: "工作" },
    });
    history.back();
    await screen.findByTestId("codes-dock");
    expect(screen.getByRole("radio", { name: "工作 3" })).toHaveAttribute("aria-checked", "true");
    expect(names()).toEqual(["GitHub", "AWS", "Cloudflare"]);
  });

  it("moves 未分组 like a group while reordering", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("codes-reorder"));
    screen.getByRole("button", { name: "移动「未分组」：拖动，或按上下方向键" }).focus();
    await user.keyboard("{ArrowUp}");
    expect(backend.calls.filter((c) => c.command === "view_order_groups").at(-1)).toEqual({
      command: "view_order_groups",
      groups: ["", "工作"],
    });
  });
});
