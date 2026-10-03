import { MOCK_PASSWORD, MockBackend, sampleEntries } from "@lockra/shared/mock";
import { act, screen, waitFor, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";

function locked(options: ConstructorParameters<typeof MockBackend>[0] = {}) {
  return new MockBackend({
    entries: sampleEntries(),
    phase: "locked",
    settings: { locale: "zh-cn" },
    ...options,
  });
}

describe("Unlock", () => {
  it("counts wrong passwords, then makes the user wait", async () => {
    const { user } = renderApp({ backend: locked() });
    await ready();
    const field = screen.getByLabelText("主密码");
    const submit = screen.getByRole("button", { name: "解锁" });
    expect(submit).toBeDisabled();
    await user.type(field, "nope{Enter}");
    expect(await screen.findByText("密码错误（第 1 次）")).toBeInTheDocument();
    expect(field).toHaveValue("");
    await user.type(field, "nope{Enter}");
    expect(await screen.findByText("密码错误（第 2 次）")).toBeInTheDocument();
    await user.type(field, "nope{Enter}");
    expect(await screen.findByTestId("retry-wait")).toHaveTextContent(/请在 \d 秒后重试/);
    await user.type(field, MOCK_PASSWORD);
    expect(screen.getByRole("button", { name: "解锁" })).toBeDisabled();
  });

  it("unlocks with the master password", async () => {
    const { user } = renderApp({ backend: locked() });
    await ready();
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(screen.getAllByTestId("entry-row")).toHaveLength(sampleEntries().length);
  });

  it("offers the remembered key only when it is on, and only where a keychain exists", async () => {
    const first = renderApp({ backend: locked() });
    await ready();
    expect(
      screen.queryByRole("button", { name: "使用本机记住的密钥解锁" }),
    ).not.toBeInTheDocument();
    first.unmount();

    const missing = renderApp({
      backend: locked({ deviceUnlock: true, keychainAvailable: false }),
    });
    await ready();
    expect(screen.getByRole("button", { name: "使用本机记住的密钥解锁" })).toBeDisabled();
    missing.unmount();

    const { user } = renderApp({ backend: locked({ deviceUnlock: true }) });
    await ready();
    await user.click(screen.getByRole("button", { name: "使用本机记住的密钥解锁" }));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
  });

  it("unlocks with Touch ID from its button, and stays quiet when it is cancelled", async () => {
    const backend = locked({
      deviceUnlock: true,
      biometric: "touch_id",
      biometricUnlock: true,
      settings: { locale: "zh-cn", default_unlock: "password" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const button = screen.getByRole("button", { name: "使用 Touch ID 解锁" });
    expect(button.querySelector('[data-icon="fingerprint"]')).not.toBeNull();
    backend.answerBiometric("biometric_cancelled");
    await user.click(button);
    expect(screen.queryByRole("alert")).toBeNull();
    backend.answerBiometric("biometric_failed");
    await user.click(button);
    expect(await screen.findByRole("alert")).toHaveTextContent("验证未通过，请重试或输入主密码");
    backend.answerBiometric(null);
    await user.click(button);
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(backend.calls.filter((c) => c.command === "vault_unlock_device")).toContainEqual({
      command: "vault_unlock_device",
      reason: "解锁保险库",
    });
    expect(backend.biometricReasons).toEqual(["解锁保险库", "解锁保险库", "解锁保险库"]);
  });

  it("points to Settings on a computer with Touch ID that is not set up", async () => {
    renderApp({ backend: locked({ biometric: "touch_id", platform: "macos" }) });
    await ready();
    expect(
      screen.getByText("解锁后，可以在「设置 › 安全」开启「使用 Touch ID 解锁」。"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "使用 Touch ID 解锁" })).toBeNull();
  });

  it("says nothing of a fingerprint where there is none", async () => {
    renderApp({ backend: locked() });
    await ready();
    expect(screen.queryByText(/设置 › 安全/)).toBeNull();
  });

  it("keeps the Touch ID button while the sensor is away, and says why it cannot open", async () => {
    const backend = locked({ deviceUnlock: true, biometricUnlock: true, platform: "macos" });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByRole("button", { name: "使用 Touch ID 解锁" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "这台设备现在无法使用指纹、Touch ID 或 Windows Hello，请输入主密码",
    );
  });

  it("names Windows Hello on Windows", async () => {
    renderApp({
      backend: locked({
        deviceUnlock: true,
        biometric: "windows_hello",
        biometricUnlock: true,
        biometricAnswer: "biometric_cancelled",
      }),
    });
    await ready();
    expect(screen.getByRole("button", { name: "使用 Windows Hello 解锁" })).toBeInTheDocument();
  });

  it("resets a vault whose password is lost after the word is typed", async () => {
    const { user, backend } = renderApp({ backend: locked() });
    await ready();
    await user.click(screen.getByRole("button", { name: "忘记主密码？" }));
    const dialog = within(screen.getByRole("dialog", { name: "重置保险库" }));
    const confirm = dialog.getByRole("button", { name: "重置保险库" });
    expect(confirm).toBeDisabled();
    await user.type(dialog.getByLabelText("输入「重置」以确认"), "重");
    expect(confirm).toBeDisabled();
    await user.click(dialog.getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "忘记主密码？" }));
    const again = within(screen.getByRole("dialog", { name: "重置保险库" }));
    await user.type(again.getByLabelText("输入「重置」以确认"), "重置");
    await user.click(again.getByRole("button", { name: "重置保险库" }));
    expect(await screen.findByTestId("page-welcome")).toBeInTheDocument();
    expect(backend.calls).toContainEqual({ command: "vault_reset" });
  });
});

describe("Unlock with the system's check as the default", () => {
  let focused = true;
  beforeEach(() => {
    focused = true;
    vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
  });
  afterEach(() => vi.restoreAllMocks());
  // Once the effects that listen for the focus have run: the lock screen's come after its render.
  const leave = async () => {
    await act(async () => {});
    act(() => {
      focused = false;
      window.dispatchEvent(new Event("blur"));
    });
  };
  const come = async () => {
    await act(async () => {});
    act(() => {
      focused = true;
      window.dispatchEvent(new Event("focus"));
    });
  };
  const asked = (backend: MockBackend) =>
    backend.calls.filter((c) => c.command === "vault_unlock_device").length;
  const withCheck = {
    entries: sampleEntries(),
    deviceUnlock: true,
    biometricUnlock: true,
    biometricAnswer: "biometric_cancelled" as const,
    settings: { locale: "zh-cn" as const },
  };

  it("asks for Touch ID as Lockra starts in front, and leads with it", async () => {
    const backend = new MockBackend({ ...withCheck, phase: "locked", biometric: "touch_id" });
    renderApp({ backend });
    await ready();
    await waitFor(() => expect(asked(backend)).toBe(1));
    expect(backend.biometricReasons).toEqual(["解锁保险库"]);
    // Cancelled: nothing to say, and the button comes first.
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("使用 Touch ID 或主密码解锁。")).toBeInTheDocument();
    const touchId = screen.getByRole("button", { name: "使用 Touch ID 解锁" });
    expect(touchId).toHaveAttribute("data-variant", "primary");
    expect(screen.getByRole("button", { name: "解锁" })).not.toHaveAttribute(
      "data-variant",
      "primary",
    );
    expect(
      touchId.compareDocumentPosition(screen.getByLabelText("主密码")) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    // Not again until Lockra has been left and comes back.
    await come();
    expect(asked(backend)).toBe(1);
    backend.answerBiometric(null);
    await leave();
    await come();
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(asked(backend)).toBe(2);
  });

  it("asks for Windows Hello when Lockra locks by itself, once it is in front", async () => {
    const backend = new MockBackend({ ...withCheck, biometric: "windows_hello" });
    renderApp({ backend });
    await ready();
    // Locked by itself in front: at once.
    act(() => backend.autoLock());
    await waitFor(() => expect(asked(backend)).toBe(1));
    backend.answerBiometric(null);
    await leave();
    await come();
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    // Locked by itself in the background: when Lockra comes to the front.
    await leave();
    act(() => backend.autoLock());
    await screen.findByTestId("page-unlock");
    expect(asked(backend)).toBe(2);
    await come();
    await waitFor(() => expect(asked(backend)).toBe(3));
  });

  it("waits after the user locks it, until they leave and come back", async () => {
    const backend = new MockBackend({ ...withCheck, biometric: "touch_id" });
    const { user } = renderApp({ backend });
    await ready();
    await user.keyboard("{Control>}l{/Control}");
    await screen.findByTestId("page-unlock");
    await come();
    expect(asked(backend)).toBe(0);
    await leave();
    await come();
    await waitFor(() => expect(asked(backend)).toBe(1));
  });

  it("puts the master password first when that is the default", async () => {
    const backend = new MockBackend({
      ...withCheck,
      phase: "locked",
      biometric: "touch_id",
      settings: { locale: "zh-cn", default_unlock: "password" },
    });
    renderApp({ backend });
    await ready();
    await leave();
    await come();
    expect(asked(backend)).toBe(0);
    expect(screen.getByRole("button", { name: "解锁" })).toHaveAttribute("data-variant", "primary");
    expect(screen.getByText("输入主密码解锁。")).toBeInTheDocument();
  });

  it("does not ask while the check cannot run", async () => {
    // Turned on, but the sensor is away (a closed lid): the button stays, nothing asks by itself.
    const backend = new MockBackend({ ...withCheck, phase: "locked", platform: "macos" });
    renderApp({ backend });
    await ready();
    await leave();
    await come();
    expect(asked(backend)).toBe(0);
  });
});
