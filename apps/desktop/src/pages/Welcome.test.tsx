import { MOCK_PASSWORD, MockBackend } from "@lockra/shared/mock";
import { screen, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";

function noVault() {
  return new MockBackend({ phase: "no_vault", settings: { locale: "zh-cn" } });
}

describe("Welcome", () => {
  it("joins a sync space from an invitation under a new master password of its own", async () => {
    const { user, backend } = renderApp({ backend: noVault() });
    await ready();
    const join = within(screen.getByTestId("welcome-join"));
    await user.click(join.getByTestId("welcome-join-open"));
    expect(
      join.getByText("这台电脑上还没有保险库，加入后会用这个主密码创建一个。"),
    ).toBeInTheDocument();
    // No other device's password: this computer's own, typed twice like any new vault's.
    expect(join.queryByLabelText("同步空间的主密码")).not.toBeInTheDocument();
    await user.type(join.getByLabelText("配对链接或邀请码"), "not an invitation");
    await user.type(join.getByLabelText("为这台设备设置主密码"), "a password of its own");
    await user.type(join.getByLabelText("再输入一次"), "a password of its owm");
    expect(join.getByText("两次输入的密码不一致")).toBeInTheDocument();
    expect(join.getByRole("button", { name: "加入" })).toBeDisabled();
    await user.clear(join.getByLabelText("再输入一次"));
    await user.type(join.getByLabelText("再输入一次"), "a password of its own");
    await user.click(join.getByRole("button", { name: "加入" }));
    expect(await join.findByText("不是有效的 Lockra 同步邀请")).toBeInTheDocument();
    // Passwords go after each attempt, as everywhere.
    expect(join.getByLabelText("为这台设备设置主密码")).toHaveValue("");
    await user.clear(join.getByLabelText("配对链接或邀请码"));
    await user.type(join.getByLabelText("配对链接或邀请码"), "lockra-invite:1:abc");
    await user.type(join.getByLabelText("为这台设备设置主密码"), "a password of its own");
    await user.type(join.getByLabelText("再输入一次"), "a password of its own");
    await user.click(join.getByRole("button", { name: "加入" }));
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(backend.calls).toContainEqual({
      command: "sync_join",
      source: { type: "invite", text: "lockra-invite:1:abc" },
      password: "a password of its own",
      device_name: "Linux 电脑",
    });
    expect((await backend.getState()).sync.space?.devices[0]?.name).toBe("Linux 电脑");
  });

  it("recovers a sync space with the recovery key under a master password of the space's", async () => {
    const { user } = renderApp({ backend: noVault() });
    await ready();
    const join = within(screen.getByTestId("welcome-join"));
    await user.click(join.getByTestId("welcome-join-open"));
    await user.click(join.getByRole("radio", { name: "恢复密钥" }));
    expect(join.queryByLabelText("为这台设备设置主密码")).not.toBeInTheDocument();
    expect(join.getByLabelText("同步空间的主密码")).toBeInTheDocument();
    expect(join.queryByLabelText("再输入一次")).not.toBeInTheDocument();
  });

  it("the join form closes again", async () => {
    const { user } = renderApp({ backend: noVault() });
    await ready();
    const join = within(screen.getByTestId("welcome-join"));
    await user.click(join.getByTestId("welcome-join-open"));
    await user.click(join.getByRole("button", { name: "取消" }));
    expect(join.getByTestId("welcome-join-open")).toBeInTheDocument();
  });

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
