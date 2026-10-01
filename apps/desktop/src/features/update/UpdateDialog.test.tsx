import { zhT } from "@lockra/shared";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ready, renderApp } from "../../test/render";
import { etaText, rateFrom } from "./download-rate";
import { ReleaseNotes, inlineParts, parseNotes } from "./release-notes";
import { downloadProgress, statusVersion } from "./status";

describe("release notes", () => {
  it("reads release-please's markdown as headings, lists and paragraphs, minus the version line", () => {
    const notes = [
      "## 0.3.0 (2026-10-02)",
      "",
      "### Features",
      "",
      "* **update:** faster ([a1b2c3d](https://example.test/c/a1b2c3d))",
      "- a second item",
      "",
      "A paragraph",
      "that wraps.",
    ].join("\r\n");
    expect(parseNotes(notes, "0.3.0")).toEqual([
      { kind: "heading", level: 3, text: "Features" },
      {
        kind: "list",
        items: ["**update:** faster ([a1b2c3d](https://example.test/c/a1b2c3d))", "a second item"],
      },
      { kind: "paragraph", text: "A paragraph that wraps." },
    ]);
    // Another version's heading stays; so does everything without a version.
    expect(parseNotes("## [0.3.1](x)", "0.3.0")[0]).toMatchObject({ kind: "heading" });
    expect(parseNotes("# v0.3.0", "0.3.0")).toEqual([]);
    expect(parseNotes("text\n* item\nmore")).toEqual([
      { kind: "paragraph", text: "text" },
      { kind: "list", items: ["item"] },
      { kind: "paragraph", text: "more" },
    ]);
  });

  it("inline markup is text, a link is only its text, and nothing is HTML", () => {
    expect(inlineParts("**a:** b `c` ([d1](https://x.test)) [e](https://y.test)")).toEqual([
      { kind: "bold", text: "a:" },
      { kind: "text", text: " b " },
      { kind: "code", text: "c" },
      { kind: "text", text: " (d1) e" },
    ]);
    expect(inlineParts("see ([](https://x.test))")).toEqual([{ kind: "text", text: "see" }]);
    const { container } = render(
      <ReleaseNotes markdown={'* <img src=x onerror="alert(1)"> [link](javascript:alert(1))'} />,
    );
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("a")).toBeNull();
    expect(container.textContent).toBe('<img src=x onerror="alert(1)"> link');
  });
});

describe("download progress", () => {
  it("measures the speed over the window and words the progress and the time left", () => {
    expect(rateFrom([])).toBeUndefined();
    expect(rateFrom([{ at: 0, received: 0 }])).toBeUndefined();
    expect(
      rateFrom([
        { at: 0, received: 0 },
        { at: 300, received: 10 },
      ]),
    ).toBeUndefined();
    expect(
      rateFrom([
        { at: 0, received: 0 },
        { at: 1000, received: 2_097_152 },
      ]),
    ).toBe(2_097_152);
    expect(
      rateFrom([
        { at: 0, received: 5 },
        { at: 1000, received: 1 },
      ]),
    ).toBeUndefined();
    const { t } = zhT;
    expect(etaText(0, 1000, 100, t)).toBe("剩余约 10 秒");
    expect(etaText(0, 100_000, 1000, t)).toBe("剩余约 1 分 40 秒");
    expect(etaText(10, 10, 5, t)).toBeUndefined();
    expect(etaText(0, null, 5, t)).toBeUndefined();
    expect(etaText(0, 10, undefined, t)).toBeUndefined();
    expect(downloadProgress(4_194_304, 11_508_084)).toBe("36%");
    expect(downloadProgress(2_000, 1_000)).toBe("100%");
    expect(downloadProgress(3_145_728, null)).toBe("3.0 MB");
    expect(statusVersion({ state: "ready", version: "0.3.0" })).toBe("0.3.0");
    expect(statusVersion({ state: "checking" })).toBeUndefined();
  });
});

describe("UpdateDialog", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("a new version shows on the title bar and opens the dialog, which walks to the restart", async () => {
    const { user, backend } = renderApp({ mock: { updateMethod: "appimage" } });
    await ready();
    expect(screen.queryByTestId("update-badge")).toBeNull();
    backend.setRelease({
      version: "0.3.0",
      notes: "### Features\n\n* faster",
      date: null,
      size: 9,
    });
    await act(async () => {
      await backend.dispatch({ command: "update_check" });
    });
    const badge = await screen.findByTestId("update-badge");
    expect(badge).toHaveTextContent("新版本 0.3.0");
    await user.click(badge);
    const dialog = within(screen.getByRole("dialog", { name: "发现新版本 0.3.0" }));
    expect(dialog.getByTestId("release-notes")).toHaveTextContent("faster");
    expect(dialog.getByTestId("update-method")).toHaveTextContent("替换此 AppImage 文件");
    expect(dialog.getByText(/仅连接 GitHub/)).toBeInTheDocument();
    await user.click(dialog.getByRole("button", { name: "稍后" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByTestId("update-badge")).toHaveTextContent("新版本 0.3.0");

    // Downloaded by the automatic update: the restart is one click from the title bar.
    act(() => backend.simulateUpdate({ state: "ready", version: "0.3.0" }));
    expect(screen.getByTestId("update-badge")).toHaveTextContent("重启以更新");
    await user.click(screen.getByTestId("update-badge"));
    const readyDialog = within(screen.getByRole("dialog", { name: "新版本 0.3.0 已下载" }));
    expect(readyDialog.getByText("已下载并校验签名，重启后完成更新。")).toBeInTheDocument();
    await user.click(readyDialog.getByRole("button", { name: "重启并更新" }));
    expect(backend.calls.at(-1)).toEqual({ command: "update_install" });
    await waitFor(() => {
      expect(screen.getByTestId("update-dialog")).toHaveAttribute("data-state", "installing");
    });
    expect(screen.getByRole("button", { name: "正在安装，Lockra 即将重启…" })).toBeDisabled();
    expect(screen.queryByTestId("update-badge")).toBeNull();
  });

  it("shows the download's speed and time left once it has two readings; closing keeps it going", async () => {
    // The speed is timed on `performance.now()`: fake it with the timers, or the two readings are
    // as far apart as the test happens to run.
    vi.useFakeTimers({
      shouldAdvanceTime: true,
      toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date", "performance"],
    });
    const { backend } = renderApp({ mock: { updateMethod: "nsis" } });
    const user = userEvent.setup({ advanceTimers: (ms) => vi.advanceTimersByTime(ms) });
    await ready();
    act(() =>
      backend.simulateUpdate({
        state: "downloading",
        version: "0.3.0",
        received: 0,
        total: 50_000_000,
      }),
    );
    await user.click(await screen.findByTestId("update-badge"));
    expect(screen.getByTestId("update-progress")).toHaveTextContent("0%");
    expect(screen.queryByTestId("update-speed")).toBeNull();
    // A first reading the open dialog certainly heard (the shell's effects ran by now), then one
    // a second later.
    act(() =>
      backend.simulateUpdate({
        state: "downloading",
        version: "0.3.0",
        received: 0,
        total: 50_000_000,
      }),
    );
    expect(screen.queryByTestId("update-speed")).toBeNull();
    act(() => {
      vi.advanceTimersByTime(1000);
      backend.simulateUpdate({
        state: "downloading",
        version: "0.3.0",
        received: 10_485_760,
        total: 50_000_000,
      });
    });
    await waitFor(() => {
      expect(screen.getByTestId("update-speed")).toHaveTextContent(/MB\/s$/);
    });
    expect(screen.getByTestId("update-eta")).toHaveTextContent(/^剩余约/);
    expect(screen.getByTestId("update-progress")).toHaveTextContent("10.0 MB / 47.7 MB");
    await user.click(screen.getByRole("button", { name: "后台下载" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByTestId("update-badge")).toHaveTextContent("下载中 20%");
  });

  it("an up-to-date or failed updater reads as one status line, and retries from the dialog", async () => {
    const { user, backend } = renderApp({ mock: { updateMethod: "deb" } });
    await ready();
    act(() =>
      backend.simulateUpdate({
        state: "available",
        version: "0.3.0",
        notes: null,
        date: null,
        checked_at_ms: 1,
      }),
    );
    await user.click(await screen.findByTestId("update-badge"));
    act(() => backend.simulateUpdate({ state: "failed", code: "update_network", at_ms: 1 }));
    const failed = within(screen.getByRole("dialog", { name: "更新失败" }));
    expect(failed.getByRole("alert")).toHaveTextContent("更新失败：无法连接更新服务器");
    await user.click(failed.getByRole("button", { name: "重试" }));
    expect(backend.calls.at(-1)).toEqual({ command: "update_check" });
    const status = within(screen.getByRole("dialog", { name: "软件更新" }));
    expect(status.getByText(/^已是最新 · 0\.1\.0/)).toBeInTheDocument();
    await user.click(status.getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("dialog", { name: "软件更新" })).toBeNull();
  });
});
