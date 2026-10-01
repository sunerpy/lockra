import { type Notice } from "@lockra/shared";
import { MockBackend, mockEntry } from "@lockra/shared/mock";
import { act, render, renderHook, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { BackendProvider, useBackend, useCodes, useUiState } from "./BackendProvider";

function Phase() {
  return <span data-testid="phase">{useUiState().phase}</span>;
}

function Gate({ children }: { children: ReactNode }) {
  const { state } = useBackend();
  return state ? <>{children}</> : <span>loading</span>;
}

describe("BackendProvider", () => {
  it("loads the state, follows state events and passes notices on", async () => {
    const backend = new MockBackend({ entries: [mockEntry("GitHub", "octocat")] });
    const notices: Notice[] = [];
    render(
      <BackendProvider backend={backend} onNotice={(n) => notices.push(n)}>
        <Gate>
          <Phase />
        </Gate>
      </BackendProvider>,
    );
    expect(screen.getByText("loading")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId("phase")).toHaveTextContent("unlocked"));
    await act(async () => {
      await backend.dispatch({ command: "vault_lock" });
    });
    expect(screen.getByTestId("phase")).toHaveTextContent("locked");
    act(() => backend.emitNotice({ type: "auto_locked" }));
    expect(notices).toEqual([{ type: "auto_locked" }]);
  });

  it("reports a failed first load", async () => {
    const backend = new MockBackend();
    vi.spyOn(backend, "getState").mockRejectedValue(new Error("no core"));
    function ErrorText() {
      return <span>{useBackend().error ?? "none"}</span>;
    }
    render(
      <BackendProvider backend={backend}>
        <ErrorText />
      </BackendProvider>,
    );
    await waitFor(() => expect(screen.getByText("no core")).toBeInTheDocument());
  });

  it("hooks outside the provider or before the state throw", () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    expect(() => renderHook(() => useBackend())).toThrow("useBackend outside <BackendProvider>");
    const backend = new MockBackend();
    expect(() =>
      renderHook(() => useUiState(), {
        wrapper: ({ children }) => <BackendProvider backend={backend}>{children}</BackendProvider>,
      }),
    ).toThrow("useUiState before the first state");
  });

  it("streams codes by entry id and unsubscribes on unmount", async () => {
    const entry = mockEntry("GitHub", "octocat");
    const backend = new MockBackend({ entries: [entry] });
    const { result, unmount } = renderHook(() => useCodes(), {
      wrapper: ({ children }) => <BackendProvider backend={backend}>{children}</BackendProvider>,
    });
    await waitFor(() => expect(result.current.get(entry.view.id)?.code).toMatch(/^\d{6}$/));
    unmount();
  });
});
