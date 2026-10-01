import { MOCK_PASSWORD, MockBackend, sampleEntries } from "@lockra/shared/mock";
import { act, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";
import { actionsFor } from "./Import";

async function openImport(user: ReturnType<typeof renderApp>["user"]) {
  await user.click(screen.getByRole("button", { name: /^导入/ }));
  await screen.findByTestId("page-import");
}

describe("actionsFor", () => {
  it("lets new accounts be added or skipped, name clashes also replace, the rest only skip", () => {
    expect(actionsFor({ type: "new" })).toEqual(["add", "skip"]);
    expect(actionsFor({ type: "conflict", entry_id: "x" })).toEqual(["add", "replace", "skip"]);
    expect(actionsFor({ type: "exists", entry_id: "x" })).toEqual([]);
    expect(actionsFor({ type: "unsupported", reason: "md5_algorithm" })).toEqual([]);
  });
});

describe("Import", () => {
  it("shows the four sources with their steps", async () => {
    const { user } = renderApp();
    await ready();
    await openImport(user);
    for (const id of ["source-google", "source-microsoft", "source-text", "source-backup"])
      expect(screen.getByTestId(id)).toBeInTheDocument();
    expect(screen.getByText(/转移账号 → 导出账号/)).toBeInTheDocument();
    expect(screen.getByText(/PhoneFactor-wal/)).toBeInTheDocument();
  });

  it("previews Google export images, says which codes are missing, and imports the chosen ones", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openImport(user);
    await user.click(screen.getByTestId("drop-zone"));
    const preview = within(await screen.findByTestId("import-preview"));
    expect(preview.getByText("Google 导出：已收到 1/2 张 · 还缺第 2 张二维码")).toBeInTheDocument();
    expect(preview.getAllByText("新增")).toHaveLength(2);
    expect(preview.getAllByText("已存在")).toHaveLength(1);
    const commit = preview.getByTestId("import-commit");
    expect(commit).toHaveTextContent("导入 2 个账号");
    await user.selectOptions(preview.getByRole("combobox", { name: "操作 · Dropbox" }), "skip");
    expect(commit).toHaveTextContent("导入 1 个账号");
    await user.click(commit);
    expect(await screen.findByText("已导入 1 个，替换 0 个，跳过 2 个")).toBeInTheDocument();
    expect(screen.queryByTestId("import-preview")).not.toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({
      command: "import_commit",
      choices: [
        { id: 0, action: "skip" },
        { id: 2, action: "add" },
      ],
    });
  });

  it("reads pasted links, marks what it cannot take, and forgets the text", async () => {
    const { user } = renderApp();
    await ready();
    await openImport(user);
    const source = within(screen.getByTestId("source-text"));
    const field = source.getByRole("textbox");
    await user.type(field, "otpauth://totp/Pasted:me?secret=JBSWY3DPEHPK3PXP{Enter}not a link");
    await user.click(source.getByRole("button", { name: "读取" }));
    const preview = within(await screen.findByTestId("import-preview"));
    expect(field).toHaveValue("");
    expect(preview.getByText("Pasted")).toBeInTheDocument();
    expect(preview.getByText("不是 otpauth 链接")).toBeInTheDocument();
    expect(preview.getByText("粘贴的文本 · 第 2 行")).toBeInTheDocument();
    await user.click(preview.getByRole("button", { name: "放弃这次导入" }));
    expect(screen.queryByTestId("import-preview")).not.toBeInTheDocument();
  });

  it("keeps a failed read in the field", async () => {
    const { user } = renderApp();
    await ready();
    await openImport(user);
    const source = within(screen.getByTestId("source-text"));
    await user.type(source.getByRole("textbox"), "# only a comment");
    await user.click(source.getByRole("button", { name: "读取" }));
    expect(await screen.findByText("没有找到可以导入的账号")).toBeInTheDocument();
    expect(source.getByRole("textbox")).toHaveValue("# only a comment");
  });

  it("reads the clipboard", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      settings: { locale: "zh-cn" },
      clipboard: "otpauth://totp/Clip:me?secret=JBSWY3DPEHPK3PXP",
    });
    const { user } = renderApp({ backend });
    await ready();
    await openImport(user);
    await user.click(
      within(screen.getByTestId("page-import")).getByRole("button", { name: "从剪贴板导入" }),
    );
    const preview = within(await screen.findByTestId("import-preview"));
    expect(preview.getByText("剪贴板")).toBeInTheDocument();
  });

  it("asks for the password of a Lockra backup", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openImport(user);
    act(() => backend.awaitBackup("other.lockrabackup"));
    const form = within(await screen.findByTestId("backup-password"));
    await user.type(form.getByLabelText("「other.lockrabackup」需要备份密码"), "wrong{Enter}");
    expect(await form.findByText("密码错误")).toBeInTheDocument();
    await user.type(
      form.getByLabelText("「other.lockrabackup」需要备份密码"),
      `${MOCK_PASSWORD}{Enter}`,
    );
    expect(await screen.findAllByText("other.lockrabackup")).not.toHaveLength(0);
    expect(screen.queryByTestId("backup-password")).not.toBeInTheDocument();
  });

  it("opens the pickers of the other sources", async () => {
    const { user } = renderApp();
    await ready();
    await openImport(user);
    await user.click(screen.getByRole("button", { name: "选择 PhoneFactor 文件…" }));
    expect(await screen.findByTestId("import-preview")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "选择文件…" }));
    await user.click(screen.getByRole("button", { name: "选择备份文件…" }));
    expect(screen.getByTestId("import-preview")).toBeInTheDocument();
  });
});
