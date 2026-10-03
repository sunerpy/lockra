import { LockraError } from "@lockra/shared";
import { MockBackend } from "@lockra/shared/mock";
import { act, renderHook, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { BackendProvider } from "../backend/BackendProvider";
import { ToastViewport, useToasts } from "../components/Toast";
import { I18nProvider } from "../i18n/I18nProvider";
import { useDispatch, useGuarded, useSubmit } from "./dispatch";
import { ToasterProvider, useToaster } from "./toaster";

/** The providers a page has, with the toasts on screen. */
function Providers({ backend, children }: { backend: MockBackend; children: ReactNode }) {
  return (
    <BackendProvider backend={backend}>
      <I18nProvider locale="zh-CN">
        <Toasts>{children}</Toasts>
      </I18nProvider>
    </BackendProvider>
  );
}

function Toasts({ children }: { children: ReactNode }) {
  const store = useToasts();
  return (
    <ToasterProvider store={store}>
      {children}
      <ToastViewport toasts={store.toasts} onDismiss={store.dismiss} />
    </ToasterProvider>
  );
}

const wrapperFor = (backend: MockBackend) =>
  function Wrapper({ children }: { children: ReactNode }) {
    return <Providers backend={backend}>{children}</Providers>;
  };

describe("useSubmit", () => {
  it("answers what the work answers, and keeps a failure's code for the form", async () => {
    const { result } = renderHook(() => useSubmit());
    let answer: number | undefined;
    await act(async () => {
      answer = await result.current.run(async () => 7);
    });
    expect(answer).toBe(7);
    expect(result.current.error).toBeUndefined();
    await act(async () => {
      answer = await result.current.run(async () => {
        throw new LockraError("wrong_password");
      });
    });
    expect(answer).toBeUndefined();
    expect(result.current.error).toBe("wrong_password");
    await act(async () => {
      await result.current.run(async () => {
        throw new Error("not one of ours");
      });
    });
    expect(result.current.error).toBe("internal");
    act(() => result.current.setError(undefined));
    expect(result.current.busy).toBe(false);
  });
});

describe("useDispatch / useGuarded", () => {
  it("runs a command, and turns a failure into a translated toast", async () => {
    const backend = new MockBackend({ phase: "locked" });
    const { result } = renderHook(() => ({ dispatch: useDispatch(), guarded: useGuarded() }), {
      wrapper: wrapperFor(backend),
    });
    let state: unknown;
    await act(async () => {
      state = await result.current.dispatch({ command: "app_state" });
    });
    expect(state).toMatchObject({ phase: "locked" });
    await act(async () => {
      await result.current.dispatch({ command: "vault_unlock", password: "nope" });
    });
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    await act(async () => {
      await result.current.guarded(async () => {
        throw new Error("a native dialog failed");
      });
    });
    expect(screen.getAllByRole("alert")).toHaveLength(2);
  });
});

describe("ToasterProvider", () => {
  it("says a core notice and plain words as toasts", () => {
    const { result } = renderHook(() => useToaster(), { wrapper: wrapperFor(new MockBackend({})) });
    act(() => result.current.notice({ type: "device_unlock_turned_off" }));
    act(() => result.current.info("已复制"));
    const shown = [...screen.queryAllByRole("alert"), ...screen.queryAllByRole("status")];
    expect(shown).toHaveLength(2);
    expect(screen.getByRole("status")).toHaveTextContent("已复制");
  });

  it("is required by its hook", () => {
    expect(() => renderHook(() => useToaster())).toThrow("useToaster outside <ToasterProvider>");
  });
});
