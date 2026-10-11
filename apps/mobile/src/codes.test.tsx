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
});
