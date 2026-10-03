import { MockBackend } from "@lockra/shared/mock";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { BackendProvider, useBackend, useUiState } from "../backend/BackendProvider";
import { useToasts } from "../components/Toast";
import { I18nProvider } from "../i18n/I18nProvider";
import { useUpdateSettings } from "./settings";
import { ToasterProvider } from "./toaster";

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
  return <ToasterProvider store={useToasts()}>{children}</ToasterProvider>;
}

/** The pages render once the first state is in; so does the hook here. */
function Ready({ children }: { children: ReactNode }) {
  return useBackend().state ? children : null;
}

describe("useUpdateSettings", () => {
  it("sends the current settings with the change on top", async () => {
    const backend = new MockBackend({ phase: "unlocked", settings: { auto_lock_minutes: 5 } });
    const { result } = renderHook(
      () => ({ update: useUpdateSettings(), settings: useUiState().settings }),
      {
        wrapper: ({ children }) => (
          <Providers backend={backend}>
            <Ready>{children}</Ready>
          </Providers>
        ),
      },
    );
    await waitFor(() => expect(result.current?.settings.auto_lock_minutes).toBe(5));
    act(() => result.current.update({ hide_codes: true }));
    await waitFor(() => expect(result.current.settings.hide_codes).toBe(true));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { hide_codes: true, auto_lock_minutes: 5 },
    });
  });
});
