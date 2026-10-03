import { MockBackend, sampleEntries } from "@lockra/shared/mock";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

afterEach(async () => {
  cleanup();
  await waitFor(() => {
    if (depthOf(history.state) !== 0) throw new Error("the history still holds pages");
  });
});

function phone(options: ConstructorParameters<typeof MockBackend>[0] = {}) {
  return new MockBackend({
    entries: sampleEntries(),
    updateMethod: "android",
    settings: { locale: "zh-cn" },
    ...options,
  });
}

async function about(user: ReturnType<typeof renderApp>["user"]) {
  await user.click(screen.getByTestId("codes-settings"));
  return within(await screen.findByTestId("about-update"));
}

describe("updates on the phone", () => {
  it("checks only when asked, and opens a newer release's page", async () => {
    const backend = phone({ release: { version: "0.7.0", notes: null, date: null, size: 1 } });
    const { user } = renderApp({ backend });
    await ready();
    const row = await about(user);
    expect(row.getByTestId("update-status")).toHaveTextContent("尚未检查更新");
    expect(row.getByText(/只连接 GitHub/)).toBeInTheDocument();
    expect(row.queryByRole("button", { name: "前往发布页" })).not.toBeInTheDocument();
    expect(backend.calls.some((c) => c.command === "update_check")).toBe(false);
    await user.click(row.getByRole("button", { name: "检查更新" }));
    expect(await row.findByText("有新版本 0.7.0 · 当前 0.1.0")).toBeInTheDocument();
    await user.click(row.getByRole("button", { name: "前往发布页" }));
    expect(row.queryByTestId("update-address")).not.toBeInTheDocument();
  });

  it("shows the page's address where no browser opens it", async () => {
    const backend = phone({
      release: { version: "0.7.0", notes: null, date: null, size: 1 },
      releasePageOpens: false,
    });
    const { user } = renderApp({ backend });
    await ready();
    const row = await about(user);
    await user.click(row.getByRole("button", { name: "检查更新" }));
    await user.click(await row.findByRole("button", { name: "前往发布页" }));
    expect(await row.findByTestId("update-address")).toHaveTextContent(
      "https://github.com/sunerpy/lockra/releases/tag/v0.7.0",
    );
  });

  it("says when this is the newest, and why a check failed", async () => {
    const backend = phone({ updateFailure: { step: "check", code: "update_network" } });
    const { user } = renderApp({ backend });
    await ready();
    const row = await about(user);
    await user.click(row.getByRole("button", { name: "检查更新" }));
    expect(await row.findByTestId("update-status")).toHaveTextContent(
      "更新失败 · 无法连接更新服务器",
    );
    const newest = phone();
    cleanup();
    const second = renderApp({ backend: newest });
    await ready();
    const again = await about(second.user);
    await second.user.click(again.getByRole("button", { name: "检查更新" }));
    expect(await again.findByTestId("update-status")).toHaveTextContent(
      /^已是最新 · 0\.1\.0 · 检查于/,
    );
  });

  it("offers no check where this build has none", async () => {
    const { user } = renderApp({ backend: phone({ updateMethod: null }) });
    await ready();
    await user.click(screen.getByTestId("codes-settings"));
    await screen.findByTestId("page-settings");
    expect(screen.queryByTestId("about-update")).not.toBeInTheDocument();
  });
});
