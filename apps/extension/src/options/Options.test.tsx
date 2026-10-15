// The options page (board 35): the connection, the shortcuts, site access, what the extension keeps.
import { extensionTranslator } from "@lockra/shared/i18n/extension";
import { act, fireEvent, render, screen } from "@testing-library/react";
import type { PopupEvent, PopupRequest, PopupState } from "../shared/messages";
import { TextProvider } from "../shared/text";
import type { BrowserFeatures, Feature } from "../popup/browser-features";
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
    browserFeatures: async () => ({ passwords: "browser", forms: "browser" }),
    setBrowserFeature: vi.fn(async (feature: Feature, take: boolean): Promise<BrowserFeatures> => ({
      passwords: feature === "passwords" && take ? "lockra" : "browser",
      forms: feature === "forms" && take ? "lockra" : "browser",
    })),
    openPasswordManager: vi.fn(),
    menuOnFocus: async () => true,
    setMenuOnFocus: vi.fn(async () => undefined),
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

  it("takes the browser's password saving over in the click, and opens its password manager", async () => {
    const { client } = await setup(state());
    const take = screen.getByRole("switch", { name: "让 Lockra 接管浏览器的密码功能" });
    expect(take).toHaveAttribute("aria-checked", "false");
    await act(async () => {
      fireEvent.click(take);
    });
    expect(client.setBrowserFeature).toHaveBeenCalledWith("passwords", true);
    expect(take).toHaveAttribute("aria-checked", "true");
    // What the switch cannot do: the passwords the browser keeps, and where they are deleted.
    expect(screen.getByText(/浏览器里已经保存的密码不受这个开关影响/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "打开浏览器的密码管理" }));
    expect(client.openPasswordManager).toHaveBeenCalled();
    // The address and card autofill: a choice of its own.
    const forms = screen.getByRole("switch", { name: "同时关闭地址和付款方式的自动填充" });
    await act(async () => {
      fireEvent.click(forms);
    });
    expect(client.setBrowserFeature).toHaveBeenLastCalledWith("forms", true);
  });

  it("says when a policy or another extension holds the setting; Firefox offers the password saving alone", async () => {
    await setup(state(), {
      browserFeatures: async () => ({ passwords: "locked", forms: null }),
      openPasswordManager: null,
      target: "firefox",
    });
    const take = screen.getByRole("switch", { name: "让 Lockra 接管浏览器的密码功能" });
    expect(take).toBeDisabled();
    expect(screen.getByText("未生效：浏览器的策略或其他扩展控制着这项设置。")).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "同时关闭地址和付款方式的自动填充" })).toBeNull();
    expect(screen.queryByRole("button", { name: "打开浏览器的密码管理" })).toBeNull();
  });

  it("opens the menu on a field's focus, or only from its mark", async () => {
    const { client } = await setup(state());
    const onFocus = screen.getByRole("switch", { name: "点击输入框时打开 Lockra 菜单" });
    expect(onFocus).toHaveAttribute("aria-checked", "true");
    await act(async () => {
      fireEvent.click(onFocus);
    });
    expect(client.setMenuOnFocus).toHaveBeenCalledWith(false);
    expect(onFocus).toHaveAttribute("aria-checked", "false");
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
