import { MOCK_PASSWORD, MOCK_SYNC_KEY, MockBackend, mockSyncSpace } from "@lockra/shared/mock";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { BackendProvider, useBackend } from "../backend/BackendProvider";
import { I18nProvider } from "../i18n/I18nProvider";
import { SyncKeyReminder } from "./SyncKeyReminder";

function Ready({ children }: { children: ReactNode }) {
  return useBackend().state ? children : null;
}

function show(backend: MockBackend, onShow = () => {}) {
  render(
    <BackendProvider backend={backend}>
      <I18nProvider locale="zh-CN">
        <Ready>
          <span>ready</span>
          <SyncKeyReminder onShow={onShow} />
        </Ready>
      </I18nProvider>
    </BackendProvider>,
  );
}

const unsaved = { phase: "unlocked" as const, sync: mockSyncSpace({ key_saved: false }) };

describe("SyncKeyReminder", () => {
  it("is not there once the key is saved, nor without a space", async () => {
    show(new MockBackend({ phase: "unlocked", sync: mockSyncSpace() }));
    show(new MockBackend({ phase: "unlocked" }));
    await waitFor(() => expect(screen.getAllByText("ready")).toHaveLength(2));
    expect(screen.queryByTestId("sync-key-reminder")).not.toBeInTheDocument();
  });

  it("saves the key with the biometric check that unlocks the vault, and goes away", async () => {
    const user = userEvent.setup();
    const backend = new MockBackend({ ...unsaved, biometric: "touch_id", biometricUnlock: true });
    show(backend);
    await user.click(await screen.findByRole("button", { name: "保存到文件…" }));
    await waitFor(() => expect(screen.queryByTestId("sync-key-reminder")).not.toBeInTheDocument());
    expect(backend.biometricReasons).toEqual(["保存恢复密钥"]);
    // The key goes in its slot in the core; the words around it are the interface's.
    expect(backend.savedSyncKeys).toEqual([
      { fileName: "Lockra 恢复密钥.txt", text: expect.stringContaining(MOCK_SYNC_KEY) as string },
    ]);
    expect(backend.savedSyncKeys[0]?.text.startsWith("Lockra 恢复密钥\n\n")).toBe(true);
  });

  it("asks for the master password when the biometric check cannot be used", async () => {
    const user = userEvent.setup();
    const backend = new MockBackend({
      ...unsaved,
      biometric: "touch_id",
      biometricUnlock: true,
      biometricAnswer: "biometric_cancelled",
    });
    show(backend);
    await user.click(await screen.findByRole("button", { name: "保存到文件…" }));
    const field = await screen.findByLabelText("输入主密码以保存恢复密钥");
    await user.type(field, "wrong password");
    await user.click(screen.getByRole("button", { name: "保存" }));
    expect(await screen.findByText("密码错误")).toBeInTheDocument();
    await user.type(screen.getByLabelText("输入主密码以保存恢复密钥"), MOCK_PASSWORD);
    await user.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(screen.queryByTestId("sync-key-reminder")).not.toBeInTheDocument());
  });

  it("asks for the password at once without a biometric check, and a cancelled dialog keeps it", async () => {
    const user = userEvent.setup();
    const backend = new MockBackend(unsaved);
    show(backend);
    await user.click(await screen.findByRole("button", { name: "保存到文件…" }));
    backend.cancelNextSave = true;
    await user.type(screen.getByLabelText("输入主密码以保存恢复密钥"), MOCK_PASSWORD);
    await user.click(screen.getByRole("button", { name: "保存" }));
    expect(screen.getByTestId("sync-key-reminder")).toBeInTheDocument();
    expect(backend.savedSyncKeys).toEqual([]);
  });

  it("leaves showing the key to the page, where it can be written down", async () => {
    const user = userEvent.setup();
    const backend = new MockBackend(unsaved);
    const onShow = vi.fn();
    show(backend, onShow);
    await user.click(await screen.findByRole("button", { name: "显示恢复密钥…" }));
    expect(onShow).toHaveBeenCalledOnce();
    // Saying it is written down is for the view that shows it.
    expect(screen.queryByRole("button", { name: "我已记下" })).not.toBeInTheDocument();
    expect(screen.getByTestId("sync-key-reminder")).toBeInTheDocument();
  });
});
