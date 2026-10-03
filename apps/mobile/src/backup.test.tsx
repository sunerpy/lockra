import type { EventListener, UiEvent } from "@lockra/shared";
import { MOCK_PASSWORD, MockBackend, sampleEntries } from "@lockra/shared/mock";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

afterEach(async () => {
  cleanup();
  await waitFor(() => {
    if (depthOf(history.state) !== 0) throw new Error("the history still holds pages");
  });
});

/** The mock, with its events held back while `holding`: on the phone a command's answer and the
 *  state event come by different routes, in no fixed order. */
class LateEvents extends MockBackend {
  holding = false;
  private held: UiEvent[] = [];
  private subscribers = new Set<EventListener>();
  constructor(...args: ConstructorParameters<typeof MockBackend>) {
    super(...args);
    super.on((event) => {
      if (this.holding) this.held.push(event);
      else for (const listener of this.subscribers) listener(event);
    });
  }
  override on(listener: EventListener): () => void {
    this.subscribers.add(listener);
    return () => this.subscribers.delete(listener);
  }
  deliver(): void {
    this.holding = false;
    const events = this.held.splice(0);
    for (const event of events) for (const listener of this.subscribers) listener(event);
  }
}

describe("backups on the phone", () => {
  it("are saved under a password of their own when asked", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("codes-settings"));
    await user.click(await screen.findByTestId("settings-backup"));
    expect(await screen.findByTestId("page-backup")).toBeInTheDocument();
    await user.click(within(screen.getByTestId("backup-separate")).getByRole("switch"));
    const save = screen.getByTestId("backup-save");
    await user.type(screen.getByLabelText("备份密码"), "a backup password");
    await user.type(screen.getByLabelText("再输入一次"), "a backup passwor");
    expect(screen.getByText("两次输入的密码不一致")).toBeInTheDocument();
    expect(save).toBeDisabled();
    await user.type(screen.getByLabelText("再输入一次"), "d");
    await user.click(save);
    expect(await screen.findByTestId("page-settings")).toBeInTheDocument();
    expect((await backend.getState()).backup.last_backup_ms).not.toBeNull();
  });

  it("are restored account by account through the import preview", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("codes-settings"));
    await user.click(await screen.findByTestId("settings-restore"));
    expect(await screen.findByTestId("page-restore")).toBeInTheDocument();
    expect(screen.getByTestId("restore-file")).toHaveTextContent("lockra-auto-20260928-091500");
    await user.type(screen.getByLabelText("备份密码"), "wrong{Enter}");
    expect(await screen.findByText("密码错误")).toBeInTheDocument();
    await user.type(screen.getByLabelText("备份密码"), `${MOCK_PASSWORD}{Enter}`);
    // Merged: the backup's accounts wait in the import preview, in the restore page's place.
    expect(await screen.findByTestId("page-preview")).toBeInTheDocument();
    expect(depthOf(history.state)).toBe(2);
    expect(backend.calls.at(-1)).toMatchObject({ command: "restore_commit", mode: "merge" });
  });

  it("are restored in place of every account, or left", async () => {
    const { user, backend } = renderApp();
    await ready();
    await user.click(screen.getByTestId("codes-settings"));
    await user.click(await screen.findByTestId("settings-restore"));
    await user.click(await screen.findByRole("button", { name: "取消" }));
    expect(await screen.findByTestId("page-settings")).toBeInTheDocument();
    expect((await backend.getState()).restore).toBeNull();
    await user.click(screen.getByTestId("settings-restore"));
    await screen.findByTestId("page-restore");
    await user.click(screen.getByRole("radio", { name: "替换现有账号" }));
    await user.type(screen.getByLabelText("备份密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({ command: "restore_commit", mode: "replace" });
  });

  it("make the vault of a new phone", async () => {
    const backend = new MockBackend({ phase: "no_vault", settings: { locale: "zh-cn" } });
    const { user } = renderApp({ backend });
    await ready();
    const restore = within(screen.getByTestId("welcome-restore"));
    await user.click(restore.getByRole("button", { name: "选择备份文件…" }));
    expect(await restore.findByTestId("restore-file")).toBeInTheDocument();
    await user.type(restore.getByLabelText("备份密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({ command: "restore_commit", mode: "replace" });
  });
});

describe("a page whose state comes after the command's answer", () => {
  it("waits for it rather than closing", async () => {
    const backend = new LateEvents({ entries: sampleEntries(), settings: { locale: "zh-cn" } });
    const { user } = renderApp({ backend });
    await ready();
    await user.click(screen.getByTestId("codes-add"));
    await user.click(await screen.findByTestId("add-links"));
    await user.paste("otpauth://totp/Late:me@example.com?secret=MFRGGZDF&issuer=Late");
    backend.holding = true;
    await user.click(screen.getByRole("button", { name: "读取" }));
    // The answer is in, the preview's state not yet: the page waits, still one page deep.
    await waitFor(() => expect(depthOf(history.state)).toBe(2));
    act(() => backend.deliver());
    expect(await screen.findByTestId("page-preview")).toBeInTheDocument();
    expect(depthOf(history.state)).toBe(2);
  });
});
