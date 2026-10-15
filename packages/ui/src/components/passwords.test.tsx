// The password page's components: the row, the tags (input and filter), the slider, the protection
// cards, the placeholders and the detail rows.
import type { ItemView, Protection } from "@lockra/shared";
import { mockItemView } from "@lockra/shared/mock";
import { act, fireEvent, render, renderHook, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ReactNode, useState } from "react";
import { I18nProvider } from "../i18n/I18nProvider";
import { DetailSection, FieldRow, KeyValueRow, UrlRow } from "./DetailRows";
import { useItemFilterChips } from "./FilterChips";
import { PasswordRow } from "./PasswordRow";
import { PROTECTION_ORDER, ProtectionChoice } from "./ProtectionChoice";
import { SKELETON_DELAY_MS, SkeletonRows } from "./Skeleton";
import { Slider } from "./Slider";
import { TagFilter } from "./TagFilter";
import { TagsInput } from "./TagsInput";

function login(fields: Partial<ItemView> = {}): ItemView {
  return mockItemView("GitHub", {
    username: "octocat",
    has_password: true,
    urls: [{ url: "https://github.com/login", matching: "domain" }],
    ...fields,
  });
}

describe("PasswordRow", () => {
  it("regression: the app's shortcuts get through its buttons, the list's keys do not", async () => {
    // The smoke test, 2026-10-15: with the copy button focused, Ctrl+, opened no settings.
    const user = userEvent.setup();
    const heard: string[] = [];
    const onKey = (event: KeyboardEvent) =>
      heard.push(`${event.ctrlKey ? "Ctrl+" : ""}${event.key}`);
    window.addEventListener("keydown", onKey);
    try {
      render(
        <PasswordRow
          item={login()}
          onSelect={vi.fn()}
          onCopyPassword={vi.fn()}
          onCopyUsername={vi.fn()}
        />,
      );
      screen.getByTestId("row-copy-password").focus();
      await user.keyboard("{Control>},{/Control}");
      await user.keyboard("/");
      await user.keyboard("{ArrowDown}");
    } finally {
      window.removeEventListener("keydown", onKey);
    }
    expect(heard).toEqual(expect.arrayContaining(["Ctrl+,", "/"]));
    expect(heard).not.toContain("ArrowDown");
  });

  it("shows the login, its marks and its site; a click selects, Enter copies the password", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const onCopyPassword = vi.fn();
    const onCopyUsername = vi.fn();
    render(
      <PasswordRow
        item={login({
          favorite: true,
          protection: "confirm",
          totp_entry: "e1",
          conflicts: [{ field: "f", purpose: "password", label: "" }],
        })}
        onSelect={onSelect}
        onCopyPassword={onCopyPassword}
        onCopyUsername={onCopyUsername}
      />,
    );
    const row = screen.getByRole("option");
    expect(row).toHaveAttribute("aria-selected", "false");
    expect(row).toHaveTextContent("GitHub");
    expect(row).toHaveTextContent("octocat");
    expect(screen.getByTestId("row-host")).toHaveTextContent("github.com");
    expect(screen.getByTestId("row-favorite-mark")).toBeInTheDocument();
    expect(screen.getByTestId("row-protected")).toHaveAccessibleName("填充前验证");
    expect(row).toHaveTextContent("验证码");
    expect(row).toHaveTextContent("冲突");
    await user.click(row);
    expect(onSelect).toHaveBeenCalledTimes(1);
    row.focus();
    await user.keyboard("{Enter}");
    expect(onCopyPassword).toHaveBeenCalledTimes(1);
    await user.keyboard(" ");
    expect(onSelect).toHaveBeenCalledTimes(2);
    // The buttons do their own thing, not the row's.
    await user.click(screen.getByRole("button", { name: "复制用户名" }));
    expect(onCopyUsername).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledTimes(2);
  });

  it("says what was just copied in place of the site; a login without a password copies none", async () => {
    const user = userEvent.setup();
    const onCopyPassword = vi.fn();
    render(
      <PasswordRow
        item={login({ has_password: false, read_only: true })}
        selected
        copied="已复制用户名"
        onSelect={vi.fn()}
        onCopyPassword={onCopyPassword}
        onCopyUsername={vi.fn()}
      />,
    );
    expect(screen.getByTestId("row-copied")).toHaveTextContent("已复制用户名");
    expect(screen.queryByTestId("row-host")).toBeNull();
    expect(screen.queryByTestId("row-copy-password")).toBeNull();
    expect(screen.getByRole("option")).toHaveTextContent("只读");
    screen.getByRole("option").focus();
    await user.keyboard("{Enter}");
    expect(onCopyPassword).not.toHaveBeenCalled();
  });

  it("is a card of its own on the phone, its password button the larger one", () => {
    render(
      <PasswordRow
        card
        item={login()}
        onSelect={vi.fn()}
        onCopyPassword={vi.fn()}
        onCopyUsername={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("row-copy-username")).toBeNull();
    expect(screen.queryByTestId("row-host")).toBeNull();
    expect(screen.getByTestId("row-copy-password")).toBeInTheDocument();
  });
});

function Tags({ start = [] as string[], suggestions = [] as string[] }) {
  const [value, setValue] = useState<string[]>(start);
  return (
    <TagsInput
      label="标签"
      value={value}
      onChange={setValue}
      suggestions={suggestions}
      placeholder="输入后按回车添加"
      data-testid="tags"
    />
  );
}

describe("TagsInput", () => {
  it("adds on Enter, a comma or leaving the field, once whatever the case; removes with × or Backspace", async () => {
    const user = userEvent.setup();
    render(<Tags suggestions={["开发", "常用"]} />);
    const input = screen.getByRole("combobox", { name: "标签" });
    expect(input).toHaveAttribute("placeholder", "输入后按回车添加");
    await user.type(input, "Work{Enter}");
    await user.type(input, "home,");
    await user.type(input, "WORK{Enter}");
    await user.type(input, "常用");
    await user.tab();
    expect(screen.getAllByTestId("tag-pill").map((p) => p.textContent)).toEqual([
      "Work",
      "home",
      "常用",
    ]);
    // A suggestion already chosen is not offered again.
    expect(
      [...document.querySelectorAll("datalist option")].map((o) => o.getAttribute("value")),
    ).toEqual(["开发"]);
    await user.click(screen.getByRole("button", { name: "删除标签「home」" }));
    expect(screen.getAllByTestId("tag-pill").map((p) => p.textContent)).toEqual(["Work", "常用"]);
    await user.click(input);
    await user.keyboard("{Backspace}");
    expect(screen.getAllByTestId("tag-pill").map((p) => p.textContent)).toEqual(["Work"]);
    // Enter on an empty field adds nothing (and does not submit a form for it).
    await user.keyboard("{Enter}");
    expect(screen.getAllByTestId("tag-pill")).toHaveLength(1);
  });
});

describe("TagFilter", () => {
  function Filter() {
    const [value, setValue] = useState<string[]>([]);
    return (
      <TagFilter
        tags={[
          { tag: "开发", count: 3 },
          { tag: "常用", count: 2 },
        ]}
        value={value}
        onChange={setValue}
        data-testid="tags"
      />
    );
  }

  it("ticks tags in its menu, shows them on its button and clears them with ×", async () => {
    const user = userEvent.setup();
    render(<Filter />);
    const button = screen.getByTestId("tags-menu");
    expect(button).toHaveTextContent("标签");
    await user.click(button);
    await user.click(screen.getByRole("menuitemcheckbox", { name: "开发 · 3" }));
    expect(button).toHaveTextContent("开发");
    await user.click(button);
    await user.click(screen.getByRole("menuitemcheckbox", { name: "常用 · 2" }));
    expect(button).toHaveTextContent("开发 +1");
    await user.click(button);
    expect(screen.getByRole("menuitemcheckbox", { name: "开发 · 3" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await user.click(screen.getByRole("menuitemcheckbox", { name: "开发 · 3" }));
    expect(button).toHaveTextContent("常用");
    await user.click(screen.getByTestId("tags-clear"));
    expect(button).toHaveTextContent("标签");
    expect(screen.queryByTestId("tags-clear")).toBeNull();
  });

  it("is off with no tags to choose", () => {
    render(<TagFilter tags={[]} value={[]} onChange={vi.fn()} data-testid="tags" />);
    expect(screen.getByTestId("tags-menu")).toBeDisabled();
  });
});

describe("Slider", () => {
  it("moves on the track and takes a typed number, kept within its range", () => {
    const onChange = vi.fn();
    render(
      <Slider label="长度" value={20} min={4} max={128} onChange={onChange} data-testid="length" />,
    );
    fireEvent.change(screen.getByTestId("length-range"), { target: { value: "32" } });
    expect(onChange).toHaveBeenLastCalledWith(32);
    fireEvent.change(screen.getByTestId("length-input"), { target: { value: "500" } });
    expect(onChange).toHaveBeenLastCalledWith(128);
    fireEvent.change(screen.getByTestId("length-input"), { target: { value: "1" } });
    expect(onChange).toHaveBeenLastCalledWith(4);
    const calls = onChange.mock.calls.length;
    fireEvent.change(screen.getByTestId("length-input"), { target: { value: "" } });
    expect(onChange).toHaveBeenCalledTimes(calls);
  });

  it("can go without its number field", () => {
    render(
      <Slider
        label="单词数"
        value={6}
        min={3}
        max={20}
        withInput={false}
        onChange={vi.fn()}
        data-testid="words"
      />,
    );
    expect(screen.queryByTestId("words-input")).toBeNull();
    expect(screen.getByRole("slider", { name: "单词数" })).toHaveValue("6");
  });
});

describe("ProtectionChoice", () => {
  function Choice() {
    const [value, setValue] = useState<Protection>("fill");
    return <ProtectionChoice value={value} onChange={setValue} />;
  }

  it("is a radio group of the three levels; a click or the arrows choose", async () => {
    const user = userEvent.setup();
    render(<Choice />);
    const group = screen.getByRole("radiogroup", { name: "保护级别" });
    const radios = within(group).getAllByRole("radio");
    expect(radios.map((r) => r.dataset.level)).toEqual([...PROTECTION_ORDER]);
    expect(radios[0]).toHaveAttribute("aria-checked", "true");
    expect(radios[0]).toHaveTextContent("浏览器扩展直接填充");
    await user.click(within(group).getByRole("radio", { name: /不交给扩展/ }));
    expect(within(group).getByRole("radio", { name: /不交给扩展/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    // Past the last, the first again.
    await user.keyboard("{ArrowDown}");
    expect(within(group).getByRole("radio", { name: /直接填充/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(within(group).getByRole("radio", { name: /直接填充/ })).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(within(group).getByRole("radio", { name: /不交给扩展/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await user.keyboard("{ArrowUp}");
    expect(within(group).getByRole("radio", { name: /填充前验证/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await user.keyboard("a");
    expect(within(group).getByRole("radio", { name: /填充前验证/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });
});

describe("SkeletonRows", () => {
  it("shows its placeholders only once the load took longer than the delay", () => {
    vi.useFakeTimers();
    try {
      render(<SkeletonRows rows={3} label="正在加载登录项" />);
      expect(screen.queryByTestId("skeleton")).toBeNull();
      act(() => {
        vi.advanceTimersByTime(SKELETON_DELAY_MS);
      });
      expect(screen.getByRole("status", { name: "正在加载登录项" }).children).toHaveLength(3);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("DetailRows", () => {
  it("lays out a section of fields, key–value rows and sites", () => {
    render(
      <DetailSection title="登录信息" aside={<span>120 秒后自动隐藏</span>} data-testid="section">
        <FieldRow label="用户名" actions={<button type="button">复制</button>} data-testid="field">
          octocat
        </FieldRow>
        <FieldRow label="密码" secret>
          ••••
        </FieldRow>
        <KeyValueRow label="分组" data-testid="kv">
          工作
        </KeyValueRow>
        <UrlRow url="https://github.com" matching="host" />
      </DetailSection>,
    );
    const section = within(screen.getByTestId("section"));
    expect(section.getByRole("heading", { name: "登录信息" })).toBeInTheDocument();
    expect(section.getByText("120 秒后自动隐藏")).toBeInTheDocument();
    expect(screen.getByTestId("field")).toHaveTextContent("用户名octocat复制");
    expect(section.getByText("••••")).toHaveClass("mono");
    expect(screen.getByTestId("kv")).toHaveTextContent("分组工作");
    expect(screen.getByTestId("url-row")).toHaveTextContent("https://github.com主机");
  });
});

describe("useItemFilterChips", () => {
  it("offers 全部, 收藏, each group and 未分组 with their counts", () => {
    const items = [
      mockItemView("A", { favorite: true, group: "工作" }),
      mockItemView("B", { group: "工作" }),
      mockItemView("C"),
    ];
    const { result } = renderHook(() => useItemFilterChips(items));
    expect(
      result.current.map(
        (c) =>
          `${c.label} ${c.count}${c.icon === undefined ? "" : ` ${c.icon}`}${c.userText ? " user" : ""}`,
      ),
    ).toEqual(["全部 3", "收藏 1 star", "工作 2 user", "未分组 1"]);
  });

  it("calls the favourites what the logins' menu calls them, in English too", () => {
    const items = [mockItemView("A", { favorite: true })];
    const { result } = renderHook(() => useItemFilterChips(items), {
      wrapper: ({ children }: { children: ReactNode }) => (
        <I18nProvider locale="en">{children}</I18nProvider>
      ),
    });
    expect(result.current.map((c) => c.label)).toEqual(["All", "Favorites"]);
  });
});
