import { MOCK_PASSWORD, MockBackend, sampleEntries, sampleItems } from "@lockra/shared/mock";
import { act, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";

async function openExport(user: ReturnType<typeof renderApp>["user"]) {
  await user.click(screen.getByRole("button", { name: "导出" }));
  await screen.findByTestId("page-export");
}

function checkbox(name: string): HTMLElement {
  return within(screen.getByTestId("export-entries")).getByRole("checkbox", { name });
}

describe("Export", () => {
  it("ticks every account that fits the target and says why the others do not", async () => {
    const { user } = renderApp();
    await ready();
    await openExport(user);
    expect(screen.getByTestId("export-count")).toHaveTextContent("已选 7 个");
    expect(checkbox("Game: player-one")).toBeDisabled();
    expect(screen.getByText("不能导出：周期不是 30 秒")).toBeInTheDocument();
    await user.click(screen.getByRole("option", { name: "Microsoft Authenticator" }));
    expect(screen.getByTestId("export-count")).toHaveTextContent("已选 4 个");
    expect(screen.getByText("不能导出：算法不是 SHA1")).toBeInTheDocument();
    expect(screen.getByText("不能导出：位数不是 6")).toBeInTheDocument();
    expect(screen.getByText("不能导出：基于计数器（HOTP）")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "全不选" }));
    expect(screen.getByTestId("export-count")).toHaveTextContent("已选 0 个");
    await user.click(checkbox("GitHub: octocat"));
    expect(screen.getByTestId("export-count")).toHaveTextContent("已选 1 个");
    await user.click(screen.getByRole("button", { name: "全选" }));
    expect(screen.getByTestId("export-count")).toHaveTextContent("已选 4 个");
    await user.click(checkbox("GitHub: octocat"));
    expect(screen.getByTestId("export-count")).toHaveTextContent("已选 3 个");
  });

  it("shows Google's migration codes behind the master password, with the codes to check", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openExport(user);
    const start = screen.getByTestId("export-start");
    expect(start).toBeDisabled();
    await user.type(screen.getByLabelText("主密码"), "wrong{Enter}");
    expect(await screen.findByText("密码错误")).toBeInTheDocument();
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    const viewer = within(await screen.findByRole("dialog", { name: "Google 身份验证器" }));
    expect(await viewer.findByText("第 1 / 1 张")).toBeInTheDocument();
    expect(viewer.getByTestId("export-verify").querySelectorAll("li")).toHaveLength(7);
    expect(viewer.getByTestId("export-countdown")).toHaveTextContent(/二维码将在 1[12]\d 秒后隐藏/);
    expect(screen.getByLabelText("主密码")).toHaveValue("");
    await user.click(viewer.getByTestId("export-finish"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(backend.calls.at(-1)).toMatchObject({ command: "export_close" });
  });

  it("pages through one code per account for Microsoft, and lists what was left out", async () => {
    const { user } = renderApp();
    await ready();
    await openExport(user);
    await user.click(screen.getByRole("option", { name: "Microsoft Authenticator" }));
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    const viewer = within(await screen.findByRole("dialog", { name: "Microsoft Authenticator" }));
    expect(await viewer.findByText("第 1 / 4 张")).toBeInTheDocument();
    expect(viewer.getByRole("button", { name: "上一张" })).toBeDisabled();
    await user.click(viewer.getByTestId("export-next"));
    expect(await viewer.findByText("第 2 / 4 张")).toBeInTheDocument();
    await user.click(viewer.getByRole("button", { name: "上一张" }));
    expect(await viewer.findByText("第 1 / 4 张")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("closes when the core expires the session", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openExport(user);
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    await screen.findByRole("dialog", { name: "Google 身份验证器" });
    const started = backend.calls.find((c) => c.command === "export_page");
    if (started?.command !== "export_page") throw new Error("no page asked for");
    act(() => backend.emitNotice({ type: "export_expired", session: "another" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    act(() => backend.emitNotice({ type: "export_expired", session: started.session }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(await screen.findAllByText("二维码已过期并隐藏")).not.toHaveLength(0);
  });

  it("writes a plain otpauth file only after the warning is accepted", async () => {
    const { user } = renderApp();
    await ready();
    await openExport(user);
    await user.click(screen.getByRole("option", { name: "otpauth 列表文件" }));
    expect(screen.getByTestId("export-count")).toHaveTextContent("已选 8 个");
    await user.type(screen.getByLabelText("主密码"), MOCK_PASSWORD);
    expect(screen.getByTestId("export-start")).toBeDisabled();
    await user.click(screen.getByRole("switch", { name: "我知道这个文件是明文" }));
    await user.click(screen.getByTestId("export-start"));
    expect(await screen.findByTestId("export-saved")).toHaveTextContent(
      "已保存：lockra-export.txt",
    );
  });
});

/** The app with password management on, the sample accounts and their twelve logins. */
function renderLogins() {
  const entries = sampleEntries();
  const backend = new MockBackend({
    entries,
    passwords: true,
    items: sampleItems(entries),
    settings: { locale: "zh-cn" },
  });
  return renderApp({ backend });
}

async function openLogins(user: ReturnType<typeof renderApp>["user"]) {
  await openExport(user);
  await user.click(
    within(screen.getByTestId("export-kind")).getByRole("radio", { name: "登录项" }),
  );
  await screen.findByTestId("export-logins");
}

function login(name: string): HTMLElement {
  return within(screen.getByTestId("export-logins")).getByRole("checkbox", { name });
}

describe("Export of logins", () => {
  it("is not offered without password management", async () => {
    const { user } = renderApp();
    await ready();
    await openExport(user);
    expect(screen.queryByTestId("export-kind")).not.toBeInTheDocument();
    expect(screen.queryByTestId("export-logins")).not.toBeInTheDocument();
  });

  it("saves the chosen logins as a KeePass database under a password typed twice", async () => {
    const { user, backend } = renderLogins();
    await ready();
    await openLogins(user);
    expect(screen.getByTestId("page-subtitle")).toHaveTextContent("把选中的登录项连同关联的验证码");
    // Every login is ticked, the ones with a code say so.
    const count = screen.getByTestId("export-count");
    expect(count).toHaveTextContent("已选 12 个");
    expect(within(screen.getByTestId("export-logins")).getAllByText("带验证码")).toHaveLength(4);
    await user.click(screen.getByRole("button", { name: "全不选" }));
    expect(count).toHaveTextContent("已选 0 个");
    await user.click(screen.getByRole("button", { name: "全选" }));
    await user.click(login("GitHub: octocat"));
    expect(count).toHaveTextContent("已选 11 个");
    const save = screen.getByTestId("export-items-save");
    await user.type(screen.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.type(screen.getByLabelText("数据库密码"), "a database password");
    await user.type(screen.getByLabelText("再输入一次"), "a database passwor");
    expect(screen.getByText("两次输入的密码不一致")).toBeInTheDocument();
    expect(save).toBeDisabled();
    await user.type(screen.getByLabelText("再输入一次"), "d");
    await user.click(save);
    expect(await screen.findByTestId("export-items-saved")).toHaveTextContent(
      "已保存：lockra-logins.kdbx",
    );
    const exported = backend.exportedItems.at(-1);
    expect(exported?.format).toBe("kdbx");
    expect(exported?.ids).toHaveLength(11);
    expect(screen.getByLabelText("主密码")).toHaveValue("");
    expect(screen.getByLabelText("数据库密码")).toHaveValue("");
  });

  it("writes a plain CSV only once its risk is accepted, and says why it did not", async () => {
    const { user, backend } = renderLogins();
    await ready();
    await openLogins(user);
    await user.click(screen.getByRole("option", { name: "明文 CSV 文件" }));
    expect(screen.getByText(/任何拿到这个文件的人都能看到其中的全部密码/)).toBeInTheDocument();
    expect(screen.queryByLabelText("数据库密码")).not.toBeInTheDocument();
    const save = screen.getByTestId("export-items-save");
    await user.type(screen.getByLabelText("主密码"), "wrong");
    expect(save).toBeDisabled();
    await user.click(
      screen.getByRole("switch", { name: "我知道这个文件不加密，任何人都能读到其中的密码" }),
    );
    await user.click(save);
    expect(await screen.findByText("密码错误")).toBeInTheDocument();
    expect(backend.exportedItems).toEqual([]);
    await user.type(screen.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(save);
    expect(await screen.findByTestId("export-items-saved")).toHaveTextContent(
      "已保存：lockra-logins.csv",
    );
    expect(backend.exportedItems.at(-1)?.format).toBe("csv");
  });
});
