import { MOCK_PASSWORD } from "@lockra/shared/mock";
import { act, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";

async function openBackup(user: ReturnType<typeof renderApp>["user"]) {
  await user.click(screen.getByRole("button", { name: "备份" }));
  await screen.findByTestId("page-backup");
}

describe("Backup", () => {
  it("saves a backup under the master password", async () => {
    const { user } = renderApp();
    await ready();
    await openBackup(user);
    await user.click(screen.getByTestId("backup-save"));
    expect(await screen.findByText("备份已保存：lockra-backup.lockrabackup")).toBeInTheDocument();
    expect(screen.getByTestId("last-backup")).toHaveTextContent("上次备份 刚刚");
  });

  it("saves under a separate password once it is long enough and repeated", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openBackup(user);
    const saveBackup = vi.spyOn(backend, "saveBackup");
    const manual = within(screen.getByTestId("backup-manual"));
    await user.click(manual.getByRole("switch", { name: "使用单独的备份密码" }));
    expect(manual.getByTestId("backup-save")).toBeDisabled();
    await user.type(manual.getByLabelText("备份密码"), "family vault 2026");
    await user.type(manual.getByLabelText("再输入一次"), "family vault");
    expect(manual.getByText("两次输入的密码不一致")).toBeInTheDocument();
    await user.type(manual.getByLabelText("再输入一次"), " 2026");
    await user.click(manual.getByTestId("backup-save"));
    expect(saveBackup).toHaveBeenCalledWith("family vault 2026");
    expect(manual.getByLabelText("备份密码")).toHaveValue("");
  });

  it("turns automatic backups on after a folder is chosen, and runs one", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openBackup(user);
    const auto = within(screen.getByTestId("backup-auto"));
    expect(auto.getByTestId("backup-dir")).toHaveTextContent("未选择");
    expect(auto.getByRole("button", { name: "立即自动备份" })).toBeDisabled();
    await user.click(auto.getByRole("switch", { name: "自动备份" }));
    expect(await auto.findByText("/home/user/OneDrive/Lockra")).toBeInTheDocument();
    expect(auto.getByRole("switch", { name: "自动备份" })).toHaveAttribute("aria-checked", "true");
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { auto_backup: { enabled: true, dir: "/home/user/OneDrive/Lockra" } },
    });
    await user.selectOptions(auto.getByRole("combobox", { name: "保留份数" }), "20");
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { auto_backup: { keep: 20 } },
    });
    await user.click(auto.getByRole("button", { name: "立即自动备份" }));
    expect(await auto.findByTestId("auto-last")).toHaveTextContent(
      "lockra-auto-20261001-081500.lockrabackup",
    );
    await user.click(auto.getByRole("switch", { name: "自动备份" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { auto_backup: { enabled: false } },
    });
    act(() => backend.failAutoBackup("backup_dir_unavailable"));
    expect(auto.getByTestId("auto-error")).toHaveTextContent(
      "自动备份失败（刚刚）：备份文件夹无法写入",
    );
    await user.click(auto.getByRole("button", { name: "选择文件夹…" }));
  });

  it("merges a backup through the import preview", async () => {
    const { user } = renderApp();
    await ready();
    await openBackup(user);
    const restore = within(screen.getByTestId("backup-restore"));
    await user.click(restore.getByRole("button", { name: "选择备份文件…" }));
    expect(await restore.findByTestId("restore-file")).toHaveTextContent(
      "lockra-auto-20260928-091500.lockrabackup",
    );
    await user.type(restore.getByLabelText("备份密码"), "wrong{Enter}");
    expect(await restore.findByText("密码错误")).toBeInTheDocument();
    await user.type(restore.getByLabelText("备份密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await screen.findByTestId("page-import")).toBeInTheDocument();
    expect(screen.getByTestId("import-preview")).toBeInTheDocument();
  });

  it("replaces every account with a backup's", async () => {
    const { user } = renderApp();
    await ready();
    await openBackup(user);
    const restore = within(screen.getByTestId("backup-restore"));
    await user.click(restore.getByRole("button", { name: "选择备份文件…" }));
    await user.click(await restore.findByRole("radio", { name: "替换现有账号" }));
    await user.type(restore.getByLabelText("备份密码"), MOCK_PASSWORD);
    await user.click(restore.getByRole("button", { name: "恢复" }));
    expect(await screen.findByText("已恢复，共 3 个账号")).toBeInTheDocument();
    await user.click(restore.getByRole("button", { name: "选择备份文件…" }));
    await user.click(await restore.findByRole("button", { name: "取消" }));
    expect(await restore.findByRole("button", { name: "选择备份文件…" })).toBeInTheDocument();
  });
});
