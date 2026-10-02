import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ContextMenu, Menu, type MenuSection } from "./Menu";

const SECTIONS: MenuSection[] = [
  {
    label: "内置预设",
    items: [
      { kind: "radio", id: "proofread", label: "校对", checked: false },
      { kind: "radio", id: "prompt", label: "提示词优化", checked: true },
    ],
  },
  {
    label: "自定义预设",
    items: [{ kind: "radio", id: "u1", label: "周报", checked: false, userText: true }],
  },
  { items: [{ kind: "action", id: "manage", label: "管理预设…" }] },
];

function renderMenu(onSelect = vi.fn(), onOuterKey = vi.fn()) {
  render(
    // A dialog under the menu that closes on Esc unless the menu handled it first.
    <div
      onKeyDown={(e) => {
        if (e.key === "Escape" && !e.defaultPrevented) onOuterKey();
      }}>
      <Menu
        trigger="提示词优化"
        label="AI 预设"
        triggerLabel="AI 预设：提示词优化"
        sections={SECTIONS}
        onSelect={onSelect}
        data-testid="presets"
      />
      <button type="button">elsewhere</button>
    </div>,
  );
  return {
    onSelect,
    onOuterKey,
    trigger: screen.getByRole("button", { name: "AI 预设：提示词优化" }),
  };
}

describe("Menu", () => {
  it("opens on the checked choice, moves with the arrow keys and picks with Enter", async () => {
    const user = userEvent.setup();
    const { onSelect, trigger } = renderMenu();
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    await user.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    const menu = screen.getByRole("menu", { name: "AI 预设" });
    expect(trigger).toHaveAttribute("aria-controls", menu.id);
    expect(
      within(menu)
        .getAllByRole("group")
        .map((g) => g.getAttribute("aria-label")),
    ).toEqual(["内置预设", "自定义预设", null]);
    expect(within(menu).getByRole("menuitemradio", { name: "提示词优化" })).toHaveFocus();
    expect(within(menu).getByRole("menuitemradio", { name: "提示词优化" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await user.keyboard("{ArrowDown}");
    expect(within(menu).getByRole("menuitemradio", { name: "周报" })).toHaveFocus();
    await user.keyboard("{ArrowDown}{ArrowDown}");
    // Wraps from the last row to the first.
    expect(within(menu).getByRole("menuitemradio", { name: "校对" })).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(within(menu).getByRole("menuitem", { name: "管理预设…" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(within(menu).getByRole("menuitemradio", { name: "校对" })).toHaveFocus();
    await user.keyboard("{End}{Enter}");
    expect(onSelect).toHaveBeenCalledWith("manage");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("closes on Esc without letting the dialog beneath close, and on a press elsewhere", async () => {
    const user = userEvent.setup();
    const { onSelect, onOuterKey, trigger } = renderMenu();
    trigger.focus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(onOuterKey).not.toHaveBeenCalled();
    expect(trigger).toHaveFocus();
    await user.click(trigger);
    fireEvent.pointerDown(screen.getByRole("button", { name: "elsewhere" }));
    expect(screen.queryByRole("menu")).toBeNull();
    // A second click on the trigger closes an open menu.
    await user.click(trigger);
    await user.click(trigger);
    expect(screen.queryByRole("menu")).toBeNull();
    // Tab leaves it closed; nothing was picked on the way.
    await user.click(trigger);
    await user.tab();
    expect(screen.queryByRole("menu")).toBeNull();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("picks with a click, keeps rows on one line and marks the user's own names", async () => {
    const user = userEvent.setup();
    const { onSelect, trigger } = renderMenu();
    await user.click(trigger);
    const menu = screen.getByTestId("presets-menu");
    expect(menu).toHaveClass("whitespace-nowrap", "w-max");
    expect(menu).toHaveAttribute("data-tauri-drag-region", "false");
    expect(within(menu).getByText("周报")).toHaveAttribute("data-user-text");
    expect(within(menu).getByText("校对")).not.toHaveAttribute("data-user-text");
    await user.click(within(menu).getByRole("menuitemradio", { name: "校对" }));
    expect(onSelect).toHaveBeenCalledWith("proofread");
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

const ACTIONS: MenuSection[] = [
  {
    items: [
      { kind: "action", id: "favorite", label: "收藏", icon: "star" },
      { kind: "action", id: "edit", label: "编辑…", icon: "edit" },
    ],
  },
  { items: [{ kind: "action", id: "delete", label: "删除…", icon: "trash" }] },
];

function renderContext(at: { x: number; y: number } | null) {
  const onSelect = vi.fn();
  const onClose = vi.fn();
  const result = render(
    <div>
      <button type="button">row</button>
      <button type="button">elsewhere</button>
      <ContextMenu
        at={at}
        label="账号操作"
        sections={ACTIONS}
        onSelect={onSelect}
        onClose={onClose}
        data-testid="row-context"
      />
    </div>,
  );
  return { onSelect, onClose, ...result };
}

describe("ContextMenu", () => {
  it("stays closed without a point, and opens where it is asked, focused on its first row", () => {
    const closed = renderContext(null);
    expect(screen.queryByRole("menu")).toBeNull();
    closed.unmount();
    renderContext({ x: 40, y: 60 });
    const menu = screen.getByRole("menu", { name: "账号操作" });
    expect(menu).toHaveStyle({ left: "40px", top: "60px" });
    expect(within(menu).getByRole("menuitem", { name: "收藏" })).toHaveFocus();
    expect(menu.parentElement).toBe(document.body);
  });

  it("stays inside the window when asked to open past its edges", () => {
    renderContext({ x: 5000, y: 5000 });
    const menu = screen.getByRole("menu", { name: "账号操作" });
    expect(Number.parseFloat(menu.style.left)).toBeLessThanOrEqual(window.innerWidth - 8);
    expect(Number.parseFloat(menu.style.top)).toBeLessThanOrEqual(window.innerHeight - 8);
    expect(Number.parseFloat(menu.style.left)).toBeGreaterThanOrEqual(8);
  });

  it("moves with the arrow keys, picks with Enter and closes", async () => {
    const user = userEvent.setup();
    const { onSelect, onClose } = renderContext({ x: 10, y: 10 });
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "删除…" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "收藏" })).toHaveFocus();
    await user.keyboard("{End}{Home}{ArrowDown}{Enter}");
    expect(onSelect).toHaveBeenCalledWith("edit");
    expect(onClose).toHaveBeenCalled();
  });

  it("closes on Esc with the focus back where it was, and on a press elsewhere", async () => {
    const user = userEvent.setup();
    const row = () => screen.getByRole("button", { name: "row" });
    const onClose = vi.fn();
    const tree = (at: { x: number; y: number } | null) => (
      <div>
        <button type="button">row</button>
        <ContextMenu
          at={at}
          label="账号操作"
          sections={ACTIONS}
          onSelect={vi.fn()}
          onClose={onClose}
        />
      </div>
    );
    const first = render(tree(null));
    row().focus();
    first.rerender(tree({ x: 1, y: 1 }));
    expect(screen.getByRole("menuitem", { name: "收藏" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
    expect(row()).toHaveFocus();
    first.unmount();
    const { onClose: closed } = renderContext({ x: 1, y: 1 });
    fireEvent.pointerDown(screen.getByRole("button", { name: "elsewhere" }));
    expect(closed).toHaveBeenCalledOnce();
  });
});
