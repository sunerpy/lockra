import type { ConfirmView } from "@lockra/shared";
import { MOCK_PASSWORD, MockBackend, type MockOptions, sampleEntries } from "@lockra/shared/mock";
import { act, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../../test/render";

function core(options: MockOptions = {}): MockBackend {
  return new MockBackend({
    entries: sampleEntries(),
    passwords: true,
    ...options,
    settings: { locale: "zh-cn", ...options.settings },
  });
}

const PAIR = {
  code: "123456",
  browser: "chrome",
  label: "",
  extension: "egbpficliljjnpkjbigonldakfllebco",
};

function confirm(over: Partial<Omit<ConfirmView, "id" | "expires_at_ms">> = {}) {
  return {
    item_id: "5c0fe1a2-0b3c-4d5e-8f60-000000000001",
    title: "招商银行",
    username: "138 **** 5678",
    frame_url: "https://www.cmbchina.com/login",
    top_url: null,
    browser: "chrome",
    label: "办公室电脑",
    reasons: ["protected" as const],
    action: "fill" as const,
    needs_proof: true,
    ...over,
  };
}

describe("a pairing request", () => {
  it("comes up with its code, and is approved with the master password", async () => {
    const backend = core();
    const { user } = renderApp({ backend });
    await ready();
    let id = "";
    act(() => {
      id = backend.simulatePairRequest(PAIR);
    });
    const dialog = within(await screen.findByRole("dialog", { name: "Chrome 请求连接" }));
    expect(dialog.getByTestId("pair-code")).toHaveTextContent("123 456");
    expect(dialog.getByText(/秒后过期/)).toBeInTheDocument();
    expect(dialog.getByText("egbpficliljjnpkjbigonldakfllebco")).toBeInTheDocument();
    // No system check unlocks this vault: the master password at once.
    expect(dialog.queryByRole("button", { name: "改用主密码" })).not.toBeInTheDocument();
    await user.type(dialog.getByLabelText("主密码"), "wrong");
    await user.click(dialog.getByRole("button", { name: "批准" }));
    expect(await dialog.findByText(/密码错误/)).toBeInTheDocument();
    await user.clear(dialog.getByLabelText("主密码"));
    await user.type(dialog.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(dialog.getByRole("button", { name: "批准" }));
    expect(backend.calls.at(-1)).toEqual({
      command: "browser_pair_approve",
      id,
      password: MOCK_PASSWORD,
    });
    expect(screen.queryByRole("dialog", { name: "Chrome 请求连接" })).not.toBeInTheDocument();
  });

  it("is approved with the system check that unlocks the vault, the master password after it fails", async () => {
    const backend = core({ biometric: "windows_hello", biometricUnlock: true });
    const { user } = renderApp({ backend });
    await ready();
    act(() => {
      backend.simulatePairRequest(PAIR);
    });
    let dialog = within(await screen.findByRole("dialog", { name: "Chrome 请求连接" }));
    await user.click(dialog.getByRole("button", { name: "使用 Windows Hello 批准" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "browser_pair_approve",
      reason: "批准「Chrome」的配对",
    });
    expect(screen.queryByRole("dialog", { name: "Chrome 请求连接" })).not.toBeInTheDocument();

    backend.answerBiometric("biometric_cancelled");
    act(() => {
      backend.simulatePairRequest(PAIR);
    });
    dialog = within(await screen.findByRole("dialog", { name: "Chrome 请求连接" }));
    await user.click(dialog.getByRole("button", { name: "使用 Windows Hello 批准" }));
    expect(await dialog.findByLabelText("主密码")).toBeInTheDocument();
    expect(
      dialog.queryByRole("button", { name: "使用 Windows Hello 批准" }),
    ).not.toBeInTheDocument();
    await user.click(dialog.getByRole("button", { name: "拒绝" }));
    expect(backend.calls.at(-1)).toMatchObject({ command: "browser_pair_deny" });
  });

  it("offers the master password instead, and Esc denies", async () => {
    const backend = core({ biometric: "touch_id", biometricUnlock: true });
    const { user } = renderApp({ backend });
    await ready();
    act(() => {
      backend.simulatePairRequest(PAIR);
      backend.simulatePairRequest({ ...PAIR, code: "654321" });
    });
    const dialog = within(await screen.findByRole("dialog", { name: "Chrome 请求连接" }));
    expect(dialog.getByText("还有 1 个配对请求在排队（最多同时 3 个）")).toBeInTheDocument();
    await user.click(dialog.getByRole("button", { name: "改用主密码" }));
    expect(dialog.getByLabelText("主密码")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(backend.calls.at(-1)).toMatchObject({ command: "browser_pair_deny" });
    // The next request comes up.
    expect(await screen.findByTestId("pair-code")).toHaveTextContent("654 321");
  });

  it("says when it expired, until closed", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const backend = core();
      const { user } = renderApp({ backend });
      await ready();
      act(() => {
        backend.simulatePairRequest({ ...PAIR, expires_at_ms: Date.now() + 1_500 });
      });
      await screen.findByRole("dialog", { name: "Chrome 请求连接" });
      await act(async () => {
        await vi.advanceTimersByTimeAsync(3_000);
      });
      const dialog = within(await screen.findByRole("dialog", { name: "配对请求已过期" }));
      expect(dialog.getByText("请求已过期，请在扩展里重新连接")).toBeInTheDocument();
      await user.click(dialog.getByRole("button", { name: "关闭" }));
      expect(screen.queryByRole("dialog", { name: "配对请求已过期" })).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("a fill waiting for the user", () => {
  it("of a login that asks for proof: the facts, then the proof", async () => {
    const backend = core();
    const { user } = renderApp({ backend });
    await ready();
    let id = "";
    act(() => {
      id = backend.simulateConfirm(confirm());
    });
    const dialog = within(
      await screen.findByRole("dialog", { name: "Chrome 请求填充「招商银行」" }),
    );
    expect(dialog.getByText(/保护级别是「填充前验证」/)).toBeInTheDocument();
    expect(dialog.getByText("https://www.cmbchina.com/login")).toBeInTheDocument();
    expect(dialog.getByText("招商银行 · 138 **** 5678")).toBeInTheDocument();
    expect(dialog.getByText("Chrome · 办公室电脑")).toBeInTheDocument();
    expect(dialog.getByText("只填当前获得焦点的表单里可见的字段。")).toBeInTheDocument();
    expect(dialog.getByText(/秒后自动拒绝/)).toBeInTheDocument();
    await user.type(dialog.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(dialog.getByRole("button", { name: "填充" }));
    expect(backend.calls.at(-1)).toEqual({
      command: "browser_fill_confirm",
      id,
      password: MOCK_PASSWORD,
    });
    expect(screen.queryByRole("dialog", { name: /请求填充/ })).not.toBeInTheDocument();
  });

  it("in a frame of another site: both pages, and a click", async () => {
    const backend = core();
    const { user } = renderApp({ backend });
    await ready();
    let id = "";
    act(() => {
      id = backend.simulateConfirm(
        confirm({
          title: "Example Pay",
          username: "alex@example.com",
          frame_url: "https://pay.example/checkout/login",
          top_url: "https://shop.example/cart",
          reasons: ["cross_origin"],
          needs_proof: false,
        }),
      );
    });
    const dialog = within(await screen.findByRole("dialog", { name: "Chrome 请求在框架里填充" }));
    expect(dialog.getByText("https://pay.example/checkout/login")).toBeInTheDocument();
    expect(dialog.getByText("https://shop.example/cart")).toBeInTheDocument();
    expect(dialog.queryByLabelText("主密码")).not.toBeInTheDocument();
    await user.click(dialog.getByRole("button", { name: "仍然填充" }));
    expect(backend.calls.at(-1)).toEqual({ command: "browser_fill_confirm", id });
  });

  it("a copy from the popup says so, and is denied", async () => {
    const backend = core();
    const { user } = renderApp({ backend });
    await ready();
    act(() => {
      backend.simulateConfirm(confirm({ action: "copy" }));
    });
    const dialog = within(
      await screen.findByRole("dialog", { name: "Chrome 请求复制「招商银行」" }),
    );
    expect(dialog.getByText(/复制前需要验证身份/)).toBeInTheDocument();
    expect(dialog.queryByText("只填当前获得焦点的表单里可见的字段。")).not.toBeInTheDocument();
    await user.click(dialog.getByRole("button", { name: "拒绝" }));
    expect(backend.calls.at(-1)).toMatchObject({ command: "browser_fill_deny" });
  });
});
