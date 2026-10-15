import { MockBackend, sampleEntries } from "@lockra/shared/mock";
import { act, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";

/** The app unlocked with the sample accounts, in Chinese. */
function renderTray() {
  const backend = new MockBackend({ entries: sampleEntries(), settings: { locale: "zh-cn" } });
  return renderApp({ backend });
}

describe("the tray's 「设置…」 and 「检查更新…」", () => {
  it("opens Settings on the core's word, and never over a dialog already open", async () => {
    const { user, backend } = renderTray();
    await ready();
    act(() => backend.emitNotice({ type: "open_settings" }));
    const settings = await screen.findByRole("dialog", { name: "设置" });
    expect(within(settings).getByRole("tab", { name: "通用" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.keyboard("{Escape}");
    // A dialog with the user's own typing in it stays as it is.
    await user.keyboard("{Control>}n{/Control}");
    const adding = await screen.findByRole("dialog", { name: "手动添加账号" });
    act(() => backend.emitNotice({ type: "open_settings" }));
    expect(screen.queryByRole("dialog", { name: "设置" })).toBeNull();
    expect(adding).toBeInTheDocument();
  });

  it("opens the update dialog over whatever is open, and tells nothing as a toast", async () => {
    const { user, backend } = renderTray();
    await ready();
    await user.keyboard("{Control>}n{/Control}");
    await screen.findByRole("dialog", { name: "手动添加账号" });
    act(() => backend.emitNotice({ type: "open_updates" }));
    expect(await screen.findByRole("dialog", { name: "软件更新" })).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "手动添加账号" })).toBeInTheDocument();
    act(() => backend.emitNotice({ type: "open_settings" }));
    expect(screen.queryByTestId("toast-viewport")).toBeNull();
  });
});
