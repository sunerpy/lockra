import { MOCK_PASSWORD, MockBackend, sampleEntries } from "@lockra/shared/mock";
import { screen, within } from "@testing-library/react";
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

  it("unlocks with Touch ID when the vault asks for it, and stays quiet when it is cancelled", async () => {
    const backend = locked({ deviceUnlock: true, biometric: "touch_id", biometricUnlock: true });
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
      "这台电脑现在无法使用指纹或 Windows Hello，请输入主密码",
    );
  });

  it("names Windows Hello on Windows", async () => {
    renderApp({
      backend: locked({ deviceUnlock: true, biometric: "windows_hello", biometricUnlock: true }),
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
