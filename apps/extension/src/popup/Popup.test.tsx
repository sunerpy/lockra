// The popup in every state of board 32, on a fake background: what it shows and what it asks for.
import { extensionTranslator } from "@lockra/shared/i18n/extension";
import { act, fireEvent, render, screen } from "@testing-library/react";
import type { ItemSummary } from "../protocol/schemas";
import type { PopupEvent, PopupRequest, PopupState } from "../shared/messages";
import { TextProvider } from "../shared/text";
import type { PageClient } from "./client";
import { Popup } from "./Popup";

const ITEM: ItemSummary = {
  item_id: "5c0fe1a2-0b3c-4d5e-8f60-000000000001",
  title: "GitHub",
  username: "octocat",
  email: "",
  has_password: true,
  has_totp: true,
  protection: "fill",
  favorite: false,
  downgrade: false,
  matches: true,
};
const ITEM2: ItemSummary = {
  ...ITEM,
  item_id: "5c0fe1a2-0b3c-4d5e-8f60-000000000002",
  username: "octocat-work",
};

const POLICY = {
  fill_otp: true,
  copy_otp: true,
  auto_submit: false,
  confirm_fill: false,
  cross_origin_frames: "never",
  on_page_load: false,
  match_port: false,
  match_host: false,
};

function state(change: Partial<PopupState> = {}): PopupState {
  return {
    screen: "ready",
    starting: false,
    appVersion: "0.10.0",
    extensionVersion: "0.10.0",
    pairing: null,
    pairingFailed: null,
    pageAccess: true,
    site: {
      site: "github.com",
      items: [ITEM, ITEM2],
      fillable: true,
      neverFill: false,
      crossOrigin: false,
    },
    policy: POLICY,
    paused: false,
    shortcut: "Ctrl+Shift+L",
    unlockAsked: false,
    pairedAt: 1_760_000_000_000,
    update: null,
    build: LOCKRA_BUILD,
    ...change,
  };
}

function setup(initial: PopupState | null = state(), extra: Partial<PageClient> = {}) {
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
    reload: vi.fn(),
    copyText: vi.fn(async () => undefined),
    shortcuts: async () => ({ fill: "Ctrl+Shift+L", menu: "Alt+Shift+L", popup: "Ctrl+Shift+Y" }),
    browserFeatures: async () => ({ passwords: "browser", forms: "browser" }),
    setBrowserFeature: async () => ({ passwords: "browser", forms: "browser" }),
    openPasswordManager: null,
    menuOnFocus: async () => true,
    setMenuOnFocus: async () => undefined,
    avoidBrowserList: async () => true,
    setAvoidBrowserList: async () => undefined,
    target: "chromium",
    ...extra,
  };
  const now = { t: 1_760_000_000_000 };
  render(
    <TextProvider t={extensionTranslator("zh-CN")}>
      <Popup client={client} now={() => now.t} />
    </TextProvider>,
  );
  const push = (event: PopupEvent) => act(() => emit(event));
  if (initial) push({ kind: "state", state: initial });
  return { sent, client, push, now };
}

describe("the popup", () => {
  it("fills or copies a login's code from its row", () => {
    const { sent } = setup();
    const fills = screen.getAllByRole("button", { name: "填入验证码" });
    expect(fills).toHaveLength(2);
    fireEvent.click(fills[1] as HTMLElement);
    expect(sent.at(-1)).toEqual({ kind: "fill_code", itemId: ITEM2.item_id });
    fireEvent.click(screen.getAllByRole("button", { name: "复制验证码" })[0] as HTMLElement);
    expect(sent.at(-1)).toEqual({ kind: "copy", itemId: ITEM.item_id, part: { type: "totp" } });
  });

  it("says when the page has no field for the code", () => {
    const { push } = setup();
    push({
      kind: "result",
      action: "fill",
      itemId: ITEM.item_id,
      outcome: "no_code_field",
      part: "totp",
    });
    expect(screen.getByRole("status")).toHaveTextContent("网页上没有找到验证码框");
  });

  it("deletes a login only after its row's question, which starts on 取消", () => {
    const { sent } = setup();
    fireEvent.click(screen.getAllByRole("button", { name: "更多操作" })[1] as HTMLElement);
    fireEvent.click(screen.getByRole("menuitem", { name: "删除…" }));
    const ask = screen.getByTestId("delete-confirm");
    expect(ask).toHaveTextContent("删除「GitHub · octocat-work」？");
    expect(ask).toHaveTextContent("删除后无法恢复，同步的设备也会删除它。");
    expect(screen.getByRole("button", { name: "取消" })).toHaveFocus();
    expect(sent.some((r) => r.kind === "delete")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.queryByTestId("delete-confirm")).not.toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "更多操作" })[1] as HTMLElement);
    fireEvent.click(screen.getByRole("menuitem", { name: "删除…" }));
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    expect(sent.at(-1)).toEqual({ kind: "delete", itemId: ITEM2.item_id });
    expect(screen.queryByTestId("delete-confirm")).not.toBeInTheDocument();
  });

  it("tells the extension's own update under the header, in every screen", () => {
    const { push } = setup(state({ update: "waiting" }));
    expect(screen.getByTestId("popup-update")).toHaveTextContent("关闭这个窗口后扩展会自动更新");
    push({
      kind: "state",
      state: state({
        screen: "locked",
        update: "manual",
        appVersion: "0.10.2",
        extensionVersion: "0.10.1",
      }),
    });
    expect(screen.getByTestId("popup-update")).toHaveTextContent(
      "这个扩展是 0.10.1，比 Lockra 0.10.2 旧",
    );
    expect(screen.getByTestId("popup-update")).toHaveTextContent("设置 › 浏览器");
    push({ kind: "state", state: state({ update: "folder", appVersion: "0.10.2" }) });
    expect(screen.getByTestId("popup-update")).toHaveTextContent("重启 Lockra 后会再试");
    push({ kind: "state", state: state() });
    expect(screen.queryByTestId("popup-update")).not.toBeInTheDocument();
  });

  it("asks for a reload when the background is of another build or too old to say", () => {
    // 2026-10-20, the user's Chrome ran a background two versions older than the popup's files:
    // its delete and code buttons did nothing.
    const { client, push } = setup(state({ build: undefined }));
    expect(screen.getByTestId("popup-stale")).toHaveTextContent("扩展的后台还是旧版本");
    expect(screen.queryByTestId("popup-update")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重新加载扩展" }));
    expect(client.reload).toHaveBeenCalledTimes(1);
    push({ kind: "state", state: state({ build: "an-older-build" }) });
    expect(screen.getByTestId("popup-stale")).toBeInTheDocument();
    // One that knows the new copy waits takes it once the window closes: it says so instead.
    push({ kind: "state", state: state({ build: "an-older-build", update: "waiting" }) });
    expect(screen.queryByTestId("popup-stale")).not.toBeInTheDocument();
    expect(screen.getByTestId("popup-update")).toHaveTextContent("关闭这个窗口后扩展会自动更新");
    push({ kind: "state", state: state() });
    expect(screen.queryByTestId("popup-stale")).not.toBeInTheDocument();
  });

  it("switches the site between its registrable domain and the page's full host", () => {
    // 2026-10-20, the user: a company domain lists every subdomain's logins; a switch in the popup.
    const site = {
      site: "example.cn",
      items: [ITEM],
      fillable: true,
      neverFill: false,
      crossOrigin: false,
      host: "login.example.cn",
      hostMatching: false,
    };
    const { sent, push } = setup(state({ site }));
    const box = screen.getByTestId("popup-matching");
    expect(box).toHaveTextContent("按完整域名匹配");
    expect(box).toHaveTextContent("列出 example.cn 下所有子域的登录项");
    fireEvent.click(screen.getByRole("switch", { name: "按完整域名匹配" }));
    expect(sent.at(-1)).toEqual({ kind: "site_matching", host: true });
    push({ kind: "state", state: state({ site: { ...site, items: [], hostMatching: true } }) });
    // No login of the host: the switch stays, to go back.
    expect(screen.getByTestId("popup-matching")).toHaveTextContent(
      "只列出 login.example.cn 的登录项",
    );
    fireEvent.click(screen.getByRole("switch", { name: "按完整域名匹配" }));
    expect(sent.at(-1)).toEqual({ kind: "site_matching", host: false });
  });

  it("in Firefox, an older extension is updated on the add-ons page", () => {
    setup(state({ update: "manual", appVersion: "0.10.2", extensionVersion: "0.10.1" }), {
      target: "firefox",
    });
    expect(screen.getByTestId("popup-update")).toHaveTextContent("附加组件");
    expect(screen.getByTestId("popup-update")).not.toHaveTextContent("设置 › 浏览器");
  });

  it("says hello, then lists the site's logins: the first is the default", () => {
    const { sent } = setup();
    expect(sent[0]).toEqual({ kind: "hello" });
    expect(screen.getByTestId("popup-state")).toHaveTextContent("已解锁");
    expect(screen.getByText("github.com")).toBeInTheDocument();
    expect(screen.getByText("2 个匹配")).toBeInTheDocument();
    const rows = screen.getAllByTestId("match-row");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("↵");
    expect(rows[1]).not.toHaveTextContent("↵");
    expect(
      screen.getByText("验证码会在填好密码后填进下一页，并复制到剪贴板。"),
    ).toBeInTheDocument();
    // The footer: the shortcut's keys, a hint short enough for the row, and Lockra's window.
    expect(screen.getByText("填充，连按换账号")).toHaveClass("truncate");
    expect(screen.getByLabelText("Ctrl Shift L")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "打开 Lockra" }));
    expect(sent.at(-1)).toEqual({ kind: "start", show: true });
  });

  it("fills a row by its button or Enter, and closes once filled", () => {
    const { sent, push, client } = setup();
    fireEvent.click(screen.getAllByRole("button", { name: "填充" })[1] as HTMLElement);
    expect(sent.at(-1)).toEqual({ kind: "fill", itemId: ITEM2.item_id, allowHttp: false });
    const list = screen.getAllByTestId("match-row")[0]?.parentElement as HTMLElement;
    fireEvent.keyDown(list, { key: "ArrowDown" });
    fireEvent.keyDown(list, { key: "ArrowDown" });
    fireEvent.keyDown(list, { key: "Enter" });
    expect(sent.at(-1)).toEqual({ kind: "fill", itemId: ITEM.item_id, allowHttp: false });
    push({ kind: "result", action: "fill", itemId: ITEM.item_id, outcome: "waiting" });
    expect(screen.getByText("在 Lockra 中确认…")).toBeInTheDocument();
    push({ kind: "result", action: "fill", itemId: ITEM.item_id, outcome: "done" });
    expect(client.close).toHaveBeenCalled();
  });

  it("asks before filling an https login into an http page", () => {
    const { sent, push } = setup();
    fireEvent.click(screen.getAllByRole("button", { name: "填充" })[0] as HTMLElement);
    push({ kind: "result", action: "fill", itemId: ITEM.item_id, outcome: "http_downgrade" });
    expect(screen.getByText("这个页面没有加密（HTTP）")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "仍然填充" }));
    expect(sent.at(-1)).toEqual({ kind: "fill", itemId: ITEM.item_id, allowHttp: true });
  });

  it("words a refusal under the list", () => {
    const { push } = setup();
    push({
      kind: "result",
      action: "copy",
      itemId: ITEM.item_id,
      outcome: "protected",
      part: "password",
    });
    expect(screen.getByText("这个登录项不交给扩展，或不能这样填充。")).toBeInTheDocument();
    push({
      kind: "result",
      action: "copy",
      itemId: ITEM2.item_id,
      outcome: "done",
      part: "password",
    });
    expect(screen.getByText("已复制，Lockra 会按时清空剪贴板")).toBeInTheDocument();
  });

  it("searches every login; other sites' are for copying", async () => {
    vi.useFakeTimers();
    const { sent, push } = setup();
    fireEvent.keyDown(screen.getByText("github.com"), { key: "/" });
    const search = screen.getByRole("searchbox", { name: "搜索全部登录项…" });
    expect(search).toHaveFocus();
    fireEvent.change(search, { target: { value: "bank" } });
    act(() => {
      vi.advanceTimersByTime(200);
    });
    vi.useRealTimers();
    expect(sent.at(-1)).toEqual({ kind: "search", text: "bank" });
    push({
      kind: "search",
      text: "bank",
      items: [{ ...ITEM, title: "Bank of Example", username: "alex", matches: false }],
      error: null,
    });
    expect(screen.getByText("搜索结果 · 1")).toBeInTheDocument();
    expect(screen.getByText("alex · 不是当前网站")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "填充" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "复制密码" }));
    expect(sent.at(-1)).toEqual({ kind: "copy", itemId: ITEM.item_id, part: { type: "password" } });
    fireEvent.click(screen.getByRole("button", { name: "复制用户名" }));
    expect(sent.at(-1)).toEqual({ kind: "copy", itemId: ITEM.item_id, part: { type: "username" } });
    fireEvent.keyDown(search, { key: "Enter" });
    expect(sent.at(-1)).toEqual({ kind: "copy", itemId: ITEM.item_id, part: { type: "password" } });
    push({ kind: "search", text: "bank", items: [], error: "locked" });
    expect(screen.getByText("Lockra 已锁定。")).toBeInTheDocument();
  });

  it("offers to open Lockra on a site without logins, and says when a page cannot be filled", () => {
    const { push, sent } = setup(
      state({
        site: {
          site: "example.com",
          items: [],
          fillable: true,
          neverFill: false,
          crossOrigin: false,
        },
      }),
    );
    expect(screen.getByText("这个网站还没有登录项")).toBeInTheDocument();
    expect(screen.getByText("没有匹配")).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "打开 Lockra" })[0] as HTMLElement);
    expect(sent.at(-1)).toEqual({ kind: "start", show: true });
    push({
      kind: "state",
      state: state({
        site: { site: "", items: [], fillable: false, neverFill: false, crossOrigin: false },
      }),
    });
    expect(screen.getByText("这个页面不能填充")).toBeInTheDocument();
    push({ kind: "state", state: state({ site: null }) });
    expect(screen.getAllByText("连接 Lockra…").length).toBeGreaterThan(0);
  });

  it("asks for site access in the click, and says when it was refused", async () => {
    const requestAccess = vi.fn(async () => false);
    const { sent } = setup(state({ pageAccess: false }), { requestAccess });
    expect(screen.getByText("在网页中启用 Lockra", { selector: "p" })).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "在网页中启用 Lockra" }));
    });
    expect(requestAccess).toHaveBeenCalledTimes(1);
    expect(screen.getByText("没有获得网站访问权限，页面内的功能保持关闭。")).toBeInTheDocument();
    expect(sent.some((r) => r.kind === "access.changed")).toBe(false);
  });

  it("shows when Lockra paused the extension", () => {
    setup(state({ paused: true }));
    expect(screen.getByRole("alert")).toHaveTextContent("Lockra 暂停了这个扩展");
  });
});

describe("the popup's other states", () => {
  it("connects an unpaired extension in three steps, and says why a pairing failed", () => {
    const { sent, push } = setup(state({ screen: "unpaired" }));
    expect(screen.getByTestId("popup-state")).toHaveTextContent("未连接");
    expect(screen.getByText("连接到 Lockra")).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(screen.getByText("配对后可用快捷键填充")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    expect(sent.at(-1)).toEqual({ kind: "pair" });
    push({ kind: "state", state: state({ screen: "unpaired", pairingFailed: "denied" }) });
    expect(screen.getByRole("alert")).toHaveTextContent("没有连上：在 Lockra 中被拒绝。");
    push({ kind: "state", state: state({ screen: "unpaired", pairingFailed: "ended" }) });
    expect(screen.getByRole("alert")).toHaveTextContent("和 Lockra 的连接断开了。");
  });

  it("shows the code Lockra shows, and the time left", () => {
    vi.useFakeTimers();
    const start = 1_760_000_000_000;
    vi.setSystemTime(start);
    const { sent, push, now } = setup(
      state({ screen: "pairing", pairing: { code: "123456", expiresAt: null } }),
    );
    expect(screen.getByTestId("popup-state")).toHaveTextContent("等待批准");
    expect(screen.getByTestId("pairing-code")).toHaveTextContent("123 456");
    expect(screen.getByText("等待 Lockra…")).toBeInTheDocument();
    push({
      kind: "state",
      state: state({ screen: "pairing", pairing: { code: "123456", expiresAt: start + 112_000 } }),
    });
    expect(screen.getByText("112 秒后过期")).toBeInTheDocument();
    now.t = start + 2_000;
    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(screen.getByText("110 秒后过期")).toBeInTheDocument();
    vi.useRealTimers();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(sent.at(-1)).toEqual({ kind: "pair.cancel" });
  });

  it("starts Lockra when it is not running", () => {
    const { sent, push } = setup(state({ screen: "not_running" }));
    expect(screen.getByText("Lockra 没有运行")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "启动 Lockra" }));
    expect(sent.at(-1)).toEqual({ kind: "start", show: false });
    push({ kind: "state", state: state({ screen: "not_running", starting: true }) });
    expect(screen.getByRole("button", { name: "正在启动…" })).toBeDisabled();
  });

  it("asks Lockra to unlock, never for the master password", () => {
    const { sent, push } = setup(state({ screen: "locked" }));
    expect(screen.getByText("Lockra 已锁定")).toBeInTheDocument();
    expect(screen.getByText("解锁后，这里会显示 github.com 的登录项。")).toBeInTheDocument();
    expect(document.querySelector('input[type="password"]')).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "在 Lockra 中解锁" }));
    expect(sent.at(-1)).toEqual({ kind: "unlock" });
    push({ kind: "state", state: state({ screen: "locked", unlockAsked: true, site: null }) });
    expect(screen.getByText("已请 Lockra 解锁")).toBeInTheDocument();
    expect(screen.getByText("解锁后，这里会显示当前网站的登录项。")).toBeInTheDocument();
  });

  it("asks to update whichever side is older", () => {
    const { sent, push } = setup(state({ screen: "update_app", appVersion: "0.9.0" }));
    expect(screen.getByText("请更新 Lockra")).toBeInTheDocument();
    expect(
      screen.getByText("这个扩展（0.10.0）需要更新版本的 Lockra，正在运行的是 Lockra 0.9.0。"),
    ).toBeInTheDocument();
    push({ kind: "state", state: state({ screen: "update_extension", appVersion: "0.12.0" }) });
    expect(screen.getByText("请更新扩展")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "打开扩展页面" }));
    expect(sent.at(-1)).toEqual({ kind: "extensions" });
  });

  it("on Firefox, says where the extension updates instead of a button", () => {
    setup(state({ screen: "update_extension", appVersion: "0.12.0" }), { target: "firefox" });
    expect(screen.queryByRole("button", { name: "打开扩展页面" })).toBeNull();
    // Firefox installs no unpacked extension for good: its add-ons page, not Lockra's folder.
    expect(screen.getByText("在 Firefox 的附加组件页里更新扩展。")).toBeInTheDocument();
  });

  it("in Chromium, says that the extension loaded from Lockra's folder updates itself", () => {
    setup(state({ screen: "update_extension", appVersion: "0.12.0" }));
    expect(screen.getByText(/从 Lockra 的扩展文件夹加载的扩展会自动更新/)).toBeInTheDocument();
  });

  it("retries when no host answered, and opens Lockra when it has no vault", () => {
    const { sent, push } = setup(state({ screen: "no_host" }));
    expect(screen.getByText("没有找到 Lockra")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(sent.at(-1)).toEqual({ kind: "retry" });
    push({ kind: "state", state: state({ screen: "no_vault" }) });
    expect(screen.getByText("Lockra 还没有保险库")).toBeInTheDocument();
  });

  it("shows the connecting line before the first state, and the options page from the gear", () => {
    const { client } = setup(null);
    expect(screen.getByText("连接 Lockra…")).toBeInTheDocument();
    expect(screen.getByText("未设置填充快捷键")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "扩展设置" }));
    expect(client.openOptions).toHaveBeenCalled();
  });
});
