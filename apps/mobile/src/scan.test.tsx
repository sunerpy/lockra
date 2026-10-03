import { MockBackend, sampleEntries } from "@lockra/shared/mock";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

const SCANNED = "otpauth://totp/Scanned:me@example.com?secret=MFRGGZDF&issuer=Scanned";

/** A camera that answers when the test says. */
class SlowCamera extends MockBackend {
  answer: ((scanned: boolean) => void) | undefined;
  override scanImport(): Promise<boolean> {
    return new Promise((resolve) => {
      this.answer = resolve;
    });
  }
}

function setVisibility(state: "visible" | "hidden") {
  Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
  act(() => {
    document.dispatchEvent(new Event("visibilitychange"));
  });
}

afterEach(async () => {
  Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
  cleanup();
  await waitFor(() => {
    if (depthOf(history.state) !== 0) throw new Error("the history still holds pages");
  });
});

describe("the camera and the photo picker", () => {
  it("scan a QR code into the preview, marked as the camera's", async () => {
    const { user } = renderApp({ mock: { scan: SCANNED } });
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-scan"));
    expect(await screen.findByTestId("page-preview")).toBeInTheDocument();
    const [found] = screen.getAllByTestId("candidate");
    expect(within(found as HTMLElement).getByText("Scanned")).toBeInTheDocument();
    expect(within(found as HTMLElement).getByText(/^相机/)).toBeInTheDocument();
  });

  it("stay on the Add page when the scan is left, and say why the camera could not be used", async () => {
    const backend = new MockBackend({ entries: sampleEntries(), settings: { locale: "zh-cn" } });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-scan"));
    expect(screen.getByTestId("page-add")).toBeInTheDocument();
    backend.setScan({ error: "camera_denied" });
    await user.click(screen.getByTestId("add-scan"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Lockra 无权使用相机");
    expect(screen.getByTestId("page-add")).toBeInTheDocument();
  });

  it("read photos into the preview, and scan the rest of a Google export into it", async () => {
    const backend = new MockBackend({ entries: sampleEntries(), settings: { locale: "zh-cn" } });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-images"));
    expect(await screen.findByTestId("page-preview")).toBeInTheDocument();
    const before = screen.getAllByTestId("candidate").length;
    // The mock's export misses its second code: the preview offers to scan it.
    backend.setScan(SCANNED);
    await user.click(screen.getByRole("button", { name: "扫描下一个二维码" }));
    await waitFor(() => expect(screen.getAllByTestId("candidate")).toHaveLength(before + 1));
    expect(screen.getByTestId("page-preview")).toBeInTheDocument();
  });

  it("keep the vault open while the camera is in front, and lock it if the app was left meanwhile", async () => {
    const backend = new SlowCamera({ entries: sampleEntries(), settings: { locale: "zh-cn" } });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-scan"));
    // The camera's page hides the app: that is not leaving it.
    setVisibility("hidden");
    setVisibility("visible");
    setVisibility("hidden");
    expect(backend.calls.some((c) => c.command === "vault_lock")).toBe(false);
    // The scan ends while the app is in the background: the vault locks then.
    act(() => backend.answer?.(false));
    expect(await screen.findByTestId("page-unlock")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({ command: "vault_lock" });
  });

  it("go on to the preview when the scan ends with the app in front again", async () => {
    const backend = new SlowCamera({ entries: sampleEntries(), settings: { locale: "zh-cn" } });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-scan"));
    setVisibility("hidden");
    setVisibility("visible");
    await backend.dispatch({ command: "import_text", text: SCANNED });
    act(() => backend.answer?.(true));
    expect(await screen.findByTestId("page-preview")).toBeInTheDocument();
    expect(backend.calls.some((c) => c.command === "vault_lock")).toBe(false);
    // Once the camera has gone, leaving the app locks the vault again.
    setVisibility("hidden");
    expect(await screen.findByTestId("page-unlock")).toBeInTheDocument();
  });
});
