import {
  MOCK_PASSWORD,
  MockBackend,
  type MockOptions,
  sampleEntries,
  sampleItems,
} from "@lockra/shared/mock";
import { act, fireEvent, screen, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { ready, renderApp } from "../test/render";

/** The app with password management on and the sample logins. */
function renderGenerator(options: MockOptions = {}) {
  const entries = sampleEntries();
  const backend = new MockBackend({
    entries,
    passwords: true,
    items: sampleItems(entries),
    ...options,
    settings: { locale: "zh-cn", ...options.settings },
  });
  return renderApp({ backend });
}

function nav() {
  return within(screen.getByRole("navigation", { name: "主导航" }));
}

/** The generator page, by the 「生成器」 button in 密码's header. */
async function openPage(user: UserEvent) {
  await ready();
  await user.click(nav().getByRole("button", { name: /^密码/ }));
  const passwords = within(await screen.findByTestId("page-passwords"));
  await user.click(passwords.getByRole("button", { name: "生成器" }));
  return within(await screen.findByTestId("page-generator"));
}

function calls(backend: MockBackend, command: string) {
  return backend.calls.filter((c) => c.command === command);
}

function lastCall(backend: MockBackend, command: string) {
  const found = calls(backend, command);
  return found[found.length - 1];
}

function count(backend: MockBackend, command: string) {
  return calls(backend, command).length;
}

/** The password the page shows, once the generator has made it. */
async function made(page: ReturnType<typeof within>): Promise<string> {
  const field = page.getByTestId("generator-password");
  await vi.waitFor(() => expect(field.textContent).not.toBe("—"));
  return field.textContent ?? "";
}

/** 「查看历史…」, then the master password: the list once it shows. */
async function viewHistory(user: UserEvent, history: ReturnType<typeof within>) {
  await user.click(history.getByTestId("generator-history-view"));
  const verify = within(await screen.findByRole("dialog", { name: "验证身份" }));
  await user.type(verify.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
  return history.findByTestId("generator-history");
}

describe("Generator page", () => {
  it("belongs to 密码: its button in 密码's header, 密码 current in the sidebar, a way back", async () => {
    const { user } = renderGenerator();
    await ready();
    // No sidebar row of its own: the sidebar keeps its rows (DESIGN.md §2).
    const vault = within(nav().getByRole("group", { name: "保险库" }));
    expect(vault.getAllByRole("button").map((b) => b.textContent)).toEqual([
      expect.stringMatching(/^验证码/),
      expect.stringMatching(/^密码/),
    ]);
    await user.click(nav().getByRole("button", { name: /^密码/ }));
    const passwords = within(await screen.findByTestId("page-passwords"));
    expect(passwords.getByRole("button", { name: "生成器" })).toHaveAttribute(
      "title",
      "生成器 · Ctrl G",
    );
    const page = await openPage(user);
    expect(nav().getByRole("button", { name: /^密码/ })).toHaveAttribute("aria-current", "page");
    await user.click(page.getByRole("button", { name: "返回密码" }));
    expect(await screen.findByTestId("page-passwords")).toBeInTheDocument();
    expect(screen.queryByTestId("page-generator")).toBeNull();
  });

  it("is not offered without password management: no sidebar entry, Ctrl G or palette page", async () => {
    const { user } = renderApp();
    await ready();
    expect(nav().queryByRole("button", { name: "生成器" })).toBeNull();
    await user.keyboard("{Control>}g{/Control}");
    expect(screen.getByTestId("page-codes")).toBeInTheDocument();
    expect(screen.queryByTestId("page-generator")).toBeNull();
    await user.keyboard("{Control>}k{/Control}");
    const palette = within(await screen.findByRole("dialog", { name: "命令菜单" }));
    expect(palette.queryByText("生成器")).toBeNull();
  });

  it("makes a password for no site and copies it through Rust, with no form to use it in", async () => {
    const { user, backend } = renderGenerator();
    const page = await openPage(user);
    expect(page.getByRole("heading", { level: 1, name: "密码生成器" })).toBeInTheDocument();
    expect(page.getByTestId("page-subtitle")).toHaveTextContent("生成随机密码或口令");
    await made(page);
    expect(lastCall(backend, "generator_generate")).not.toHaveProperty("domain");
    expect(page.queryByTestId("generator-use")).toBeNull();
    expect(page.queryByTestId("generator-rules")).toBeNull();
    expect(page.getByTestId("generator-history-count")).toHaveTextContent("0 条");
    await user.click(page.getByTestId("generator-copy"));
    expect(lastCall(backend, "generator_copy")).toBeDefined();
    expect(page.getByTestId("generator-copy")).toHaveTextContent("已复制");
    expect(lastCall(backend, "settings_set")).toMatchObject({
      settings: { generator: { kind: "password", length: 20 } },
    });
    // Copied, it is in the history.
    expect(page.getByTestId("generator-history-count")).toHaveTextContent("1 条");
    // A new password is not copied yet.
    await user.click(page.getByTestId("generator-regenerate"));
    await vi.waitFor(() =>
      expect(page.getByTestId("generator-copy")).not.toHaveTextContent("已复制"),
    );
  });

  it("asks the generator for what its options say", async () => {
    const { user, backend } = renderGenerator();
    const page = await openPage(user);
    await made(page);
    const asked = () => lastCall(backend, "generator_generate");
    fireEvent.change(page.getByTestId("generator-length-range"), { target: { value: "32" } });
    await user.click(page.getByTestId("generator-symbols"));
    await user.click(page.getByRole("switch", { name: "避免易混字符" }));
    await user.type(page.getByTestId("generator-exclude"), "0O");
    await vi.waitFor(() =>
      expect(asked()).toMatchObject({
        options: {
          kind: "password",
          length: 32,
          symbols: false,
          avoid_ambiguous: true,
          exclude: "0O",
        },
      }),
    );
    // Short and of one kind: the page says it is easy to guess.
    fireEvent.change(page.getByTestId("generator-length-range"), { target: { value: "4" } });
    await user.click(page.getByTestId("generator-upper"));
    await user.click(page.getByTestId("generator-digits"));
    expect(
      await page.findByText("这个密码容易被猜到，建议加长或多用几类字符。"),
    ).toBeInTheDocument();
    // No character at all: nothing to copy, and the reason.
    await user.click(page.getByTestId("generator-lower"));
    expect(await page.findByRole("alert")).toBeInTheDocument();
    expect(page.getByTestId("generator-password")).toHaveTextContent("—");
    expect(page.getByTestId("generator-copy")).toBeDisabled();

    await user.click(page.getByRole("radio", { name: "口令" }));
    fireEvent.change(page.getByTestId("generator-words-range"), { target: { value: "8" } });
    await user.click(page.getByRole("radio", { name: "." }));
    await user.click(page.getByRole("switch", { name: "首字母大写" }));
    await user.click(page.getByRole("switch", { name: "加一个数字" }));
    await vi.waitFor(() =>
      expect(asked()).toMatchObject({
        options: { kind: "passphrase", words: 8, separator: ".", capitalize: true, number: true },
      }),
    );
    expect(page.getByText("词表：EFF 大词表，7776 个英文单词")).toBeInTheDocument();
    await made(page);
    expect(page.queryByRole("alert")).toBeNull();
    await user.click(page.getByTestId("generator-copy"));
    expect(lastCall(backend, "settings_set")).toMatchObject({
      settings: { generator: { kind: "passphrase", words: 8 } },
    });
    expect(count(backend, "generator_copy")).toBe(1);
  });

  it("shows the history after the master password, then hides it after two minutes", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const { user, backend } = renderGenerator();
      await ready();
      // A passphrase made for a site and copied before (in a login's form, say).
      await act(async () => {
        await backend.dispatch({
          command: "generator_generate",
          options: {
            kind: "passphrase",
            words: 4,
            separator: "-",
            capitalize: false,
            number: false,
          },
          domain: "example.com",
        });
        await backend.dispatch({ command: "generator_copy" });
      });
      const page = await openPage(user);
      const password = await made(page);
      await user.click(page.getByTestId("generator-copy"));
      const history = within(page.getByTestId("generator-history-panel"));
      expect(history.getByTestId("generator-history-count")).toHaveTextContent("2 条");
      // The core's limits, and where the history stays.
      expect(history.getByTestId("generator-history-kept")).toHaveTextContent(
        "最多保留最近 100 个，最长 90 天，只保存在这台设备上",
      );
      expect(history.queryByTestId("generator-history")).toBeNull();

      const list = within(await viewHistory(user, history));
      expect(lastCall(backend, "generator_history_reveal")).toMatchObject({
        password: MOCK_PASSWORD,
      });
      const rows = list.getAllByRole("listitem");
      expect(rows).toHaveLength(2);
      expect(rows[0]).toHaveTextContent(password);
      expect(rows[0]).toHaveTextContent("刚刚");
      expect(rows[0]).not.toHaveTextContent("用于");
      expect(rows[1]).toHaveTextContent("刚刚 · 用于 example.com");
      expect(history.getByTestId("generator-history-countdown")).toHaveTextContent(
        "120 秒后自动隐藏",
      );
      act(() => {
        vi.advanceTimersByTime(121_000);
      });
      expect(history.queryByTestId("generator-history")).toBeNull();
      expect(history.getByTestId("generator-history-view")).toBeEnabled();
      // The generated password is still on the screen: the secret view goes on.
      expect(count(backend, "secret_view_closed")).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("hides the history on request and when the page is left, and asks again next time", async () => {
    const { user, backend } = renderGenerator();
    let page = await openPage(user);
    await made(page);
    await user.click(page.getByTestId("generator-copy"));
    let history = within(page.getByTestId("generator-history-panel"));
    // A cancelled verification shows nothing.
    await user.click(history.getByTestId("generator-history-view"));
    await user.click(
      within(await screen.findByRole("dialog", { name: "验证身份" })).getByRole("button", {
        name: "取消",
      }),
    );
    expect(history.queryByTestId("generator-history")).toBeNull();
    expect(count(backend, "generator_history_reveal")).toBe(0);

    await viewHistory(user, history);
    await user.click(history.getByTestId("generator-history-hide"));
    expect(history.queryByTestId("generator-history")).toBeNull();
    expect(count(backend, "secret_view_closed")).toBe(0);

    await viewHistory(user, history);
    await user.click(nav().getByRole("button", { name: /^密码/ }));
    expect(await screen.findByTestId("page-passwords")).toBeInTheDocument();
    expect(count(backend, "secret_view_closed")).toBe(1);

    page = await openPage(user);
    history = within(page.getByTestId("generator-history-panel"));
    expect(history.queryByTestId("generator-history")).toBeNull();
    await viewHistory(user, history);
    expect(count(backend, "generator_history_reveal")).toBe(3);
  });

  it("clears the history", async () => {
    const { user, backend } = renderGenerator();
    const page = await openPage(user);
    await made(page);
    const history = within(page.getByTestId("generator-history-panel"));
    // Nothing to show yet.
    expect(history.getByTestId("generator-history-view")).toBeDisabled();
    await user.click(page.getByTestId("generator-copy"));
    expect(history.getByTestId("generator-history-view")).toBeEnabled();
    await viewHistory(user, history);
    // It asks first: cancelled, nothing goes.
    await user.click(history.getByTestId("generator-history-clear"));
    let confirm = within(
      await screen.findByRole("dialog", { name: "清空生成历史？这些密码不会再显示。" }),
    );
    await user.click(confirm.getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(count(backend, "generator_history_clear")).toBe(0);
    expect(history.getByTestId("generator-history")).toBeInTheDocument();
    await user.click(history.getByTestId("generator-history-clear"));
    confirm = within(
      await screen.findByRole("dialog", { name: "清空生成历史？这些密码不会再显示。" }),
    );
    await user.click(confirm.getByTestId("generator-history-clear-confirm"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(lastCall(backend, "generator_history_clear")).toBeDefined();
    expect(history.queryByTestId("generator-history")).toBeNull();
    expect(history.getByTestId("generator-history-count")).toHaveTextContent("0 条");
    expect(history.getByTestId("generator-history-view")).toBeDisabled();
  });

  it("is a page of the command palette", async () => {
    const { user } = renderGenerator();
    await ready();
    await user.keyboard("{Control>}k{/Control}");
    await screen.findByRole("dialog", { name: "命令菜单" });
    await user.keyboard("生成器{Enter}");
    expect(await screen.findByTestId("page-generator")).toBeInTheDocument();
  });
});

describe("The login form's generator", () => {
  it("stays a dialog: its history after the proof, then 「使用此密码」 fills the form", async () => {
    const { user, backend } = renderGenerator();
    await ready();
    await user.click(nav().getByRole("button", { name: /^密码/ }));
    await screen.findByTestId("page-passwords");
    await user.keyboard("{Control>}n{/Control}");
    const form = within(screen.getByRole("dialog", { name: "新建登录项" }));
    await user.type(form.getByRole("textbox", { name: "网站" }), "example.com");
    await user.click(form.getByTestId("item-form-generate"));
    let generator = within(screen.getByRole("dialog", { name: "生成密码" }));
    const password = await made(generator);
    expect(lastCall(backend, "generator_generate")).toMatchObject({ domain: "example.com" });
    expect(generator.getByTestId("generator-rules")).toHaveTextContent(
      "已按 example.com 的密码规则生成",
    );
    expect(generator.queryByTestId("generator-copy")).toBeNull();
    await user.click(generator.getByTestId("generator-copy-icon"));
    expect(lastCall(backend, "generator_copy")).toBeDefined();
    expect(generator.getByTestId("generator-history-open")).toHaveTextContent("生成历史 · 1 条");

    await user.click(generator.getByTestId("generator-history-open"));
    const verify = within(await screen.findByRole("dialog", { name: "验证身份" }));
    await user.type(verify.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    const history = within(await screen.findByRole("dialog", { name: "生成历史" }));
    expect(history.getByTestId("generator-history")).toHaveTextContent(password);
    expect(history.getByTestId("generator-history")).toHaveTextContent("用于 example.com");
    expect(history.getByTestId("generator-history-countdown")).toHaveTextContent(
      "120 秒后自动隐藏",
    );
    await user.click(history.getByRole("button", { name: "完成" }));

    generator = within(screen.getByRole("dialog", { name: "生成密码" }));
    expect(generator.getByTestId("generator-password")).toHaveTextContent(password);
    await user.click(generator.getByTestId("generator-use"));
    expect(screen.queryByRole("dialog", { name: "生成密码" })).toBeNull();
    expect(form.getByTestId("item-form-password")).toHaveValue(password);
  });

  it("clears the history from the dialog's list", async () => {
    const { user, backend } = renderGenerator();
    await ready();
    await user.click(nav().getByRole("button", { name: /^密码/ }));
    await screen.findByTestId("page-passwords");
    await user.keyboard("{Control>}n{/Control}");
    const form = within(screen.getByRole("dialog", { name: "新建登录项" }));
    await user.click(form.getByTestId("item-form-generate"));
    // The generator's dialog, drawn again each time the history gives way to it.
    const generator = () => within(screen.getByRole("dialog", { name: "生成密码" }));
    await made(generator());
    const open = async () => {
      await user.click(generator().getByTestId("generator-history-open"));
      const verify = within(await screen.findByRole("dialog", { name: "验证身份" }));
      await user.type(verify.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
      return within(await screen.findByRole("dialog", { name: "生成历史" }));
    };
    // Nothing copied yet: an empty history, nothing to clear.
    let history = await open();
    expect(history.getByText("还没有生成过密码")).toBeInTheDocument();
    expect(history.getByTestId("generator-history-clear")).toBeDisabled();
    await user.click(history.getByRole("button", { name: "完成" }));
    await user.click(generator().getByTestId("generator-copy-icon"));
    history = await open();
    await user.click(history.getByTestId("generator-history-clear"));
    await screen.findByRole("dialog", { name: "清空生成历史？这些密码不会再显示。" });
    // Esc closes the question only: the history stays.
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "清空生成历史？这些密码不会再显示。" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "生成历史" })).toBeInTheDocument();
    expect(count(backend, "generator_history_clear")).toBe(0);
    await user.click(history.getByTestId("generator-history-clear"));
    const confirm = within(
      await screen.findByRole("dialog", { name: "清空生成历史？这些密码不会再显示。" }),
    );
    await user.click(confirm.getByRole("button", { name: "清空" }));
    expect(lastCall(backend, "generator_history_clear")).toBeDefined();
    // Back to the generator, its link counting none.
    expect(
      within(screen.getByRole("dialog", { name: "生成密码" })).getByTestId(
        "generator-history-open",
      ),
    ).toHaveTextContent("生成历史 · 0 条");
  });
});
