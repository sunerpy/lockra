import { ipcFixtures } from "./fixtures";
import { createTranslator } from "./i18n";
import {
  entryLabel,
  errorText,
  groupCode,
  incompatibleText,
  noticeIsProblem,
  noticeText,
  originText,
  parametersText,
  rejectText,
  relativeTime,
  statusText,
  themeName,
  themeSubtitle,
} from "./labels";
import {
  ERROR_CODES,
  INCOMPATIBLE,
  ORIGINS,
  REJECT_REASONS,
  THEME_IDS,
  uiEventSchema,
} from "./schema";

const zh = createTranslator("zh-CN").t;
const en = createTranslator("en").t;

describe("labels", () => {
  it("every code has words in both languages", () => {
    for (const t of [zh, en]) {
      for (const code of ERROR_CODES) expect(errorText(t, code)).not.toContain("error.");
      for (const reason of REJECT_REASONS) expect(rejectText(t, reason)).not.toContain("reject.");
      for (const reason of INCOMPATIBLE)
        expect(incompatibleText(t, reason)).not.toContain("incompatible.");
      for (const origin of ORIGINS) expect(originText(t, origin)).not.toContain("origin.");
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

  it("codes are grouped for reading", () => {
    expect(groupCode("123456")).toBe("123 456");
    expect(groupCode("1234567")).toBe("123 4567");
    expect(groupCode("12345678")).toBe("1234 5678");
    expect(groupCode("12345")).toBe("12345");
    expect(entryLabel("GitHub", "octocat")).toBe("GitHub: octocat");
    expect(entryLabel("", "octocat")).toBe("octocat");
    expect(entryLabel("GitHub", "")).toBe("GitHub");
  });
});
