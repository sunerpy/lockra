import { type CodeView, type EntryView } from "@lockra/shared";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { heardByWindow } from "../test/keys";
import { EntryTile, tileStep } from "./EntryTile";

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

describe("EntryTile", () => {
  it("regression: the app's shortcuts get through its buttons, the cards' keys do not", async () => {
    // 2026-10-15: with a card's star focused, Ctrl+, opened no settings.
    const user = userEvent.setup();
    const onCopy = vi.fn();
    const onNext = vi.fn();
    const onFavorite = vi.fn();
    render(
      <EntryTile
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
        "{ArrowRight}",
      );
      expect(heard).toEqual(expect.arrayContaining(["Ctrl+,", "Meta+k"]));
      expect(heard).not.toContain("ArrowRight");
    }
    // The buttons keep their own keys, and the card does not copy.
    screen.getByRole("button", { name: "收藏" }).focus();
    await user.keyboard("{Enter}");
    screen.getByRole("button", { name: "生成下一个" }).focus();
    await user.keyboard(" ");
    expect([onFavorite, onNext, onCopy].map((f) => f.mock.calls.length)).toEqual([1, 1, 0]);
  });

  it("copies on click, Enter and Space, but not from its own buttons; a counter card counts", async () => {
    const onCopy = vi.fn();
    const onNext = vi.fn();
    render(
      <EntryTile
        entry={entry({ kind: { type: "hotp", counter: 3 } })}
        code={code({ next_code: null, valid_from_ms: null, valid_until_ms: null })}
        nowMs={T}
        onCopy={onCopy}
        onNext={onNext}
        menu={<button type="button">menu</button>}
      />,
    );
    const tile = screen.getByTestId("entry-tile");
    expect(tile).toHaveAttribute("role", "button");
    expect(tile).toHaveAttribute("title", "点击复制");
    expect(tile).toHaveClass("hover:border-[color:var(--tile-hover-edge)]", "hover:shadow-pop");
    await userEvent.click(tile);
    fireEvent.keyDown(tile, { key: "Enter" });
    fireEvent.keyDown(tile, { key: " " });
    fireEvent.keyDown(tile, { key: "a" });
    expect(onCopy).toHaveBeenCalledTimes(3);
    expect(tile).toHaveTextContent("计数器 3");
    await userEvent.click(screen.getByRole("button", { name: "生成下一个" }));
    fireEvent.keyDown(screen.getByRole("button", { name: "生成下一个" }), { key: "Enter" });
    await userEvent.click(screen.getByRole("button", { name: "menu" }));
    expect(onNext).toHaveBeenCalledOnce();
    expect(onCopy).toHaveBeenCalledTimes(3);
    expect(screen.queryByTestId("countdown-bar")).toBeNull();
  });

  it("keeps the button alone on the phone's counter card", () => {
    render(
      <EntryTile
        entry={entry({ kind: { type: "hotp", counter: 3 } })}
        code={code({ next_code: null, valid_from_ms: null, valid_until_ms: null })}
        nowMs={T}
        size="sm"
        onCopy={() => undefined}
        onNext={() => undefined}
      />,
    );
    expect(screen.getByRole("button", { name: "生成下一个" })).toBeInTheDocument();
    expect(screen.getByTestId("entry-tile")).not.toHaveTextContent("计数器");
  });

  it("empties a bar over the window, warns with the next code in the last seconds and switches when a frame is late", () => {
    const props = { entry: entry(), code: code(), onCopy: () => undefined };
    const { rerender } = render(<EntryTile {...props} nowMs={T} />);
    const fill = () => screen.getByTestId("countdown-bar").firstElementChild as HTMLElement;
    expect(screen.getByTestId("otp-code")).toHaveTextContent("492 039");
    expect(fill().style.width).toBe(`${(20 / 30) * 100}%`);
    // The theme's meter: the accent, or ink in 墨白.
    expect(fill()).toHaveClass("bg-[var(--meter)]", "transition-[width]");
    expect(screen.getByTestId("countdown-seconds")).toHaveTextContent("20 秒");
    expect(screen.queryByTestId("next-code")).toBeNull();
    rerender(<EntryTile {...props} nowMs={T + 16_000} still />);
    expect(screen.getByTestId("countdown-bar")).toHaveAttribute("data-warning", "true");
    expect(fill()).toHaveClass("bg-warning");
    expect(fill().getAttribute("class")).not.toContain("transition");
    expect(screen.getByTestId("otp-code")).toHaveClass("text-warning");
    expect(screen.getByTestId("next-code")).toHaveTextContent("下一个 114 415");
    expect(screen.getByTestId("countdown-seconds")).toHaveTextContent("4 秒");
    rerender(<EntryTile {...props} nowMs={T + 20_500} />);
    expect(screen.getByTestId("otp-code")).toHaveTextContent("114 415");
    expect(screen.queryByTestId("countdown-bar")).toBeNull();
  });

  it("says it was copied in place of the timer, in the theme's copied colours", () => {
    const { rerender } = render(
      <EntryTile
        entry={entry()}
        code={code()}
        nowMs={T}
        onCopy={() => undefined}
        copied="已复制 · 30 秒后清空剪贴板"
      />,
    );
    const tile = screen.getByTestId("entry-tile");
    expect(tile).toHaveClass("tile-copied");
    expect(tile).toHaveAttribute("data-copied", "true");
    expect(screen.getByTestId("tile-copied")).toHaveTextContent("已复制 · 30 秒后清空剪贴板");
    expect(screen.queryByTestId("countdown-bar")).toBeNull();
    rerender(<EntryTile entry={entry()} code={code()} nowMs={T} onCopy={() => undefined} />);
    expect(tile).not.toHaveClass("tile-copied");
    expect(screen.queryByTestId("tile-copied")).toBeNull();
    expect(screen.getByTestId("countdown-bar")).toBeInTheDocument();
  });

  it("pins from its star, hands a right click over, masks until hovered and waits for a frame", async () => {
    const onCopy = vi.fn();
    const onFavorite = vi.fn();
    const onContextMenu = vi.fn();
    const props = { code: code(), nowMs: T, onCopy, onFavorite, onContextMenu };
    const { rerender } = render(<EntryTile entry={entry()} {...props} />);
    const pin = screen.getByRole("button", { name: "收藏" });
    expect(pin).toHaveAttribute("aria-pressed", "false");
    // Shown on hover or focus until pinned, so an unpinned card stays quiet.
    expect(pin).toHaveClass("opacity-0", "group-hover:opacity-100");
    await userEvent.click(pin);
    expect(onFavorite).toHaveBeenCalledOnce();
    expect(onCopy).not.toHaveBeenCalled();
    fireEvent.contextMenu(screen.getByTestId("entry-tile"));
    expect(onContextMenu).toHaveBeenCalledOnce();
    rerender(<EntryTile entry={entry({ favorite: true })} {...props} />);
    expect(screen.getByRole("button", { name: "收藏" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "收藏" })).not.toHaveClass("opacity-0");
    rerender(<EntryTile entry={entry()} {...props} masked />);
    const codes = screen.getAllByTestId("otp-code");
    expect(codes[0]).toHaveTextContent("••• •••");
    expect(codes[1]).toHaveClass("hidden", "group-hover:inline");
    // Masked, the next code stays hidden as well.
    rerender(<EntryTile entry={entry()} {...props} masked nowMs={T + 16_000} />);
    expect(screen.queryByTestId("next-code")).toBeNull();
    rerender(<EntryTile entry={entry({ issuer: "", favorite: true })} nowMs={T} onCopy={onCopy} />);
    expect(screen.getByText("— — —")).toBeInTheDocument();
    expect(screen.getByText("octocat")).toBeInTheDocument();
    // Without the pin button the star says it beside the name.
    expect(screen.getByTestId("entry-tile").querySelectorAll('[data-icon="star"]')).toHaveLength(1);
  });

  it("draws the code as large as the card allows: smaller for eight digits and on the phone", () => {
    const { rerender } = render(
      <EntryTile entry={entry()} code={code()} nowMs={T} onCopy={() => undefined} />,
    );
    expect(screen.getByTestId("otp-code").style.fontSize).toBe("46px");
    expect(screen.getByTestId("entry-avatar").style.width).toBe("36px");
    expect(screen.getByTestId("tile-hint")).toHaveTextContent("点击复制");
    rerender(
      <EntryTile
        entry={entry({ digits: 8 })}
        code={code({ code: "12345678" })}
        nowMs={T}
        onCopy={() => undefined}
      />,
    );
    expect(screen.getByTestId("otp-code").style.fontSize).toBe("40px");
    rerender(
      <EntryTile entry={entry()} code={code()} nowMs={T} onCopy={() => undefined} size="sm" />,
    );
    expect(screen.getByTestId("otp-code").style.fontSize).toBe("28px");
    expect(screen.getByTestId("entry-avatar").style.width).toBe("28px");
    // No pointer to hover with on the phone: no hint.
    expect(screen.queryByTestId("tile-hint")).toBeNull();
  });
});

describe("tileStep", () => {
  // Three columns of 300 px, rows 220 px apart; the last row holds one card.
  const boxes = [0, 1, 2, 3, 4, 5, 6].map((i) => ({
    left: (i % 3) * 316,
    top: Math.floor(i / 3) * 220,
    width: 300,
  }));

  it("moves along the order with left and right, and to the nearest card above or below", () => {
    expect(tileStep(boxes, 0, "ArrowRight")).toBe(1);
    expect(tileStep(boxes, 6, "ArrowRight")).toBe(6);
    expect(tileStep(boxes, 3, "ArrowLeft")).toBe(2);
    expect(tileStep(boxes, 0, "ArrowLeft")).toBe(0);
    expect(tileStep(boxes, 1, "ArrowDown")).toBe(4);
    expect(tileStep(boxes, 5, "ArrowDown")).toBe(6);
    expect(tileStep(boxes, 6, "ArrowDown")).toBe(6);
    expect(tileStep(boxes, 6, "ArrowUp")).toBe(3);
    expect(tileStep(boxes, 4, "ArrowUp")).toBe(1);
    // Above the first row: the search field.
    expect(tileStep(boxes, 2, "ArrowUp")).toBe(-1);
    expect(tileStep(boxes, 4, "Home")).toBe(0);
    expect(tileStep(boxes, 0, "End")).toBe(6);
    expect(tileStep(boxes, 0, "a")).toBeUndefined();
    expect(tileStep([], 0, "ArrowDown")).toBeUndefined();
  });

  it("finds the row below across a section heading", () => {
    // A heading pushes the next section's first row 40 px further down.
    const sections = [
      { left: 0, top: 0, width: 300 },
      { left: 316, top: 0, width: 300 },
      { left: 0, top: 260, width: 300 },
    ];
    expect(tileStep(sections, 1, "ArrowDown")).toBe(2);
    expect(tileStep(sections, 2, "ArrowUp")).toBe(0);
  });
});
