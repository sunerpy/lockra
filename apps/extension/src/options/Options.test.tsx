// The options page (board 35): the connection, the shortcuts, site access, what the extension keeps.
import { extensionTranslator } from "@lockra/shared/i18n/extension";
import { act, fireEvent, render, screen } from "@testing-library/react";
import type { PopupEvent, PopupRequest, PopupState } from "../shared/messages";
import { TextProvider } from "../shared/text";
import type { PageClient } from "../popup/client";
import { Options, SOURCE_URL } from "./Options";

function state(change: Partial<PopupState> = {}): PopupState {
  return {
    screen: "ready",
    starting: false,
    appVersion: "0.10.0",
    extensionVersion: "0.10.0",
    pairing: null,
    pairingFailed: null,
    pageAccess: false,
    site: null,
    policy: null,
    paused: false,
    shortcut: "Ctrl+Shift+L",
    unlockAsked: false,
    pairedAt: Date.UTC(2026, 9, 12, 12),
    ...change,
  };
}

async function setup(initial: PopupState, extra: Partial<PageClient> = {}) {
  const sent: PopupRequest[] = [];
  let emit: (event: PopupEvent) => void = () => undefined;
  const client: PageClient = {
    connect: (onEvent) => {
      emit = onEvent;
      return { send: (request) => sent.push(request), close: vi.fn() };
    },
    requestAccess: vi.fn(async () => true),
    removeAccess: vi.fn(async () => true),
    openOptions: vi.fn(),
    close: vi.fn(),
    copyText: vi.fn(async () => undefined),
    shortcuts: async () => ({ fill: "Ctrl+Shift+L", popup: "" }),
    target: "chromium",
    ...extra,
  };
  await act(async () => {
    render(
      <TextProvider t={extensionTranslator("zh-CN")}>
        <Options client={client} locale="zh-CN" />
      </TextProvider>,
    );
  });
  act(() => emit({ kind: "state", state: initial }));
  return {
    sent,
    client,
    push: (next: PopupState) => act(() => emit({ kind: "state", state: next })),
  };
}

describe("the options page", () => {
  it("shows the pairing and unpairs it", async () => {
    const { sent } = await setup(state());
    expect(screen.getByText("Lockra 扩展")).toBeInTheDocument();
    expect(
      screen.getByText("已配对 · Lockra 0.10.0 · 已解锁 · 配对于 10月12日"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "取消配对" }));
    expect(sent.at(-1)).toEqual({ kind: "unpair" });
  });

  it("lists the shortcuts, a missing one as not set, and where to change them", async () => {
    await setup(state());
    expect(screen.getByLabelText("Ctrl Shift L")).toBeInTheDocument();
    expect(screen.getByText("未设置")).toBeInTheDocument();
    expect(screen.getByText(/chrome:\/\/extensions\/shortcuts/)).toBeInTheDocument();
  });

  it("grants and removes site access in the click", async () => {
    const { client, sent, push } = await setup(state());
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "授予权限" }));
    });
    expect(client.requestAccess).toHaveBeenCalled();
    expect(sent.at(-1)).toEqual({ kind: "access.changed" });
    push(state({ pageAccess: true }));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "收回权限" }));
    });
    expect(client.removeAccess).toHaveBeenCalled();
  });

  it("copies the source link, and says the extension keeps nothing", async () => {
    const { client } = await setup(state({ screen: "unpaired", pairedAt: null }), {
      target: "firefox",
    });
    expect(screen.getByText("未配对")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "取消配对" })).toBeNull();
    expect(screen.getByText("扩展不保存登录项")).toBeInTheDocument();
    expect(screen.getByText("在附加组件管理器 › 管理扩展快捷键中修改。")).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "复制链接" }));
    });
    expect(client.copyText).toHaveBeenCalledWith(SOURCE_URL);
    expect(screen.getByRole("button", { name: "已复制" })).toBeInTheDocument();
  });
});
