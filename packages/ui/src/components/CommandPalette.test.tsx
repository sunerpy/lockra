import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type CommandItem, CommandPalette, filterCommands } from "./CommandPalette";

function items(run = vi.fn()): CommandItem[] {
  return [
    { id: "theme-light", group: "主题", label: "主题 › 明亮", hint: "当前", run },
    { id: "theme-dark", group: "主题", label: "主题 › 暗黑", run, icon: "settings" },
    { id: "dictate", group: "动作", label: "开始听写", keys: "Ctrl Alt Space", run },
    {
      id: "clear",
      group: "动作",
      label: "删除全部历史…",
      disabled: true,
      disabledHint: "历史记录已关闭",
      run,
    },
    { id: "settings", group: "导航", label: "打开设置 › 外观", keys: "Ctrl ,", run },
  ];
}

describe("CommandPalette", () => {
  it("filters by query and groups", () => {
    expect(filterCommands(items(), "暗").map((i) => i.id)).toEqual(["theme-dark"]);
    expect(filterCommands(items(), "  ")).toHaveLength(5);
    expect(filterCommands(items(), "当前").map((i) => i.id)).toEqual(["theme-light"]);
  });

  it("navigates with arrows, tab, enter; runs and closes; previews highlight", async () => {
    const user = userEvent.setup();
    const run = vi.fn();
    const onClose = vi.fn();
    const onHighlight = vi.fn();
    render(<CommandPalette open items={items(run)} onClose={onClose} onHighlight={onHighlight} />);
    const input = screen.getByRole("combobox");
    expect(input).toHaveFocus();
    expect(onHighlight).toHaveBeenLastCalledWith(expect.objectContaining({ id: "theme-light" }));
    expect(screen.getByText("5 条结果")).toBeInTheDocument();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("option", { name: /暗黑/ })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(screen.getByRole("option", { name: /打开设置/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.keyboard("{Tab}");
    expect(screen.getByRole("option", { name: /明亮/ })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{Tab}");
    expect(screen.getByRole("option", { name: /开始听写/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.keyboard("{Enter}");
    expect(run).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("highlights matches, refuses disabled items, closes on Esc and scrim", async () => {
    const user = userEvent.setup();
    const run = vi.fn();
    const onClose = vi.fn();
    render(<CommandPalette open items={items(run)} onClose={onClose} />);
    await user.type(screen.getByRole("combobox"), "历史");
    expect(screen.getByText("1 条结果")).toBeInTheDocument();
    expect(screen.getByText("历史记录已关闭")).toBeInTheDocument();
    await user.keyboard("{Enter}");
    expect(run).not.toHaveBeenCalled();
    await user.click(screen.getByRole("option", { name: /删除全部历史/ }));
    expect(run).not.toHaveBeenCalled();
    await user.clear(screen.getByRole("combobox"));
    await user.type(screen.getByRole("combobox"), "zzz");
    expect(screen.getByText("没有匹配的账号或命令")).toBeInTheDocument();
    await user.keyboard("{ArrowDown}{Enter}{Tab}");
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    await user.click(screen.getByTestId("palette-scrim"));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("regression: keys that land outside the dialog still drive it (the quick search's Esc before its focus)", async () => {
    // 2026-10-17, the desktop smoke under Xvfb: the shortcut's palette was in the page 30 ms before
    // its input took the focus, and an Esc in between reached <body> and closed nothing. A click on
    // a part of the palette that takes no focus puts it on <body> too.
    const onClose = vi.fn();
    const run = vi.fn();
    render(<CommandPalette open items={items(run)} onClose={onClose} />);
    expect(screen.getByRole("combobox")).toHaveFocus();
    const press = (key: string) =>
      act(() => {
        document.body.dispatchEvent(
          new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
        );
      });
    screen.getByRole("combobox").blur();
    expect(document.body).toHaveFocus();
    press("ArrowDown");
    expect(screen.getByRole("option", { name: /主题 › 暗黑/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("combobox")).toHaveFocus();
    screen.getByRole("combobox").blur();
    press("Enter");
    expect(run).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
    screen.getByRole("combobox").blur();
    press("Escape");
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("closes once on an Esc in its own input, and not on keys it does not use", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<CommandPalette open items={items()} onClose={onClose} />);
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    act(() => {
      document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("goes through a login's own actions with Tab; Enter or a click runs the one chosen (board 11)", async () => {
    const user = userEvent.setup();
    const ran: string[] = [];
    const login: CommandItem = {
      id: "login",
      group: "登录项",
      label: "GitHub · octocat",
      run: () => ran.push("run"),
      actions: ["复制密码", "复制用户名", "显示密码"].map((label) => ({
        id: label,
        label,
        run: () => ran.push(label),
      })),
    };
    const onClose = vi.fn();
    render(<CommandPalette open items={[login, ...items()]} onClose={onClose} />);
    const active = () =>
      screen
        .getAllByTestId("palette-action")
        .filter((a) => a.dataset.active)
        .map((a) => a.textContent);
    expect(active()).toEqual(["复制密码↵"]);
    expect(screen.getByText("↑↓ 选择 · ↵ 执行 · Tab 切换操作 · Esc 关闭")).toBeInTheDocument();
    await user.keyboard("{Tab}{Tab}");
    expect(active()).toEqual(["显示密码↵"]);
    await user.keyboard("{Tab}");
    expect(active()).toEqual(["复制密码↵"]);
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(active()).toEqual(["显示密码↵"]);
    await user.keyboard("{Enter}");
    expect(ran).toEqual(["显示密码"]);
    expect(onClose).toHaveBeenCalledTimes(1);
    // Away and back: the first action again; a pill runs its own.
    await user.keyboard("{ArrowDown}");
    expect(screen.queryAllByTestId("palette-action")).toHaveLength(0);
    expect(screen.getByText("↑↓ 选择 · ↵ 执行 · Esc 关闭")).toBeInTheDocument();
    await user.keyboard("{ArrowUp}");
    expect(active()).toEqual(["复制密码↵"]);
    await user.click(screen.getByText("复制用户名"));
    expect(ran).toEqual(["显示密码", "复制用户名"]);
  });

  it("mouse hover moves the cursor and click runs", async () => {
    const user = userEvent.setup();
    const run = vi.fn();
    render(<CommandPalette open items={items(run)} onClose={vi.fn()} />);
    await user.hover(screen.getByRole("option", { name: /开始听写/ }));
    expect(screen.getByRole("option", { name: /开始听写/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.click(screen.getByRole("option", { name: /开始听写/ }));
    expect(run).toHaveBeenCalled();
    const { container } = render(<CommandPalette open={false} items={[]} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });
});
