import { MockBackend } from "@lockra/shared/mock";
import { act, screen } from "@testing-library/react";
import { renderApp } from "./test/render";

describe("App", () => {
  afterEach(() => {
    history.replaceState(null, "", "/");
  });

  it("shows the logo until the first state arrives", async () => {
    const backend = new MockBackend({ phase: "no_vault" });
    let release: (() => void) | undefined;
    const gate = new Promise<void>((r) => {
      release = r;
    });
    const getState = backend.getState.bind(backend);
    vi.spyOn(backend, "getState").mockImplementation(async () => {
      await gate;
      return getState();
    });
    renderApp({ backend });
    expect(screen.getByTestId("splash")).toBeInTheDocument();
    await act(async () => release?.());
    expect(await screen.findByTestId("page-welcome")).toBeInTheDocument();
  });

  it("turns core notices into toasts", async () => {
    const { backend } = renderApp();
    await screen.findByTestId("page-codes");
    act(() => backend.emitNotice({ type: "auto_locked" }));
    expect(screen.getByText("长时间没有操作，保险库已自动锁定")).toBeInTheDocument();
  });

  it("opens the component showcase on #showcase in a development build", async () => {
    history.replaceState(null, "", "/#showcase");
    renderApp();
    expect(
      await screen.findByText("组件展示（仅开发版）", undefined, { timeout: 5000 }),
    ).toBeInTheDocument();
  });
});
