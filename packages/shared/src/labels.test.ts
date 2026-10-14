import { ipcFixtures } from "./fixtures";
import { createTranslator } from "./i18n";
import {
  candidateSource,
  conflictText,
  entryLabel,
  errorText,
  formatBytes,
  groupCode,
  incompatibleText,
  noticeIsProblem,
  noticeText,
  browserName,
  pairingCode,
  originText,
  parametersText,
  rejectText,
  downloadProgress,
  relativeTime,
  statusText,
  statusVersion,
  syncStatusLine,
  syncSummary,
  updateStatusLine,
  themeName,
  themeSubtitle,
} from "./labels";
import {
  CONFLICT_REASONS,
  ERROR_CODES,
  INCOMPATIBLE,
  ORIGINS,
  REJECT_REASONS,
  type Notice,
  THEME_IDS,
  uiEventSchema,
} from "./schema";

const zh = createTranslator("zh-CN").t;
const en = createTranslator("en").t;

describe("labels", () => {
  it("names where a found account came from, and its line", () => {
    const base = {
      id: 0,
      origin: "uri",
      issuer: "GitHub",
      account: "",
      kind: null,
      algorithm: null,
      digits: null,
      status: { type: "new" },
      default_action: "add",
      login: null,
    } as const;
    expect(candidateSource(en, { ...base, source: { type: "text" }, line: 3 })).toBe(
      "Pasted text · line 3",
    );
    expect(candidateSource(zh, { ...base, source: { type: "clipboard" }, line: null })).toBe(
      "剪贴板",
    );
    expect(
      candidateSource(en, { ...base, source: { type: "file", name: "codes.png" }, line: null }),
    ).toBe("codes.png");
    expect(candidateSource(zh, { ...base, source: { type: "camera" }, line: null })).toBe("相机");
  });

  it("every code has words in both languages", () => {
    for (const t of [zh, en]) {
      for (const code of ERROR_CODES) expect(errorText(t, code)).not.toContain("error.");
      for (const reason of REJECT_REASONS) expect(rejectText(t, reason)).not.toContain("reject.");
      for (const reason of INCOMPATIBLE)
        expect(incompatibleText(t, reason)).not.toContain("incompatible.");
      for (const origin of ORIGINS) expect(originText(t, origin)).not.toContain("origin.");
      for (const reason of CONFLICT_REASONS)
        expect(conflictText(t, reason)).not.toContain("import.conflict.");
    }
    for (const theme of THEME_IDS) {
      expect(themeName(theme, "en")).not.toContain("theme.");
      expect(themeSubtitle(theme)).not.toContain("theme.");
    }
  });

  it("every notice in the fixtures reads as a sentence", () => {
    const notices = ipcFixtures.events
      .map((e) => uiEventSchema.parse(e))
      .flatMap((e) => (e.type === "notice" ? [e.notice] : []));
    const unfinished = notices.flatMap((notice) =>
      [zh, en].map((t) => noticeText(t, notice)).filter((text) => /notice\.|\{/.test(text)),
    );
    expect(unfinished).toEqual([]);
    expect(noticeText(en, { type: "backup_failed", code: "backup_dir_unavailable" })).toBe(
      "Automatic backup failed: The backup folder cannot be written",
    );
    expect(noticeIsProblem({ type: "backup_failed", code: "io_failed" })).toBe(true);
    expect(noticeIsProblem({ type: "auto_locked" })).toBe(false);
  });

  it("names browsers and groups pairing codes as both sides show them", () => {
    expect([
      browserName("chrome"),
      browserName("edge"),
      browserName("vivaldi"),
      browserName(""),
    ]).toEqual(["Chrome", "Edge", "vivaldi", "?"]);
    expect([pairingCode("123456"), pairingCode("12")]).toEqual(["123 456", "12"]);
    expect(
      noticeText(zh, {
        type: "browser_pair_requested",
        id: "r",
        browser: "chrome",
        code: "123456",
      }),
    ).toBe("Chrome 请求连接，代码 123 456");
    expect(
      noticeText(en, { type: "browser_paused", pairing: "p", browser: "edge", label: "" }),
    ).toBe("“Edge” asked too often and is paused");
    expect(
      noticeText(en, { type: "browser_paused", pairing: "p", browser: "edge", label: "Work" }),
    ).toBe("“Edge · Work” asked too often and is paused");
    expect(
      noticeText(zh, { type: "item_saved", item_id: "i", title: "GitHub", updated: false }),
    ).toBe("已在浏览器中保存「GitHub」");
  });

  it("an import's counts name the logins apart, and only when there were any", () => {
    const none = { added: 0, replaced: 0, skipped: 0 };
    const codes = { type: "imported", added: 2, replaced: 0, skipped: 1, logins: none } as const;
    expect(noticeText(zh, codes)).toBe("已导入 2 个，替换 0 个，跳过 1 个");
    const logins = { added: 3, replaced: 1, skipped: 2 };
    expect(noticeText(zh, { ...codes, ...none, logins })).toBe(
      "已导入 3 个登录项，替换 1 个，跳过 2 个",
    );
    expect(noticeText(en, { ...codes, ...none, logins })).toBe(
      "Imported 3 logins, replaced 1, skipped 2",
    );
    expect(noticeText(en, { ...codes, logins })).toBe(
      "Codes: imported 2, replaced 0, skipped 1; logins: imported 3, replaced 1, skipped 2",
    );
    expect(statusText(zh, { type: "login_conflict", item_id: "x", reason: "password" })).toBe(
      "与本机不同 · 密码不同：替换后，本机的旧密码保留在历史里",
    );
    expect(statusText(en, { type: "login_exists", item_id: "x" })).toBe("Already here");
  });

  it("a copy names the account when the caller knows it", () => {
    const copied = { type: "copied", entry_id: "e1", clear_after_s: 30 } as const;
    expect(noticeText(zh, copied, "GitHub")).toBe("已复制 GitHub 的验证码 · 30 秒后清空剪贴板");
    expect(noticeText(en, copied, "GitHub")).toBe(
      "Copied GitHub's code · the clipboard clears in 30 s",
    );
    expect(noticeText(zh, { ...copied, clear_after_s: null }, "GitHub")).toBe(
      "已复制 GitHub 的验证码",
    );
    // Without a name, as before.
    expect(noticeText(zh, copied)).toBe("已复制 · 30 秒后清空剪贴板");
  });

  it("what a sync run brought names the counts that are not zero, and where they came from", () => {
    const brought = {
      type: "sync_brought",
      added: 2,
      updated: 1,
      removed: 0,
      logins: 0,
      devices: ["MacBook", "Pixel 8"],
    } satisfies Notice;
    expect(noticeText(zh, brought)).toBe("已从 MacBook、Pixel 8 同步：新增 2 个，修改 1 个");
    expect(noticeText(en, brought)).toBe("Synced from MacBook, Pixel 8: 2 added, 1 changed");
    // Logins after the accounts, when there are any.
    const logins = { ...brought, added: 1, updated: 0, logins: 3 };
    expect(noticeText(zh, logins)).toBe("已从 MacBook、Pixel 8 同步：新增 1 个，3 个登录项有变化");
    expect(noticeText(en, logins)).toBe("Synced from MacBook, Pixel 8: 1 added, 3 logins changed");
    expect(noticeText(en, { ...logins, added: 0 })).toBe(
      "Synced from MacBook, Pixel 8: 3 logins changed",
    );
    const failed = { ...brought, added: 0, updated: 0, removed: 3, devices: [] };
    expect(noticeText(zh, failed)).toBe("已同步来自其他设备的更改：删除 3 个");
    expect(noticeText(en, failed)).toBe("Synced changes from other devices: 3 removed");
    expect(noticeIsProblem(brought)).toBe(false);
  });

  it("statuses, parameters and times", () => {
    expect(statusText(en, { type: "new" })).toBe("New");
    expect(statusText(en, { type: "unsupported", reason: "md5_algorithm" })).toBe(
      "Unsupported · Uses MD5, which is not supported",
    );
    expect(parametersText(en, { type: "totp", period: 30 }, "sha1", 6)).toBe("SHA1 · 6 · 30 s");
    expect(parametersText(zh, { type: "hotp", counter: 4 }, "sha256", 8)).toBe(
      "SHA256 · 8 · 计数器 4",
    );
    const now = Date.UTC(2026, 8, 30, 12);
    expect(relativeTime(en, now - 10_000, now)).toBe("just now");
    expect(relativeTime(en, now - 5 * 60_000, now)).toBe("5 minutes ago");
    expect(relativeTime(en, now - 3 * 3_600_000, now)).toBe("3 hours ago");
    expect(relativeTime(zh, now - 3 * 86_400_000, now)).toBe("3 天前");
    expect(relativeTime(en, now + 5000, now)).toBe("just now");
  });

  it("a sync space's last run, with its lamp", () => {
    const now = Date.UTC(2026, 8, 30, 12);
    expect(syncStatusLine({ state: "idle" }, en, now)).toEqual({
      tone: "idle",
      text: "Not synced yet",
    });
    expect(syncStatusLine({ state: "syncing" }, zh, now)).toEqual({
      tone: "accent",
      text: "正在同步…",
    });
    expect(syncStatusLine({ state: "synced", at_ms: now - 5 * 60_000 }, zh, now)).toEqual({
      tone: "ok",
      text: "已同步 · 5 分钟前",
    });
    expect(syncStatusLine({ state: "failed", code: "sync_denied", at_ms: now }, zh, now)).toEqual({
      tone: "danger",
      text: "同步失败：存储服务拒绝访问，请检查访问密钥或密码",
    });
  });

  it("sums a sync space up in one line for the status cards", () => {
    const device = { tag: "t", name: "Pixel", written_at_ms: null, this_device: false };
    const space = {
      storage: { kind: "relay" as const, url: "https://relay.example" },
      device_name: "本机",
      devices: [device, { ...device, tag: "u" }, { ...device, tag: "v", this_device: true }],
      status: { state: "synced" as const, at_ms: 1 },
      last_sync_ms: 1,
      rolled_back: [],
      unreadable: [],
      keyring_pending: false,
      keyring_unsealed: false,
      key_saved: true,
    };
    expect(syncSummary(zh, null)).toBe("未开启同步");
    expect(syncSummary(zh, space)).toBe("已同步 · 3 台设备");
    expect(syncSummary(en, { ...space, devices: [device] })).toBe("Synced · 1 device");
    expect(syncSummary(zh, { ...space, status: { state: "syncing" } })).toBe("正在同步…");
    expect(
      syncSummary(zh, { ...space, status: { state: "failed", code: "sync_denied", at_ms: 1 } }),
    ).toBe("同步失败");
    // Not run yet this session: the devices as the last run left them, or nothing to say yet.
    expect(syncSummary(zh, { ...space, status: { state: "idle" } })).toBe("已同步 · 3 台设备");
    expect(syncSummary(zh, { ...space, status: { state: "idle" }, last_sync_ms: null })).toBe(
      "尚未同步",
    );
  });

  it("the updater's state in one line, with its lamp", () => {
    const at = Date.UTC(2026, 8, 30, 12);
    const line = (update: Parameters<typeof updateStatusLine>[0]) =>
      updateStatusLine(update, "0.6.0", zh, "zh-CN");
    expect(line({ method: null, status: { state: "idle" } }).tone).toBe("idle");
    // Google Play updates a copy it installed: nothing to check.
    expect(line({ method: "play", status: { state: "idle" } })).toEqual({
      text: "由 Google Play 更新 · 当前 0.6.0",
      tone: "idle",
    });
    expect(line({ method: "android", status: { state: "idle" } })).toEqual({
      text: "尚未检查更新",
      tone: "idle",
    });
    expect(line({ method: "android", status: { state: "checking" } }).tone).toBe("accent");
    expect(
      line({ method: "android", status: { state: "up_to_date", checked_at_ms: at } }).text,
    ).toMatch(/^已是最新 · 0\.6\.0 · 检查于 /);
    const available = { state: "available" as const, version: "0.7.0", notes: null, date: null };
    expect(line({ method: "android", status: { ...available, checked_at_ms: at } })).toEqual({
      text: "有新版本 0.7.0 · 当前 0.6.0",
      tone: "accent",
    });
    const downloading = { state: "downloading" as const, version: "0.7.0", received: 5, total: 10 };
    expect(line({ method: "deb", status: downloading }).text).toBe("正在下载 0.7.0 · 50%");
    expect(line({ method: "deb", status: { state: "ready", version: "0.7.0" } }).tone).toBe("ok");
    expect(line({ method: "deb", status: { state: "installing", version: "0.7.0" } }).tone).toBe(
      "accent",
    );
    expect(
      line({ method: "android", status: { state: "failed", code: "update_network", at_ms: at } }),
    ).toEqual({ text: "更新失败 · 无法连接更新服务器", tone: "danger" });
    expect(downloadProgress(4_194_304, 11_508_084)).toBe("36%");
    expect(downloadProgress(3_145_728, null)).toBe("3.0 MB");
    expect(statusVersion({ state: "ready", version: "0.3.0" })).toBe("0.3.0");
    expect(statusVersion({ state: "checking" })).toBeUndefined();
  });

  it("codes are grouped for reading", () => {
    expect(groupCode("123456")).toBe("123 456");
    expect(groupCode("1234567")).toBe("123 4567");
    expect(groupCode("12345678")).toBe("1234 5678");
    expect(groupCode("12345")).toBe("12345");
    expect(entryLabel("GitHub", "octocat")).toBe("GitHub: octocat");
    expect(entryLabel("", "octocat")).toBe("octocat");
    expect(entryLabel("GitHub", "")).toBe("GitHub");
  });

  it("sizes read in KB and MB", () => {
    expect(formatBytes(0)).toBe("0 KB");
    expect(formatBytes(512 * 1024)).toBe("512 KB");
    expect(formatBytes(4 * 1024 * 1024 + 100_000)).toBe("4.1 MB");
  });
});
