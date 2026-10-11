import {
  DEFAULT_LOCALE,
  LOCALES,
  MESSAGES,
  type MessageTree,
  createTranslator,
  en,
  format,
  formatDateTime,
  interpolate,
  intlTag,
  leafPaths,
  lookup,
  pluralForm,
  resolveLocale,
  translate,
  zhCN,
  zhT,
} from "./index";

const CJK = /[一-鿿]/;

describe("i18n dictionaries", () => {
  it("every locale dictionary has the same key set", () => {
    const reference = leafPaths(zhCN);
    expect(reference.length).toBeGreaterThan(300);
    expect(new Set(reference).size).toBe(reference.length);
    for (const locale of LOCALES) {
      expect(new Set(leafPaths(MESSAGES[locale]))).toEqual(new Set(reference));
    }
  });

  it("the English dictionary carries no Chinese except the language's own name", () => {
    const tree: MessageTree = en;
    const offenders = leafPaths(tree).filter((path) => {
      if (path === "settings.general.locale.zh-cn") return false;
      const leaf = lookup(tree, path);
      const text = typeof leaf === "string" ? leaf : `${leaf?.one ?? ""}${leaf?.other ?? ""}`;
      return CJK.test(text);
    });
    expect(offenders).toEqual([]);
  });

  it("every leaf is non-empty and every plural has both forms", () => {
    const empty = LOCALES.flatMap((locale) => {
      const tree: MessageTree = MESSAGES[locale];
      return leafPaths(tree).filter((path) => {
        const leaf = lookup(tree, path);
        return typeof leaf === "string" ? leaf.length === 0 : !(leaf?.one && leaf.other);
      });
    });
    expect(empty).toEqual([]);
  });

  it("placeholders match between the two languages", () => {
    const text = (leaf: ReturnType<typeof lookup>) =>
      typeof leaf === "string" ? leaf : `${leaf?.one ?? ""} ${leaf?.other ?? ""}`;
    const names = (leaf: ReturnType<typeof lookup>) =>
      [...new Set([...text(leaf).matchAll(/\{(\w+)\}/g)].map((m) => m[1]))].join(",");
    const different = leafPaths(zhCN).filter((path) => {
      const zh = new Set(names(lookup(zhCN, path)).split(","));
      const english = new Set(names(lookup(en, path)).split(","));
      return zh.size !== english.size || [...zh].some((n) => !english.has(n));
    });
    expect(different).toEqual([]);
  });
});

describe("i18n runtime", () => {
  it("resolves the locale setting", () => {
    expect(resolveLocale("system", "zh-TW")).toBe("zh-CN");
    expect(resolveLocale("system", "fr-FR")).toBe("en");
    expect(resolveLocale("zh-cn", "en-US")).toBe("zh-CN");
    expect(resolveLocale("en", "zh-CN")).toBe("en");
    expect(intlTag("en")).toBe("en-US");
  });

  it("formats plurals, placeholders and missing keys", () => {
    expect(translate("en", "common.accounts", { n: 1 })).toBe("1 account");
    expect(translate("en", "common.accounts", { n: 3 })).toBe("3 accounts");
    expect(translate("zh-CN", "common.accounts", { n: 1 })).toBe("1 个账号");
    expect(pluralForm("zh-CN", 1)).toBe("other");
    expect(interpolate("{a}-{b}", { a: 1 })).toBe("1-{b}");
    expect(interpolate("plain", undefined)).toBe("plain");
    expect(format("en", en, "no.such.key", undefined)).toBe("no.such.key");
    expect(format("en", en, "common", undefined)).toBe("common");
    expect(format("en", en, "common.accounts", undefined)).toBe("{n} accounts");
  });

  it("caches translators and formats dates", () => {
    expect(createTranslator("en")).toBe(createTranslator("en"));
    expect(zhT.locale).toBe(DEFAULT_LOCALE);
    expect(zhT.t("shell.nav.codes")).toBe("验证码");
    expect(formatDateTime("en", Date.UTC(2026, 8, 30), { year: "numeric", timeZone: "UTC" })).toBe(
      "2026",
    );
  });
});
