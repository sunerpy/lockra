import { MOCK_PASSWORD, MockBackend } from "@lockra/shared/mock";
import { screen, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";

function noVault() {
  return new MockBackend({ phase: "no_vault", settings: { locale: "zh-cn" } });
}

describe("Welcome", () => {
  it("creates a vault once the password is long enough and repeated", async () => {
    const { user, backend } = renderApp({ backend: noVault() });
    await ready();
    const create = within(screen.getByTestId("welcome-create"));
    const submit = create.getByRole("button", { name: "创建保险库" });
    expect(submit).toBeDisabled();
    await user.type(create.getByLabelText("主密码"), "short");
    expect(create.getByTestId("password-strength")).toHaveAttribute("data-strength", "tooShort");
    await user.type(create.getByLabelText("主密码"), " but long now");
    await user.type(create.getByLabelText("再输入一次"), "different");
    expect(create.getByText("两次输入的密码不一致")).toBeInTheDocument();
    expect(submit).toBeDisabled();
    await user.clear(create.getByLabelText("再输入一次"));
    await user.type(create.getByLabelText("再输入一次"), "short but long now");
    await user.click(submit);
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(screen.getByText("还没有账号")).toBeInTheDocument();
    expect(backend.calls).toContainEqual({
      command: "vault_create",
      password: "short but long now",
    });
  });

  it("counts characters, not UTF-16 units, as the core does", async () => {
    const { user } = renderApp({ backend: noVault() });
    await ready();
    const create = within(screen.getByTestId("welcome-create"));
    // Seven characters, fourteen UTF-16 units.
    await user.type(create.getByLabelText("主密码"), "😀😀😀😀😀😀😀");
    await user.type(create.getByLabelText("再输入一次"), "😀😀😀😀😀😀😀");
    expect(create.getByRole("button", { name: "创建保险库" })).toBeDisabled();
  });

  it("restores a backup, whose password becomes the master password", async () => {
    const { user } = renderApp({ backend: noVault() });
    await ready();
    const restore = within(screen.getByTestId("welcome-restore"));
    await user.click(restore.getByRole("button", { name: "选择备份文件…" }));
    expect(await restore.findByTestId("restore-file")).toHaveTextContent(
      "lockra-auto-20260928-091500.lockrabackup · 创建于",
    );
    await user.type(restore.getByLabelText("备份密码"), "wrong password");
    await user.click(restore.getByRole("button", { name: "恢复" }));
    expect(await restore.findByText("密码错误")).toBeInTheDocument();
    await user.type(restore.getByLabelText("备份密码"), MOCK_PASSWORD);
    await user.click(restore.getByRole("button", { name: "恢复" }));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(screen.getAllByTestId("entry-row")).toHaveLength(3);
    expect(await screen.findByText("已恢复，共 3 个账号")).toBeInTheDocument();
  });

  it("puts a chosen backup back", async () => {
    const { user, backend } = renderApp({ backend: noVault() });
    await ready();
    const restore = within(screen.getByTestId("welcome-restore"));
    await user.click(restore.getByRole("button", { name: "选择备份文件…" }));
    await user.click(await restore.findByRole("button", { name: "取消" }));
    expect(await restore.findByRole("button", { name: "选择备份文件…" })).toBeInTheDocument();
    expect(backend.calls).toContainEqual({ command: "restore_cancel" });
  });
});
