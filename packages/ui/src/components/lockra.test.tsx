import { type CodeView, type EntryView } from "@lockra/shared";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { I18nProvider } from "../i18n/I18nProvider";
import { heardByWindow } from "../test/keys";
import { CountdownRing, remaining } from "./CountdownRing";
import { DropZone } from "./DropZone";
import { EntryAvatar, autoColor, initial } from "./EntryAvatar";
import { EntryRow } from "./EntryRow";
import { Logo } from "./Logo";
import { OtpCode } from "./OtpCode";
import { PasswordField, passwordStrength } from "./PasswordField";
import { QrView } from "./QrView";
import { StepList } from "./StepList";

const T = 1_790_000_010_000;

function entry(overrides: Partial<EntryView> = {}): EntryView {
  return {
    id: "e1",
    issuer: "GitHub",
    account: "octocat",
    kind: { type: "totp", period: 30 },
    algorithm: "sha1",
    digits: 6,
    group: null,
    favorite: false,
    color: "auto",
    mark: null,
    origin: "uri",
    created_at_ms: 0,
    updated_at_ms: 0,
    last_used_at_ms: null,
    export: { google: null, microsoft: null },
    ...overrides,
  };
}

function code(overrides: Partial<CodeView> = {}): CodeView {
  return {
    entry_id: "e1",
    code: "492039",
    next_code: "114415",
    valid_from_ms: T - 10_000,
    valid_until_ms: T + 20_000,
    ...overrides,
  };
}

describe("Logo", () => {
  it("sits on the theme's card, not on a dark square: the padlock in the theme's ink and blue", () => {
    // User choice 2026-10-11 (option B, Penpot 「00 · Logo」): the navy tile looked like a hole in
    // 澄空 and 墨白. The plate is the theme's surface with its edge and depth; the colours are tokens.
    const { rerender } = render(<Logo size={28} />);
    const logo = screen.getByTestId("app-logo");
    expect(logo.tagName).toBe("SPAN");
    expect(logo).toHaveClass("bg-surface", "hairline", "panel", "rounded-[22%]");
    expect([logo.style.width, logo.style.height]).toEqual(["28px", "28px"]);
    expect(logo).toHaveAttribute("aria-hidden", "true");
    const svg = logo.querySelector("svg");
    expect(svg?.querySelector("path")).toHaveClass("stroke-fg");
    expect(svg?.querySelector("rect")).toHaveClass("fill-[var(--logo-body)]");
    const dots = svg?.querySelectorAll("circle") ?? [];
    expect(dots).toHaveLength(3);
    for (const dot of dots) expect(dot).toHaveClass("fill-[var(--logo-dots)]");
    // No colour of its own any more.
    expect(logo.outerHTML).not.toMatch(/#[0-9a-f]{6}/i);
    rerender(<Logo size={48} label="Lockra" />);
    expect(screen.getByRole("img", { name: "Lockra" })).toBe(screen.getByTestId("app-logo"));
  });
});

describe("OtpCode", () => {
  it("groups digits, announces them and masks on request", () => {
    const { rerender } = render(<OtpCode code="492039" />);
    expect(screen.getByTestId("otp-code")).toHaveTextContent("492 039");
    expect(screen.getByLabelText("4 9 2 0 3 9")).toBeInTheDocument();
    rerender(<OtpCode code="12345678" tone="warning" size="lg" />);
    expect(screen.getByTestId("otp-code")).toHaveTextContent("1234 5678");
    expect(screen.getByTestId("otp-code")).toHaveClass("text-warning", "text-[32px]");
    rerender(<OtpCode code="492039" masked />);
    expect(screen.getByTestId("otp-code")).toHaveTextContent("••• •••");
    expect(screen.getByTestId("otp-code")).not.toHaveAttribute("aria-label");
    // A card's code takes its own size in pixels.
    rerender(<OtpCode code="492039" size={46} />);
    expect(screen.getByTestId("otp-code").style.fontSize).toBe("46px");
    expect(screen.getByTestId("otp-code").className).not.toMatch(/text-\[\d+px\]/);
  });
});

describe("CountdownRing", () => {
  it("empties over the window and warns in the last five seconds", () => {
    expect(remaining(0, 30_000, 15_000)).toEqual({ fraction: 0.5, seconds: 15 });
    expect(remaining(0, 30_000, 40_000)).toEqual({ fraction: 0, seconds: 0 });
    expect(remaining(0, 0, 0).fraction).toBe(0);
    const { rerender } = render(
      <CountdownRing validFromMs={0} validUntilMs={30_000} nowMs={10_000} />,
    );
    const ring = screen.getByTestId("countdown-ring");
    expect(ring).toHaveAccessibleName("剩余时间 20");
    expect(ring).not.toHaveAttribute("data-warning");
    rerender(<CountdownRing validFromMs={0} validUntilMs={30_000} nowMs={25_500} still />);
    expect(ring).toHaveAttribute("data-warning", "true");
    expect(ring.querySelectorAll("circle")[1]).toHaveClass("stroke-warning");
    expect(ring.querySelectorAll("circle")[1]?.getAttribute("class")).not.toContain("transition");
  });
});

describe("EntryAvatar", () => {
  it("takes the account's colour, its name's when automatic, and its mark", () => {
    // The same name, the same colour, on every device and whatever the case.
    expect(autoColor("GitHub")).toBe(autoColor("  github "));
    expect(autoColor("", "octocat")).toBe(autoColor("octocat"));
    const picked = new Set(
      ["GitHub", "Google", "Microsoft", "AWS", "Bank", "Proton", "Game", "Cloudflare"].map((n) =>
        autoColor(n),
      ),
    );
    expect(picked.size).toBeGreaterThanOrEqual(4);
    expect(picked.has("gray")).toBe(false);
    const { rerender } = render(<EntryAvatar issuer="GitHub" />);
    const avatar = screen.getByTestId("entry-avatar");
    expect(avatar).toHaveAttribute("data-tag", autoColor("GitHub"));
    expect(avatar).toHaveClass("bg-tag-bg", "text-tag-fg");
    expect(avatar).toHaveTextContent("G");
    rerender(<EntryAvatar issuer="GitHub" color="purple" mark="GH" />);
    expect(avatar).toHaveAttribute("data-tag", "purple");
    expect(avatar).toHaveTextContent("GH");
    expect(avatar).toHaveClass("text-[12px]");
    rerender(<EntryAvatar issuer="GitHub" color="gray" mark={null} />);
    expect(avatar).toHaveAttribute("data-tag", "gray");
    expect(avatar).toHaveTextContent("G");
  });

  it("keeps its own corner whatever the theme's card radius, at the cards' sizes too", () => {
    // regression (澄空 and 墨白, 2026-10-11): rounded-10 follows the theme's card radius (18 / 16),
    // which turned a 32 px square into a circle.
    const { rerender } = render(<EntryAvatar issuer="GitHub" />);
    const avatar = screen.getByTestId("entry-avatar");
    expect(avatar.className).not.toContain("rounded-10");
    expect(avatar.style.borderRadius).toBe("10px");
    rerender(<EntryAvatar issuer="GitHub" size={36} />);
    expect([avatar.style.width, avatar.style.borderRadius]).toEqual(["36px", "10px"]);
    expect(avatar).toHaveClass("text-[16px]");
    rerender(<EntryAvatar issuer="GitHub" size={28} />);
    expect([avatar.style.width, avatar.style.borderRadius]).toEqual(["28px", "8px"]);
    expect(avatar).toHaveClass("text-[13px]");
  });

  it("shows the first letter of the issuer, else of the account", () => {
    expect(initial("github", "x")).toBe("G");
    expect(initial("  ", "octocat")).toBe("O");
    expect(initial("", "")).toBe("?");
    expect(initial("中国银行")).toBe("中");
    render(<EntryAvatar issuer="" account="mail@x" size={40} />);
    expect(screen.getByTestId("entry-avatar")).toHaveTextContent("M");
  });
});

describe("EntryRow", () => {
  it("regression: the app's shortcuts get through its buttons, the list's keys do not", async () => {
    // 2026-10-15: with the pin beside a row's menu focused, Ctrl+, opened no settings.
    const user = userEvent.setup();
    const onCopy = vi.fn();
    const onNext = vi.fn();
    const onFavorite = vi.fn();
    render(
      <EntryRow
        entry={entry({ kind: { type: "hotp", counter: 3 } })}
        code={code({ next_code: null, valid_from_ms: null, valid_until_ms: null })}
        nowMs={T}
        onCopy={onCopy}
        onNext={onNext}
        onFavorite={onFavorite}
        menu={<button type="button">menu</button>}
      />,
    );
    for (const name of ["收藏", "menu", "生成下一个"]) {
      const heard = await heardByWindow(
        screen.getByRole("button", { name }),
        "{Control>},{/Control}",
        "{Meta>}k{/Meta}",
        "/",
        "{ArrowDown}",
      );
      expect(heard).toEqual(expect.arrayContaining(["Ctrl+,", "Meta+k", "/"]));
      expect(heard).not.toContain("ArrowDown");
    }
    // The buttons keep their own keys, and the row does not copy.
    screen.getByRole("button", { name: "收藏" }).focus();
    await user.keyboard("{Enter}");
    screen.getByRole("button", { name: "生成下一个" }).focus();
    await user.keyboard(" ");
    expect([onFavorite, onNext, onCopy].map((f) => f.mock.calls.length)).toEqual([1, 1, 0]);
  });

  it("copies on click, Enter and Space, but not from its own buttons", async () => {
    const onCopy = vi.fn();
    const onNext = vi.fn();
    render(
      <EntryRow
        entry={entry({ kind: { type: "hotp", counter: 3 }, favorite: true })}
        code={code({ next_code: null, valid_from_ms: null, valid_until_ms: null })}
        nowMs={T}
        onCopy={onCopy}
        onNext={onNext}
        menu={<button type="button">menu</button>}
      />,
    );
    const row = screen.getByTestId("entry-row");
    await userEvent.click(row);
    fireEvent.keyDown(row, { key: "Enter" });
    fireEvent.keyDown(row, { key: " " });
    fireEvent.keyDown(row, { key: "a" });
    expect(onCopy).toHaveBeenCalledTimes(3);
    await userEvent.click(screen.getByRole("button", { name: "生成下一个" }));
    await userEvent.click(screen.getByRole("button", { name: "menu" }));
    fireEvent.keyDown(screen.getByRole("button", { name: "menu" }), { key: "Enter" });
    expect(onNext).toHaveBeenCalledOnce();
    expect(onCopy).toHaveBeenCalledTimes(3);
    expect(screen.queryByTestId("countdown-ring")).toBeNull();
  });

  it("is a checkbox while selecting: a click, Enter or Space ticks it, and the buttons step aside", async () => {
    const onCopy = vi.fn();
    const onToggle = vi.fn();
    const onContextMenu = vi.fn();
    const props = {
      code: code(),
      nowMs: T,
      onCopy,
      onFavorite: vi.fn(),
      onContextMenu,
      menu: <button type="button">menu</button>,
    };
    const { rerender } = render(
      <EntryRow entry={entry()} {...props} selection={{ checked: false, onToggle }} />,
    );
    const row = screen.getByRole("checkbox");
    expect(row).toHaveAttribute("aria-checked", "false");
    expect(row).toHaveAttribute("title", "点击选择");
    await userEvent.click(row);
    fireEvent.keyDown(row, { key: "Enter" });
    fireEvent.keyDown(row, { key: " " });
    fireEvent.contextMenu(row);
    expect(onToggle).toHaveBeenCalledTimes(3);
    expect([onCopy, onContextMenu].map((f) => f.mock.calls.length)).toEqual([0, 0]);
    expect(screen.queryByTestId("row-favorite")).toBeNull();
    expect(screen.queryByTestId("row-actions")).toBeNull();
    rerender(<EntryRow entry={entry()} {...props} selection={{ checked: true, onToggle }} />);
    expect(screen.getByRole("checkbox")).toHaveAttribute("aria-checked", "true");
    expect(screen.getByTestId("row-select")).toBeChecked();
    // A counter-based row's "next code" is an action too.
    const hotp = entry({ kind: { type: "hotp", counter: 3 } });
    const nextCode = code({ next_code: null, valid_from_ms: null, valid_until_ms: null });
    rerender(
      <EntryRow
        entry={hotp}
        {...props}
        code={nextCode}
        onNext={vi.fn()}
        selection={{ checked: false, onToggle }}
      />,
    );
    expect(screen.queryByRole("button", { name: "生成下一个" })).toBeNull();
    // Its room is kept, so the codes stay in one column with the rows that have a ring.
    expect(screen.getByTestId("row-ring-slot")).toHaveClass("size-[30px]");
    // Out of selection, a button again.
    rerender(<EntryRow entry={entry()} {...props} />);
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.getByTestId("row-favorite")).toBeInTheDocument();
  });

  it("shows the next code in the last seconds and switches by itself when a frame is late", () => {
    const { rerender } = render(
      <EntryRow entry={entry()} code={code()} nowMs={T} onCopy={() => undefined} />,
    );
    expect(screen.getByTestId("otp-code")).toHaveTextContent("492 039");
    expect(screen.queryByTestId("next-code")).toBeNull();
    rerender(
      <EntryRow entry={entry()} code={code()} nowMs={T + 16_000} onCopy={() => undefined} />,
    );
    expect(screen.getByTestId("next-code")).toHaveTextContent("下一个 114 415");
    expect(screen.getByTestId("otp-code")).toHaveClass("text-warning");
    rerender(
      <EntryRow entry={entry()} code={code()} nowMs={T + 20_500} onCopy={() => undefined} />,
    );
    expect(screen.getByTestId("otp-code")).toHaveTextContent("114 415");
    expect(screen.queryByTestId("countdown-ring")).toBeNull();
  });

  it("shows the pin beside the menu, with a mouse only on hover, and hands a right click over", async () => {
    const onCopy = vi.fn();
    const onFavorite = vi.fn();
    const onContextMenu = vi.fn();
    const menu = <button type="button">menu</button>;
    const props = { code: code(), nowMs: T, onCopy, onFavorite, onContextMenu, menu };
    const { rerender } = render(<EntryRow entry={entry()} {...props} />);
    const pin = screen.getByRole("button", { name: "收藏" });
    expect(pin).toHaveAttribute("aria-pressed", "false");
    await userEvent.click(pin);
    expect(onFavorite).toHaveBeenCalledOnce();
    expect(onCopy).not.toHaveBeenCalled();
    // Board A1: the pin and the menu wait for the row (hovered, focused, or its menu open).
    expect(screen.getByTestId("row-actions")).toHaveClass(
      "pointer-fine:opacity-0",
      "group-hover:opacity-100",
      "group-focus-within:opacity-100",
      "has-[[aria-expanded=true]]:opacity-100",
    );
    expect(screen.getByTestId("row-actions")).toContainElement(
      screen.getByRole("button", { name: "menu" }),
    );
    fireEvent.contextMenu(screen.getByTestId("entry-row"));
    expect(onContextMenu).toHaveBeenCalledOnce();
    expect(screen.queryByTestId("row-pinned")).toBeNull();
    rerender(<EntryRow entry={entry({ favorite: true })} {...props} />);
    // A pinned account says so after its name whatever the pointer does; the button is the toggle.
    expect(screen.getByTestId("row-pinned")).toHaveClass("text-warning");
    const pinned = screen.getByRole("button", { name: "收藏" });
    expect(pinned).toHaveAttribute("aria-pressed", "true");
    expect(pinned.querySelector("svg")).toHaveAttribute("fill", "currentColor");
    // On the phone's cards there is no hover: they stay.
    rerender(<EntryRow entry={entry()} {...props} card />);
    expect(screen.getByTestId("row-actions")).not.toHaveClass("pointer-fine:opacity-0");
    // Without the handlers, no buttons.
    rerender(<EntryRow entry={entry()} code={code()} nowMs={T} onCopy={onCopy} />);
    expect(screen.queryByRole("button", { name: "收藏" })).toBeNull();
    expect(screen.queryByTestId("row-actions")).toBeNull();
  });

  it("has the boards' measures: 64 px rows in the list, cards of their own on the phone", () => {
    const { rerender } = render(
      <EntryRow entry={entry()} code={code()} nowMs={T} onCopy={() => undefined} />,
    );
    const row = screen.getByTestId("entry-row");
    expect(row).toHaveClass("h-[calc(var(--row-h)+32px)]", "rounded-10", "hover:bg-inset");
    expect(row).not.toHaveClass("panel");
    expect(screen.getByTestId("entry-avatar")).toHaveStyle({ width: "40px" });
    expect(screen.getByTestId("otp-code").style.fontSize).toBe("26px");
    expect(screen.getByTestId("otp-code")).toHaveClass("font-semibold");
    expect(screen.getByTestId("countdown-ring")).toHaveAttribute("width", "30");
    rerender(<EntryRow entry={entry()} code={code()} nowMs={T} onCopy={() => undefined} card />);
    expect(row).toHaveClass("h-[calc(var(--row-h)+38px)]", "rounded-14", "bg-surface", "panel");
    expect(screen.getByTestId("otp-code").style.fontSize).toBe("22px");
    expect(screen.getByTestId("countdown-ring")).toHaveAttribute("width", "26");
  });

  it("says it was copied in place of its code, and hints at copying on hover", () => {
    const { rerender } = render(
      <EntryRow entry={entry()} code={code()} nowMs={T} onCopy={() => undefined} />,
    );
    const row = screen.getByTestId("entry-row");
    // Shown on hover or focus, beside the code (boards A and B); hidden it takes no room, which a
    // phone's narrow row needs for the names (regression 2026-10-11: they were cut to 「Git…」).
    expect(screen.getByTestId("row-hint")).toHaveTextContent("点击复制");
    expect(screen.getByTestId("row-hint")).toHaveClass("hidden", "group-hover:inline");
    expect(screen.getByTestId("row-hint")).not.toHaveClass("opacity-0");
    rerender(
      <EntryRow entry={entry()} code={code()} nowMs={T} onCopy={() => undefined} copied="已复制" />,
    );
    expect(row).toHaveAttribute("data-copied", "true");
    const pill = screen.getByTestId("row-copied");
    expect(pill).toHaveTextContent(/^已复制$/);
    // The theme's pill: the accent's soft fill, or ink in 墨白.
    expect(pill).toHaveClass("bg-[var(--row-pill-bg)]", "text-[var(--row-pill-fg)]");
    expect(screen.queryByTestId("otp-code")).toBeNull();
    expect(screen.queryByTestId("row-hint")).toBeNull();
    expect(screen.getByTestId("countdown-ring")).toBeInTheDocument();
    // In its last seconds, or masked, the pill still stands alone.
    const props = { entry: entry(), code: code(), onCopy: () => undefined, copied: "已复制" };
    rerender(<EntryRow {...props} nowMs={T + 16_000} />);
    expect(screen.queryByTestId("next-code")).toBeNull();
    rerender(<EntryRow {...props} nowMs={T} masked />);
    expect(screen.queryByTestId("otp-code")).toBeNull();
    // While selecting, the row is a checkbox: no hint.
    rerender(
      <EntryRow
        entry={entry()}
        code={code()}
        nowMs={T}
        onCopy={() => undefined}
        selection={{ checked: false, onToggle: () => undefined }}
      />,
    );
    expect(screen.queryByTestId("row-hint")).toBeNull();
  });

  it("masks until hovered, waits for the first frame, and names an account-only entry", () => {
    const { rerender } = render(
      <EntryRow entry={entry({ issuer: "" })} nowMs={T} onCopy={() => undefined} />,
    );
    expect(screen.getByText("— — —")).toBeInTheDocument();
    expect(screen.getByText("octocat")).toBeInTheDocument();
    rerender(<EntryRow entry={entry()} code={code()} nowMs={T} masked onCopy={() => undefined} />);
    const codes = screen.getAllByTestId("otp-code");
    expect(codes[0]).toHaveTextContent("••• •••");
    expect(codes[1]).toHaveClass("hidden", "group-hover:inline");
  });
});

describe("PasswordField", () => {
  it("rates strength, toggles visibility and shows errors", async () => {
    expect(passwordStrength("short")).toBe("tooShort");
    expect(passwordStrength("abcdefgh")).toBe("weak");
    expect(passwordStrength("abcdefghijkl")).toBe("fair");
    expect(passwordStrength("Abcdefg1!")).toBe("fair");
    expect(passwordStrength("correct horse battery staple")).toBe("strong");
    expect(passwordStrength("Abcdefghij1!")).toBe("strong");
    const onChange = vi.fn();
    const { rerender } = render(
      <PasswordField label="主密码" value="" onChange={onChange} strength help="至少 8 个字符" />,
    );
    const input = screen.getByLabelText("主密码");
    expect(input).toHaveAttribute("type", "password");
    expect(screen.queryByTestId("password-strength")).toBeNull();
    expect(screen.getByText("至少 8 个字符")).toBeInTheDocument();
    await userEvent.type(input, "x");
    expect(onChange).toHaveBeenLastCalledWith("x");
    await userEvent.click(screen.getByRole("button", { name: "显示密码" }));
    expect(input).toHaveAttribute("type", "text");
    await userEvent.click(screen.getByRole("button", { name: "隐藏密码" }));
    rerender(
      <PasswordField
        label="主密码"
        value="abcdefghijkl"
        onChange={onChange}
        strength
        error="密码错误"
      />,
    );
    expect(screen.getByTestId("password-strength")).toHaveAttribute("data-strength", "fair");
    expect(screen.getByText("一般")).toBeInTheDocument();
    expect(screen.getByText("密码错误")).toBeInTheDocument();
    expect(input).toHaveAttribute("aria-invalid", "true");
  });
});

describe("DropZone / QrView / StepList", () => {
  it("drop zone opens the picker and shows a hovering drag", async () => {
    const onActivate = vi.fn();
    const { rerender } = render(
      <DropZone title="选择图片" hint="或拖到这里" onActivate={onActivate} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "拖放文件区域" }));
    expect(onActivate).toHaveBeenCalledOnce();
    expect(screen.getByTestId("drop-zone")).not.toHaveAttribute("data-active");
    rerender(<DropZone title="选择图片" active onActivate={onActivate} />);
    expect(screen.getByTestId("drop-zone")).toHaveClass("bg-accent-soft");
  });

  it("the QR code is an image on the white plate", () => {
    render(
      <I18nProvider locale="en">
        <QrView svg='<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>' footer="1 / 2" />
      </I18nProvider>,
    );
    const image = screen.getByRole("img", { name: "QR code" });
    expect(image.getAttribute("src")).toMatch(/^data:image\/svg\+xml;charset=utf-8,%3Csvg/);
    expect(screen.getByTestId("qr-plate")).toHaveClass("bg-qr-plate");
    expect(screen.getByText("1 / 2")).toBeInTheDocument();
  });

  it("steps are numbered", () => {
    render(<StepList steps={["打开应用", <b key="b">导出</b>]} />);
    const list = screen.getByRole("list", { name: "操作步骤" });
    expect(list.querySelectorAll("li")).toHaveLength(2);
    expect(list).toHaveTextContent("1打开应用2导出");
  });
});
