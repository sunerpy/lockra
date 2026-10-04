import { MOCK_PASSWORD, MockBackend, sampleEntries } from "@lockra/shared/mock";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

afterEach(async () => {
  cleanup();
  Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
  await waitFor(() => {
    if (depthOf(history.state) !== 0) throw new Error("the history still holds pages");
  });
});

/** The app leaves the screen (another app, the screen off) or comes back to it, once the effects
 *  that listen for it have run. */
async function visibility(state: "hidden" | "visible") {
  await act(async () => {});
  Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
  act(() => {
    document.dispatchEvent(new Event("visibilitychange"));
  });
}

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

  it("unlocks the vault from its button, and a cancelled check says nothing", async () => {
    const backend = phone({
      phase: "locked",
      deviceUnlock: true,
      biometricUnlock: true,
      settings: { locale: "zh-cn", default_unlock: "password" },
    });
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

  it("chooses the fingerprint or the master password first, once it is on", async () => {
    const backend = phone({ deviceUnlock: true, biometricUnlock: true });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("codes-settings"));
    const choice = within(await screen.findByRole("radiogroup", { name: "默认解锁方式" }));
    expect(choice.getByRole("radio", { name: "指纹" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText(/打开或切回 Lockra 时会自动请求指纹/)).toBeInTheDocument();
    await user.click(choice.getByRole("radio", { name: "主密码" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { default_unlock: "password" },
    });
  });

  it("asks for the fingerprint as the app comes back to the screen, and leads with it", async () => {
    const backend = phone({
      deviceUnlock: true,
      biometricUnlock: true,
      biometricAnswer: "biometric_cancelled",
    });
    renderApp({ backend });
    await ready();
    const asked = () => backend.calls.filter((c) => c.command === "vault_unlock_device").length;
    // Leaving locks the vault; the check waits for the app to be on the screen again.
    await visibility("hidden");
    await screen.findByTestId("page-unlock");
    expect(asked()).toBe(0);
    await visibility("visible");
    await waitFor(() => expect(asked()).toBe(1));
    const button = screen.getByRole("button", { name: "使用指纹解锁" });
    expect(button).toHaveAttribute("data-variant", "primary");
    expect(screen.getByText("使用指纹或主密码解锁。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "解锁" })).not.toHaveAttribute(
      "data-variant",
      "primary",
    );
    // Cancelled: only once the app has left and come back.
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    backend.answerBiometric(null);
    await visibility("hidden");
    await visibility("visible");
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(asked()).toBe(2);
  });

  it("waits after the user's own lock until the app has left the screen", async () => {
    const backend = phone({
      deviceUnlock: true,
      biometricUnlock: true,
      biometricAnswer: "biometric_cancelled",
    });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("codes-lock"));
    await screen.findByTestId("page-unlock");
    const asked = () => backend.calls.filter((c) => c.command === "vault_unlock_device").length;
    expect(asked()).toBe(0);
    await visibility("hidden");
    await visibility("visible");
    await waitFor(() => expect(asked()).toBe(1));
  });

  it("asks nothing by itself when the master password comes first", async () => {
    const backend = phone({
      phase: "locked",
      deviceUnlock: true,
      biometricUnlock: true,
      settings: { locale: "zh-cn", default_unlock: "password" },
    });
    renderApp({ backend });
    await ready();
    await visibility("hidden");
    await visibility("visible");
    expect(backend.calls.some((c) => c.command === "vault_unlock_device")).toBe(false);
    expect(screen.getByRole("button", { name: "解锁" })).toHaveAttribute("data-variant", "primary");
  });
});

describe("the fingerprint by default", () => {
  const enabled = async (backend: MockBackend) =>
    (await backend.getState()).lock.device_unlock.biometric.enabled;
  const enables = (backend: MockBackend) =>
    backend.calls.filter((c) => c.command === "device_biometric_enable");

  async function createVault(user: ReturnType<typeof renderApp>["user"]) {
    await user.type(screen.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.type(screen.getByLabelText("再输入一次"), MOCK_PASSWORD);
    await user.click(screen.getByRole("button", { name: "创建保险库" }));
    await screen.findByTestId("page-codes");
  }

  it("turns on for a new vault with one check, the switch on unless turned off", async () => {
    const backend = phone({ phase: "no_vault", entries: [] });
    const { user } = renderApp({ backend });
    await ready();
    const row = within(screen.getByTestId("welcome-fingerprint"));
    expect(row.getByRole("switch", { name: "使用指纹解锁" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await createVault(user);
    await waitFor(async () => expect(await enabled(backend)).toBe(true));
    expect(enables(backend)).toEqual([
      { command: "device_biometric_enable", reason: "开启指纹解锁" },
    ]);
    // Nothing more to offer.
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("turned off for a new vault, asks for nothing and is not offered later", async () => {
    const backend = phone({ phase: "no_vault", entries: [] });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(within(screen.getByTestId("welcome-fingerprint")).getByRole("switch"));
    await createVault(user);
    await waitFor(async () =>
      expect((await backend.getState()).settings.biometric_offer).toBe(false),
    );
    expect(enables(backend)).toEqual([]);
    // Unlocked again with the master password: no offer.
    await user.click(screen.getByTestId("codes-lock"));
    await user.type(await screen.findByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    await screen.findByTestId("page-codes");
    await act(async () => {});
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("turns on for a vault restored from a backup too", async () => {
    const backend = phone({ phase: "no_vault", entries: [] });
    const { user } = renderApp({ backend });
    await ready();
    const restore = within(screen.getByTestId("welcome-restore"));
    await user.click(restore.getByRole("button", { name: "选择备份文件…" }));
    await user.type(await restore.findByLabelText("备份密码"), `${MOCK_PASSWORD}{Enter}`);
    await screen.findByTestId("page-codes");
    await waitFor(async () => expect(await enabled(backend)).toBe(true));
  });

  it("is offered once after a master-password unlock, and 暂不 ends the offer", async () => {
    const backend = phone({ phase: "locked" });
    const { user } = renderApp({ backend });
    await ready();
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    const offer = within(await screen.findByRole("dialog", { name: "使用指纹解锁？" }));
    expect(offer.getByText(/打开或切回 Lockra 时验证指纹即可解锁/)).toBeInTheDocument();
    await user.click(offer.getByRole("button", { name: "暂不" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { biometric_offer: false },
    });
    expect(enables(backend)).toEqual([]);
    await user.click(screen.getByTestId("codes-lock"));
    await user.type(await screen.findByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    await screen.findByTestId("page-codes");
    await act(async () => {});
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("turns on from the offer with one check; a cancelled check offers it again next time", async () => {
    const backend = phone({ phase: "locked" });
    const { user } = renderApp({ backend });
    await ready();
    backend.answerBiometric("biometric_cancelled");
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    let offer = within(await screen.findByRole("dialog", { name: "使用指纹解锁？" }));
    await user.click(offer.getByRole("button", { name: "开启" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(await enabled(backend)).toBe(false);
    expect((await backend.getState()).settings.biometric_offer).toBe(true);
    await user.click(screen.getByTestId("codes-lock"));
    backend.answerBiometric(null);
    await user.type(await screen.findByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    offer = within(await screen.findByRole("dialog", { name: "使用指纹解锁？" }));
    await user.click(offer.getByRole("button", { name: "开启" }));
    await waitFor(async () => expect(await enabled(backend)).toBe(true));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(enables(backend)).toHaveLength(2);
  });

  it("is neither switched on nor offered where no fingerprint is enrolled", async () => {
    const welcome = renderApp({
      backend: phone({ phase: "no_vault", entries: [], biometric: null }),
    });
    await ready();
    expect(screen.queryByTestId("welcome-fingerprint")).not.toBeInTheDocument();
    welcome.unmount();
    const backend = phone({ phase: "locked", biometric: null });
    const { user } = renderApp({ backend });
    await ready();
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    await screen.findByTestId("page-codes");
    await act(async () => {});
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
