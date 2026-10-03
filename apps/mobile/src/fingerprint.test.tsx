import { MOCK_PASSWORD, MockBackend, sampleEntries } from "@lockra/shared/mock";
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
    biometric: "fingerprint",
    settings: { locale: "zh-cn" },
    ...options,
  });
}

describe("the fingerprint on the phone", () => {
  it("turns on with one passed check, and off behind the master password", async () => {
    const backend = phone();
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("codes-settings"));
    const row = within(await screen.findByTestId("settings-fingerprint"));
    expect(row.getByText("使用指纹解锁")).toBeInTheDocument();
    const toggle = row.getByRole("switch");
    expect(toggle).toHaveAttribute("aria-checked", "false");
    await user.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));
    expect(backend.calls.at(-1)).toEqual({
      command: "device_biometric_enable",
      reason: "开启指纹解锁",
    });
    // Off: the master password first, and the key goes with the check.
    await user.click(toggle);
    const dialog = within(await screen.findByRole("dialog", { name: "关闭指纹解锁" }));
    await user.type(dialog.getByLabelText("主密码"), "wrong{Enter}");
    expect(await dialog.findByText("密码错误")).toBeInTheDocument();
    await user.type(dialog.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(backend.calls.at(-1)).toEqual({
      command: "device_unlock_disable",
      password: MOCK_PASSWORD,
    });
    expect(toggle).toHaveAttribute("aria-checked", "false");
  });

  it("is not offered where no fingerprint is enrolled", async () => {
    const { user } = renderApp({ backend: phone({ biometric: null }) });
    await ready();
    await user.click(screen.getByTestId("codes-settings"));
    await screen.findByTestId("page-settings");
    expect(screen.queryByTestId("settings-fingerprint")).not.toBeInTheDocument();
  });

  it("unlocks the vault, and a cancelled check says nothing", async () => {
    const backend = phone({ phase: "locked", deviceUnlock: true, biometricUnlock: true });
    const { user } = renderApp({ backend });
    await ready();
    backend.answerBiometric("biometric_cancelled");
    const button = screen.getByRole("button", { name: "使用指纹解锁" });
    await user.click(button);
    await waitFor(() => expect(button).not.toHaveAttribute("aria-busy", "true"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    backend.answerBiometric("biometric_unavailable");
    await user.click(button);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "这台设备现在无法使用指纹、Touch ID 或 Windows Hello，请输入主密码",
    );
    backend.answerBiometric(null);
    await user.click(button);
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({ command: "vault_unlock_device", reason: "解锁保险库" });
  });

  it("says where to turn it on while it is off", async () => {
    renderApp({ backend: phone({ phase: "locked" }) });
    await ready();
    expect(screen.queryByRole("button", { name: "使用指纹解锁" })).not.toBeInTheDocument();
    expect(screen.getByTestId("biometric-offer")).toHaveTextContent(
      "解锁后，可以在「设置 › 安全」开启「使用指纹解锁」。",
    );
  });
});
