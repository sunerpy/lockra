import { MOCK_PASSWORD, MockBackend, sampleEntries } from "@lockra/shared/mock";
import { act, screen, within } from "@testing-library/react";
import { ready, renderApp } from "./test/render";

function names(): string[] {
  return screen
    .getAllByTestId("entry-row")
    .map((row) => row.querySelector(".truncate")?.textContent ?? "");
}

/** The page as the system hides it (another app in front, the screen off). */
function leaveTheScreen() {
  Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
  act(() => {
    document.dispatchEvent(new Event("visibilitychange"));
  });
}

afterEach(() => {
  Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
});

describe("the phone app", () => {
  it("creates a vault with a master password typed twice", async () => {
    const backend = new MockBackend({ phase: "no_vault", settings: { locale: "zh-cn" } });
    const { user } = renderApp({ backend });
    await ready();
    const create = screen.getByRole("button", { name: "创建保险库" });
    expect(create).toBeDisabled();
    await user.type(screen.getByLabelText("主密码"), "a long pass phrase");
    await user.type(screen.getByLabelText("再输入一次"), "a long pass phras");
    expect(screen.getByText("两次输入的密码不一致")).toBeInTheDocument();
    await user.type(screen.getByLabelText("再输入一次"), "e");
    await user.click(create);
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
    expect(backend.calls).toContainEqual({
      command: "vault_create",
      password: "a long pass phrase",
    });
    expect(screen.getByText("还没有账号")).toBeInTheDocument();
    expect(
      screen.getByText("即将支持在手机上扫描二维码、读取截图和手动添加账号。"),
    ).toBeInTheDocument();
  });

  it("unlocks with the master password and counts the wrong ones", async () => {
    const backend = new MockBackend({
      entries: sampleEntries(),
      phase: "locked",
      settings: { locale: "zh-cn" },
    });
    const { user } = renderApp({ backend });
    await ready();
    const field = screen.getByLabelText("主密码");
    await user.type(field, "nope{Enter}");
    expect(await screen.findByText("密码错误（第 1 次）")).toBeInTheDocument();
    expect(field).toHaveValue("");
    await user.type(field, `${MOCK_PASSWORD}{Enter}`);
    expect(await screen.findByTestId("page-codes")).toBeInTheDocument();
  });

  it("copies a code with a tap, searches, and folds a group", async () => {
    const { user, backend } = renderApp();
    await ready();
    const [first] = screen.getAllByTestId("entry-row");
    if (!first) throw new Error("no row");
    await user.click(first);
    expect(backend.calls.at(-1)).toEqual({ command: "entry_copy", id: first.dataset.entry });
    expect(await screen.findByRole("status")).toHaveTextContent("已复制");
    // A search shows what it finds, in every section.
    await user.type(screen.getByTestId("codes-search"), "aws");
    expect(names()).toEqual(["AWS"]);
    await user.clear(screen.getByTestId("codes-search"));
    // A section folds and opens again.
    const work = screen.getByRole("button", { name: /^工作/ });
    await user.click(work);
    expect(backend.calls.at(-1)).toEqual({ command: "view_collapse_groups", groups: ["工作"] });
    expect(work).toHaveAttribute("aria-expanded", "false");
    expect(names()).not.toContain("AWS");
    await user.click(work);
    expect(backend.calls.at(-1)).toEqual({ command: "view_collapse_groups", groups: [] });
    await user.type(screen.getByTestId("codes-search"), "nothing like it");
    expect(screen.getByText("没有匹配「nothing like it」的账号")).toBeInTheDocument();
  });

  it("shows a single list when no account has a group", async () => {
    renderApp({
      backend: new MockBackend({
        entries: sampleEntries().map((e) => ({ ...e, view: { ...e.view, group: null } })),
        settings: { locale: "zh-cn" },
      }),
    });
    await ready();
    expect(screen.queryAllByTestId("codes-group-toggle")).toHaveLength(0);
    expect(within(screen.getByTestId("codes-list")).getAllByTestId("entry-row")).toHaveLength(8);
  });

  it("locks as the app leaves the screen, and from its button", async () => {
    const { user, backend } = renderApp();
    await ready();
    leaveTheScreen();
    expect(await screen.findByTestId("page-unlock")).toBeInTheDocument();
    expect(backend.calls.at(-1)).toEqual({ command: "vault_lock" });
    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    await screen.findByTestId("page-codes");
    await user.click(screen.getByRole("button", { name: "锁定" }));
    expect(await screen.findByTestId("page-unlock")).toBeInTheDocument();
    // Locked already: leaving again asks for nothing.
    const before = backend.calls.length;
    leaveTheScreen();
    expect(backend.calls).toHaveLength(before);
  });
});
