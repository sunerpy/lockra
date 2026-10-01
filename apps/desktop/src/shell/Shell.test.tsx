import { act, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";

describe("Shell", () => {
  it("navigates with the sidebar and titles the page", async () => {
    const { user } = renderApp();
    await ready();
    const banner = screen.getByRole("banner");
    expect(within(banner).getByText("验证码")).toBeInTheDocument();
    for (const [nav, page] of [
      ["导入", "page-import"],
      ["导出", "page-export"],
      ["备份", "page-backup"],
      ["验证码", "page-codes"],
    ] as const) {
      await user.click(
        within(screen.getByRole("navigation", { name: "主导航" })).getByRole("button", {
          name: new RegExp(`^${nav}`),
        }),
      );
      expect(await screen.findByTestId(page)).toBeInTheDocument();
      expect(within(banner).getByText(nav)).toBeInTheDocument();
    }
  });

  it("regression: the shell's row cannot grow past the window, so only the page body scrolls", async () => {
    renderApp();
    await ready();
    const shell = screen.getByTestId("shell");
    expect(shell.style.gridTemplateRows).toBe("minmax(0, 1fr)");
    expect(shell).toHaveClass("h-full", "overflow-hidden");
    expect(screen.getByTestId("page-body")).toHaveClass("min-h-0", "overflow-y-auto");
  });

  it("reads out the vault state and the auto-lock in the title bar", async () => {
    renderApp();
    await ready();
    const banner = screen.getByRole("banner");
    expect(within(banner).getByText("已解锁 · 8 个账号")).toBeInTheDocument();
    expect(within(banner).getByText("5 分钟后自动锁定")).toBeInTheDocument();
  });

  it("copies an account's code from the command palette", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.keyboard("{Control>}k{/Control}");
    const palette = await screen.findByRole("dialog", { name: "命令菜单" });
    expect(within(palette).getByText("组件展示（仅开发版）")).toBeInTheDocument();
    await user.keyboard("Cloudflare{Enter}");
    expect(screen.queryByRole("dialog", { name: "命令菜单" })).not.toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({ command: "entry_copy" });
  });

  it("runs the actions of the palette", async () => {
    const { user, backend } = renderApp({
      mock: { clipboard: "otpauth://totp/Clip:me?secret=JBSWY3DPEHPK3PXP" },
    });
    await ready();
    const run = async (label: string) => {
      await user.keyboard("{Control>}k{/Control}");
      await screen.findByRole("dialog", { name: "命令菜单" });
      await user.keyboard(`${label}{Enter}`);
    };
    await run("粘贴 otpauth");
    expect(screen.getByRole("dialog", { name: "粘贴 otpauth 链接" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await run("扫描图片");
    expect(await screen.findByTestId("page-import")).toBeInTheDocument();
    await run("备份");
    expect(await screen.findByTestId("page-backup")).toBeInTheDocument();
    await run("从剪贴板");
    expect(backend.calls.at(-1)).toEqual({ command: "import_clipboard" });
    await run("设置");
    expect(await screen.findByRole("dialog", { name: "设置" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await run("锁定");
    expect(await screen.findByTestId("page-unlock")).toBeInTheDocument();
  });

  it("answers the global shortcuts", async () => {
    const { user } = renderApp();
    await ready();
    await user.keyboard("/");
    expect(screen.getByTestId("codes-search")).toHaveFocus();
    await user.click(
      within(screen.getByRole("navigation", { name: "主导航" })).getByRole("button", {
        name: /^导出/,
      }),
    );
    await user.keyboard("{Control>}f{/Control}");
    expect(await screen.findByTestId("codes-search")).toHaveFocus();
    await user.keyboard("{Control>}n{/Control}");
    expect(screen.getByRole("dialog", { name: "手动添加账号" })).toBeInTheDocument();
    // An open dialog keeps the keys to itself.
    await user.keyboard("{Control>}l{/Control}");
    expect(screen.getByTestId("page-codes")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.keyboard("{Control>}l{/Control}");
    expect(await screen.findByTestId("page-unlock")).toBeInTheDocument();
  });

  it("locks from the sidebar, collapses it, and cycles the theme", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByRole("button", { name: "收起为图标栏" }));
    expect(localStorage.getItem("lockra.sidebar.collapsed")).toBe("1");
    await user.click(screen.getByRole("button", { name: "展开侧栏" }));
    expect(localStorage.getItem("lockra.sidebar.collapsed")).toBe("0");
    await user.click(screen.getByRole("button", { name: /切换到/ }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { follow_system_theme: false, theme: "light" },
    });
    await user.click(screen.getByRole("button", { name: /切换到/ }));
    expect(backend.calls.at(-1)).toMatchObject({ settings: { theme: "dark" } });
    await user.click(screen.getByTestId("sidebar-lock"));
    expect(await screen.findByTestId("page-unlock")).toBeInTheDocument();
  });

  it("goes to the import page when a preview arrives (a drop, a restore)", async () => {
    const { backend } = renderApp();
    await ready();
    await act(async () => {
      await backend.pickImportFiles("images");
    });
    expect(screen.getByTestId("page-import")).toBeInTheDocument();
    expect(
      within(screen.getByRole("navigation", { name: "主导航" })).getByText("3"),
    ).toBeInTheDocument();
  });

  it("shows the last backup in the footer", async () => {
    const { backend } = renderApp();
    await ready();
    await act(async () => {
      await backend.saveBackup();
    });
    expect(screen.getByRole("contentinfo")).toHaveTextContent("上次备份 刚刚");
  });
});
