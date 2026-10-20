import type { ActivityView, PairedView } from "@lockra/shared";
import {
  MOCK_APP_VERSION,
  MOCK_EXTENSION_FOLDER,
  MOCK_PASSWORD,
  MockBackend,
  type MockOptions,
  sampleEntries,
} from "@lockra/shared/mock";
import { screen, waitFor, within } from "@testing-library/react";
import { ready, renderApp } from "../../test/render";

function paired(over: Partial<PairedView> = {}): PairedView {
  return {
    id: "9b3c0d1e-0000-4000-8000-000000000001",
    browser: "chrome",
    label: "办公室电脑",
    extension: "egbpficliljjnpkjbigonldakfllebco",
    paired_at_ms: Date.UTC(2026, 9, 12, 4),
    last_seen_ms: null,
    connected: true,
    page_access: true,
    version: null,
    paused: false,
    ...over,
  };
}

function core(options: MockOptions = {}): MockBackend {
  return new MockBackend({
    entries: sampleEntries(),
    passwords: true,
    ...options,
    settings: { locale: "zh-cn", ...options.settings },
  });
}

async function openBrowser(user: ReturnType<typeof renderApp>["user"]) {
  await user.keyboard("{Control>},{/Control}");
  const dialog = await screen.findByRole("dialog", { name: "设置" });
  await user.click(within(dialog).getByRole("tab", { name: "浏览器" }));
  return within(dialog);
}

describe("Settings › 浏览器", () => {
  it("is offered with password management only, between 同步 and 关于", async () => {
    const without = renderApp();
    await ready();
    await without.user.keyboard("{Control>},{/Control}");
    const dialog = within(await screen.findByRole("dialog", { name: "设置" }));
    expect(dialog.queryByRole("tab", { name: "浏览器" })).not.toBeInTheDocument();
    without.unmount();

    const { user } = renderApp({ backend: core() });
    await ready();
    await user.keyboard("{Control>},{/Control}");
    const tabs = within(await screen.findByRole("dialog", { name: "设置" }))
      .getAllByRole("tab")
      .map((tab) => tab.textContent);
    expect(tabs).toEqual(["通用", "外观", "安全", "同步", "浏览器", "关于"]);
  });

  it("turned off, shows its two switches and what turning it on brings", async () => {
    const { user } = renderApp({ backend: core() });
    await ready();
    const pane = await openBrowser(user);
    expect(pane.getByRole("switch", { name: "浏览器集成" })).not.toBeChecked();
    expect(pane.getByRole("switch", { name: "关闭窗口后继续运行" })).not.toBeChecked();
    expect(pane.getByTestId("browser-off-note")).toHaveTextContent("开启浏览器集成后");
    expect(pane.queryByRole("heading", { name: "已配对的扩展" })).not.toBeInTheDocument();
  });

  it("turning it on writes the browsers used here, then offers to keep running", async () => {
    const backend = core();
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    await user.click(pane.getByRole("switch", { name: "浏览器集成" }));
    const prompt = await screen.findByRole("dialog", { name: "关闭窗口后继续运行？" });
    expect((await backend.browserStatus()).filter((s) => s.manifest).map((s) => s.browser)).toEqual(
      ["chrome", "edge", "firefox"],
    );
    await user.click(within(prompt).getByRole("button", { name: "开启后台运行" }));
    expect(backend.calls.at(-1)).toMatchObject({
      command: "settings_set",
      settings: { run_in_background: true },
    });
    expect(pane.getByRole("switch", { name: "关闭窗口后继续运行" })).toBeChecked();
    expect(screen.queryByRole("dialog", { name: "关闭窗口后继续运行？" })).not.toBeInTheDocument();
    const rows = within(await pane.findByTestId("browser-rows"));
    expect(rows.getByTestId("browser-row-chrome")).toHaveTextContent("Chrome未安装扩展");
    expect(rows.queryByTestId("browser-row-brave")).not.toBeInTheDocument();
  });

  it("asks nothing more when Lockra already runs in the background, or the user says not now", async () => {
    const running = core({ settings: { run_in_background: true } });
    const first = renderApp({ backend: running });
    await ready();
    let pane = await openBrowser(first.user);
    await first.user.click(pane.getByRole("switch", { name: "浏览器集成" }));
    expect(await pane.findByTestId("browser-rows")).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "关闭窗口后继续运行？" })).not.toBeInTheDocument();
    first.unmount();

    const backend = core();
    const { user } = renderApp({ backend });
    await ready();
    pane = await openBrowser(user);
    await user.click(pane.getByRole("switch", { name: "浏览器集成" }));
    await user.click(
      within(await screen.findByRole("dialog", { name: "关闭窗口后继续运行？" })).getByRole(
        "button",
        { name: "暂不" },
      ),
    );
    expect(pane.getByRole("switch", { name: "关闭窗口后继续运行" })).not.toBeChecked();
    expect(backend.calls.some((c) => c.command === "settings_set")).toBe(false);
  });

  it("says what Lockra can tell of each browser, and writes a missing manifest", async () => {
    const backend = core({
      settings: { run_in_background: true },
      browser: { paired: [paired(), paired({ id: "p-edge", browser: "edge", connected: false })] },
    });
    await backend.browserEnable(["chrome", "edge"]);
    const { user } = renderApp({ backend });
    await ready();
    const rows = within(await (await openBrowser(user)).findByTestId("browser-rows"));
    expect(rows.getByTestId("browser-row-chrome")).toHaveTextContent("已连接");
    expect(rows.getByTestId("browser-row-edge")).toHaveTextContent("清单已写入");
    const firefox = within(rows.getByTestId("browser-row-firefox"));
    expect(rows.getByTestId("browser-row-firefox")).toHaveTextContent("还没有写入连接文件");
    await user.click(firefox.getByRole("button", { name: "写入" }));
    expect(await rows.findByText("未安装扩展")).toBeInTheDocument();
    expect((await backend.browserStatus()).find((s) => s.browser === "firefox")?.manifest).toBe(
      true,
    );
  });

  it("shows the folder Lockra keeps its extension in, puts its path on the clipboard and opens it", async () => {
    const backend = core({ settings: { run_in_background: true } });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const folder = within(await pane.findByTestId("extension-folder"));
    expect(folder.getByText(MOCK_EXTENSION_FOLDER)).toBeInTheDocument();
    expect(folder.getByTestId("extension-folder-state")).toHaveTextContent(
      `Lockra ${MOCK_APP_VERSION} 附带的扩展`,
    );
    expect(pane.getByTestId("extension-folder-steps")).toHaveTextContent("加载已解压的扩展程序");
    await user.click(folder.getByRole("button", { name: "复制路径" }));
    expect(backend.extensionCopies).toBe(1);
    expect(await folder.findByRole("button", { name: "已复制" })).toBeInTheDocument();
    await user.click(folder.getByRole("button", { name: "打开文件夹" }));
    expect(backend.extensionReveals).toBe(1);
    expect(folder.queryByText(/没能打开文件管理器/)).not.toBeInTheDocument();
    // Or the release's package, by hand: it opens in the browser.
    const download = within(pane.getByTestId("extension-download"));
    expect(download.getByText(/不会随 Lockra 自动更新/)).toBeInTheDocument();
    await user.click(download.getByRole("button", { name: "下载扩展安装包" }));
    expect(backend.extensionDownloads).toBe(1);
    expect(download.queryByTestId("extension-download-address")).not.toBeInTheDocument();
  });

  it("shows the package's address when no browser opened it, and offers it without a folder too", async () => {
    const backend = core({
      settings: { run_in_background: true },
      extensionFolder: null,
      extensionDownloadOpens: false,
    });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const download = within(await pane.findByTestId("extension-download"));
    await user.click(download.getByRole("button", { name: "下载扩展安装包" }));
    expect(await download.findByTestId("extension-download-address")).toHaveTextContent(
      `releases/download/v${MOCK_APP_VERSION}/lockra-extension_${MOCK_APP_VERSION}_chromium-unpacked.zip`,
    );
  });

  it("says when the folder does not hold this build's extension yet, and shows no folder without one", async () => {
    const pending = core({
      settings: { run_in_background: true },
      extensionFolder: { path: MOCK_EXTENSION_FOLDER, version: "0.10.2", ready: false },
    });
    await pending.browserEnable(["chrome"]);
    const first = renderApp({ backend: pending });
    await ready();
    const pane = await openBrowser(first.user);
    expect(await pane.findByTestId("extension-folder-state")).toHaveTextContent(
      "还没能写入这个文件夹",
    );
    first.unmount();

    const without = core({ settings: { run_in_background: true }, extensionFolder: null });
    await without.browserEnable(["chrome"]);
    const { user } = renderApp({ backend: without });
    await ready();
    const other = await openBrowser(user);
    expect(await other.findByTestId("browser-rows")).toBeInTheDocument();
    expect(other.queryByTestId("extension-folder")).not.toBeInTheDocument();
    expect(other.getByTestId("extension-download")).toBeInTheDocument();
  });

  it("tells each paired extension's version, and the ones older than Lockra", async () => {
    const backend = core({
      settings: { run_in_background: true },
      browser: {
        paired: [
          paired({ id: "p-chrome", version: MOCK_APP_VERSION }),
          paired({ id: "p-edge", browser: "edge", connected: false, version: "0.0.9" }),
          paired({ id: "p-firefox", browser: "firefox", connected: false }),
        ],
      },
    });
    await backend.browserEnable(["chrome", "edge", "firefox"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const chrome = await pane.findByTestId("paired-p-chrome");
    expect(chrome).toHaveTextContent(`扩展 ${MOCK_APP_VERSION}`);
    expect(chrome).not.toHaveTextContent("可更新");
    const edge = pane.getByTestId("paired-p-edge");
    expect(edge).toHaveTextContent("扩展 0.0.9");
    expect(edge).toHaveTextContent("可更新");
    expect(edge).toHaveTextContent(`比 Lockra ${MOCK_APP_VERSION} 旧`);
    expect(pane.getByTestId("paired-p-firefox")).not.toHaveTextContent("扩展 ");
  });

  it("turning it off removes every manifest", async () => {
    const backend = core({ settings: { run_in_background: true } });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    await user.click(pane.getByRole("switch", { name: "浏览器集成" }));
    expect(await pane.findByTestId("browser-off-note")).toBeInTheDocument();
    expect((await backend.browserStatus()).some((s) => s.manifest)).toBe(false);
  });

  it("lists the paired extensions, warns of missing site access, and unpairs after asking", async () => {
    const backend = core({
      settings: { run_in_background: true },
      browser: { paired: [paired({ page_access: false, last_seen_ms: Date.now() })] },
    });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const row = within(await pane.findByTestId("paired-9b3c0d1e-0000-4000-8000-000000000001"));
    expect(row.getByText("Chrome · 办公室电脑")).toBeInTheDocument();
    expect(row.getByText(/配对于 .* · 最近使用：刚刚/)).toBeInTheDocument();
    expect(row.getByText(/尚未获得网站访问权限/)).toBeInTheDocument();
    await user.click(row.getByRole("button", { name: "取消配对" }));
    const confirm = await screen.findByRole("dialog", {
      name: "取消配对「Chrome · 办公室电脑」？",
    });
    await user.click(within(confirm).getByRole("button", { name: "取消配对" }));
    expect(backend.calls.at(-1)).toEqual({
      command: "browser_unpair",
      id: "9b3c0d1e-0000-4000-8000-000000000001",
    });
    expect(await pane.findByText(/还没有配对的扩展/)).toBeInTheDocument();
  });

  it("shows a paused extension at the top, and resumes it", async () => {
    const backend = core({
      settings: { run_in_background: true },
      browser: { paired: [paired({ paused: true })] },
    });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const banner = within(await pane.findByTestId("browser-paused"));
    expect(banner.getByText("「Chrome · 办公室电脑」已暂停")).toBeInTheDocument();
    await user.click(banner.getByRole("button", { name: "恢复" }));
    expect(backend.calls.at(-1)).toEqual({ command: "browser_resume" });
    expect(pane.queryByTestId("browser-paused")).not.toBeInTheDocument();
  });

  it("changes the policy at once when it lets the extensions do less, after proof when more", async () => {
    const backend = core({ settings: { run_in_background: true } });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const fillOtp = await pane.findByRole("switch", { name: "填入验证码" });
    await user.click(fillOtp);
    expect(backend.calls.at(-1)).toMatchObject({
      command: "browser_set_policy",
      policy: { fill_otp: false },
    });
    expect(backend.calls.at(-1)).not.toHaveProperty("password");
    await user.click(fillOtp);
    const verify = within(await screen.findByTestId("verify-dialog"));
    await user.type(verify.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(screen.getByTestId("verify-submit"));
    expect(backend.calls.at(-1)).toEqual({
      command: "browser_set_policy",
      policy: expect.objectContaining({ fill_otp: true }),
      password: MOCK_PASSWORD,
    });
    expect(fillOtp).toBeChecked();
    await user.click(pane.getByRole("radio", { name: "先询问" }));
    expect(await screen.findByTestId("verify-dialog")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(pane.getByRole("radio", { name: "从不填充" })).toHaveAttribute("aria-checked", "true");
  });

  it("matches the port too once asked, both ways without proof", async () => {
    const backend = core({ settings: { run_in_background: true } });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const ports = await pane.findByRole("switch", { name: "匹配端口" });
    expect(ports).not.toBeChecked();
    expect(pane.getByText(/同一地址不同端口上的登录项分开填充/)).toBeInTheDocument();
    await user.click(ports);
    expect(backend.calls.at(-1)).toEqual({
      command: "browser_set_policy",
      policy: expect.objectContaining({ match_port: true }),
    });
    expect(await pane.findByRole("switch", { name: "匹配端口" })).toBeChecked();
    await user.click(ports);
    expect(backend.calls.at(-1)).toEqual({
      command: "browser_set_policy",
      policy: expect.objectContaining({ match_port: false }),
    });
    expect(screen.queryByTestId("verify-dialog")).not.toBeInTheDocument();
  });

  it("matches the full host once asked, right after the port, both ways without proof", async () => {
    const backend = core({ settings: { run_in_background: true } });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const host = await pane.findByRole("switch", { name: "按完整域名匹配" });
    const names = pane.getAllByRole("switch").map((s) => s.getAttribute("aria-label"));
    expect(names.indexOf("按完整域名匹配")).toBe(names.indexOf("匹配端口") + 1);
    expect(host).not.toBeChecked();
    expect(pane.getByText(/在 git.example.com 也会列出 wiki.example.com 的/)).toBeInTheDocument();
    await user.click(host);
    expect(backend.calls.at(-1)).toEqual({
      command: "browser_set_policy",
      policy: expect.objectContaining({ match_host: true, match_port: false }),
    });
    expect(await pane.findByRole("switch", { name: "按完整域名匹配" })).toBeChecked();
    await user.click(host);
    expect(backend.calls.at(-1)).toEqual({
      command: "browser_set_policy",
      policy: expect.objectContaining({ match_host: false }),
    });
    expect(screen.queryByTestId("verify-dialog")).not.toBeInTheDocument();
  });

  it("turns filling as pages load on after its risks and the proof", async () => {
    const backend = core({ settings: { run_in_background: true } });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const onLoad = await pane.findByRole("switch", { name: "页面加载时自动填充" });
    await user.click(onLoad);
    let risk = within(await screen.findByRole("dialog", { name: "开启页面加载时自动填充？" }));
    expect(risk.getByText(/任何一个子域里的一段恶意脚本/)).toBeInTheDocument();
    expect(risk.getAllByRole("listitem")).toHaveLength(6);
    await user.click(risk.getByRole("button", { name: "取消" }));
    expect(screen.queryByTestId("verify-dialog")).not.toBeInTheDocument();
    expect(onLoad).not.toBeChecked();

    await user.click(onLoad);
    risk = within(await screen.findByRole("dialog", { name: "开启页面加载时自动填充？" }));
    await user.click(risk.getByRole("button", { name: "继续…" }));
    const verify = within(await screen.findByTestId("verify-dialog"));
    await user.type(verify.getByLabelText("主密码"), MOCK_PASSWORD);
    await user.click(screen.getByTestId("verify-submit"));
    expect(backend.calls.at(-1)).toEqual({
      command: "browser_set_policy",
      policy: expect.objectContaining({ on_page_load: true }),
      password: MOCK_PASSWORD,
    });
    expect(onLoad).toBeChecked();
    // Off again goes at once.
    await user.click(onLoad);
    expect(backend.calls.at(-1)).toMatchObject({ policy: { on_page_load: false } });
  });

  it("adds and removes sites of the two lists", async () => {
    const backend = core({ settings: { run_in_background: true } });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const never = within(await pane.findByTestId("sites-never_fill"));
    expect(never.getByText("没有网站")).toBeInTheDocument();
    await user.click(never.getByRole("button", { name: "添加网站…" }));
    const add = within(await screen.findByRole("dialog", { name: "添加到「禁止填充」" }));
    await user.type(add.getByLabelText("网站"), "bank.example.cn");
    await user.click(add.getByRole("button", { name: "添加" }));
    // The rules changed with the logins' revision: the list is asked for again after the command.
    expect(backend.calls).toContainEqual({
      command: "site_rule_add",
      kind: "never_fill",
      domains: ["bank.example.cn"],
    });
    expect(await never.findByText("bank.example.cn")).toBeInTheDocument();
    expect(within(pane.getByTestId("sites-never_save")).getByText("没有网站")).toBeInTheDocument();
    await user.click(never.getByRole("button", { name: "移除「bank.example.cn」" }));
    expect(backend.calls.some((c) => c.command === "site_rule_remove")).toBe(true);
    expect(await never.findByText("没有网站")).toBeInTheDocument();
  });

  it("lists the sites matched their own way, each with its level, and removes one", async () => {
    const backend = core({ settings: { run_in_background: true } });
    await backend.browserEnable(["chrome"]);
    await backend.dispatch({
      command: "site_rule_add",
      kind: "match_host",
      domains: ["git.example.com"],
    });
    await backend.dispatch({
      command: "site_rule_add",
      kind: "match_domain",
      domains: ["wiki.example.com"],
    });
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const matching = within(await pane.findByTestId("sites-matching"));
    expect(matching.getByText("匹配方式")).toBeInTheDocument();
    expect(matching.getByText(/不跟随上面的默认设置/)).toBeInTheDocument();
    expect(matching.getAllByTestId("site-rule").map((row) => row.textContent)).toEqual([
      "git.example.com完整域名",
      "wiki.example.com根域名",
    ]);
    // The two lists keep to their own kinds.
    expect(within(pane.getByTestId("sites-never_fill")).getByText("没有网站")).toBeInTheDocument();
    expect(within(pane.getByTestId("sites-never_save")).getByText("没有网站")).toBeInTheDocument();
    const [git] = (await backend.getState()).site_rules;
    await user.click(matching.getByRole("button", { name: "移除「git.example.com」" }));
    expect(backend.calls).toContainEqual({ command: "site_rule_remove", id: git?.id });
    await waitFor(() => expect(matching.queryByText("git.example.com")).not.toBeInTheDocument());
    expect(matching.getAllByTestId("site-rule").map((row) => row.textContent)).toEqual([
      "wiki.example.com根域名",
    ]);
  });

  it("adds a site matched by its full host unless the user picks its domain", async () => {
    const backend = core({ settings: { run_in_background: true } });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const matching = within(await pane.findByTestId("sites-matching"));
    expect(matching.getByText("没有网站")).toBeInTheDocument();
    await user.click(matching.getByRole("button", { name: "添加网站…" }));
    let add = within(await screen.findByRole("dialog", { name: "添加到「匹配方式」" }));
    expect(add.getByRole("radio", { name: "完整域名" })).toHaveAttribute("aria-checked", "true");
    expect(add.getByText("它的子域也按这个方式匹配。")).toBeInTheDocument();
    await user.type(add.getByLabelText("网站"), "git.example.com");
    await user.click(add.getByRole("button", { name: "添加" }));
    expect(backend.calls).toContainEqual({
      command: "site_rule_add",
      kind: "match_host",
      domains: ["git.example.com"],
    });
    expect(await matching.findByText("完整域名")).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "添加到「匹配方式」" })).not.toBeInTheDocument();
    // The same site by its domain: the other way goes.
    await user.click(matching.getByRole("button", { name: "添加网站…" }));
    add = within(await screen.findByRole("dialog", { name: "添加到「匹配方式」" }));
    await user.type(add.getByLabelText("网站"), "git.example.com");
    await user.click(add.getByRole("radio", { name: "根域名" }));
    await user.click(add.getByRole("button", { name: "添加" }));
    expect(backend.calls).toContainEqual({
      command: "site_rule_add",
      kind: "match_domain",
      domains: ["git.example.com"],
    });
    expect(await matching.findByText("根域名")).toBeInTheDocument();
    expect(matching.getAllByTestId("site-rule").map((row) => row.textContent)).toEqual([
      "git.example.com根域名",
    ]);
  });

  it("shows the recent activity, and clears it", async () => {
    const at = Date.now();
    const activity: ActivityView[] = [
      {
        at_ms: at,
        browser: "chrome",
        label: "",
        site: "github.com",
        event: "filled",
        refusal: null,
        item_id: null,
        title: "GitHub",
      },
      {
        at_ms: at,
        browser: "firefox",
        label: "",
        site: "bank.example.cn",
        event: "refused",
        refusal: "never_fill",
        item_id: null,
        title: null,
      },
      {
        at_ms: at,
        browser: "edge",
        label: "",
        site: "",
        event: "paused",
        refusal: null,
        item_id: null,
        title: null,
      },
    ];
    const backend = core({ settings: { run_in_background: true }, browser: { activity } });
    await backend.browserEnable(["chrome"]);
    const { user } = renderApp({ backend });
    await ready();
    const pane = await openBrowser(user);
    const table = within(await pane.findByTestId("browser-activity"));
    expect(table.getByText("填充「GitHub」")).toBeInTheDocument();
    expect(table.getByText("拒绝：这个网站在「禁止填充」名单上。")).toBeInTheDocument();
    expect(table.getByText("已暂停：请求过于频繁")).toBeInTheDocument();
    expect(pane.getByText("只保存在内存里，锁定时清空")).toBeInTheDocument();
    await user.click(pane.getByRole("button", { name: "清空" }));
    expect(backend.calls.at(-1)).toEqual({ command: "browser_activity_clear" });
    expect(await pane.findByText("还没有活动。")).toBeInTheDocument();
  });
});
