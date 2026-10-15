import { type CodeView, type EntryView } from "@lockra/shared";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { heardByWindow } from "../test/keys";
import { RecentCard } from "./RecentCard";

const T = 1_790_000_010_000;

function entry(overrides: Partial<EntryView> = {}): EntryView {
  return {
    id: "e1",
    issuer: "GitHub",
    account: "octocat@example.com",
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
    last_used_at_ms: T - 60_000,
    export: { google: null, microsoft: null },
    ...overrides,
  };
}

function code(overrides: Partial<CodeView> = {}): CodeView {
  return {
    entry_id: "e1",
    code: "261491",
    next_code: "653018",
    valid_from_ms: T - 12_000,
    valid_until_ms: T + 18_000,
    ...overrides,
  };
}

describe("RecentCard", () => {
  it("shows the account used last with a large code, its seconds in a ring and the next code", async () => {
    const onCopy = vi.fn();
    render(<RecentCard entry={entry()} code={code()} nowMs={T} onCopy={onCopy} />);
    const card = screen.getByTestId("recent-card");
    expect(card).toHaveAttribute("role", "button");
    expect(card).toHaveTextContent("最近使用");
    expect(card).toHaveTextContent("GitHub");
    expect(card).toHaveTextContent("octocat@example.com");
    expect(screen.getByTestId("otp-code")).toHaveTextContent("261 491");
    expect(screen.getByTestId("otp-code").style.fontSize).toBe("54px");
    expect(screen.getByTestId("recent-seconds")).toHaveTextContent("18");
    expect(screen.getByTestId("countdown-ring")).toHaveAttribute("width", "80");
    // The next code is always there on the card, not only in the last seconds.
    expect(screen.getByTestId("recent-next")).toHaveTextContent("下一个 653 018");
    expect(card).toHaveTextContent("点击卡片或按 Enter 复制");
    await userEvent.click(card);
    fireEvent.keyDown(card, { key: "Enter" });
    fireEvent.keyDown(card, { key: " " });
    expect(onCopy).toHaveBeenCalledTimes(3);
  });

  it("warns in the last seconds, masks until hovered, and offers the next code for HOTP", async () => {
    const onNext = vi.fn();
    const onCopy = vi.fn();
    const { rerender } = render(
      <RecentCard entry={entry()} code={code()} nowMs={T + 14_000} onCopy={onCopy} />,
    );
    expect(screen.getByTestId("otp-code")).toHaveClass("text-warning");
    expect(screen.getByTestId("countdown-ring")).toHaveAttribute("data-warning", "true");
    rerender(<RecentCard entry={entry()} code={code()} nowMs={T} masked onCopy={onCopy} />);
    expect(screen.getAllByTestId("otp-code")[0]).toHaveTextContent("••• •••");
    expect(screen.queryByTestId("recent-next")).toBeNull();
    rerender(
      <RecentCard
        entry={entry({ kind: { type: "hotp", counter: 4 } })}
        code={code({ next_code: null, valid_from_ms: null, valid_until_ms: null })}
        nowMs={T}
        onCopy={onCopy}
        onNext={onNext}
      />,
    );
    expect(screen.queryByTestId("countdown-ring")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "生成下一个" }));
    expect(onNext).toHaveBeenCalledOnce();
    expect(onCopy).not.toHaveBeenCalled();
  });

  it("regression: the app's shortcuts get through its next-code button, other keys do not", async () => {
    // 2026-10-15: with the button focused, Ctrl+, opened no settings.
    const user = userEvent.setup();
    const onCopy = vi.fn();
    const onNext = vi.fn();
    render(
      <RecentCard
        entry={entry({ kind: { type: "hotp", counter: 4 } })}
        code={code({ next_code: null, valid_from_ms: null, valid_until_ms: null })}
        nowMs={T}
        onCopy={onCopy}
        onNext={onNext}
      />,
    );
    const next = screen.getByRole("button", { name: "生成下一个" });
    const heard = await heardByWindow(
      next,
      "{Control>},{/Control}",
      "{Meta>}k{/Meta}",
      "/",
      "{ArrowDown}",
    );
    expect(heard).toEqual(expect.arrayContaining(["Ctrl+,", "Meta+k", "/"]));
    expect(heard).not.toContain("ArrowDown");
    // The button keeps its own keys, and the card does not copy.
    next.focus();
    await user.keyboard("{Enter}");
    expect(onNext).toHaveBeenCalledOnce();
    expect(onCopy).not.toHaveBeenCalled();
  });

  it("is smaller on the phone, with a hint for a tap", () => {
    render(
      <RecentCard entry={entry()} code={code()} nowMs={T} onCopy={() => undefined} size="sm" />,
    );
    expect(screen.getByTestId("otp-code").style.fontSize).toBe("44px");
    expect(screen.getByTestId("countdown-ring")).toHaveAttribute("width", "64");
    expect(screen.getByTestId("recent-card")).toHaveTextContent("轻点复制");
  });
});
