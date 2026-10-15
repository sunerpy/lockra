import { en } from "./en";
import { extensionEn, extensionLocale, extensionTranslator, extensionZhCN } from "./extension";
import { zhCN } from "./zh-CN";

describe("the extension's strings", () => {
  it("are the dictionaries' `extension` namespace", () => {
    expect(zhCN.extension).toBe(extensionZhCN);
    expect(en.extension).toBe(extensionEn);
  });

  it("name Lockra's settings in Lockra's own words", () => {
    expect(extensionEn.notRunning.note).toContain(`“${en.settings.browser.background}”`);
    expect(extensionZhCN.notRunning.note).toContain(`「${zhCN.settings.browser.background}」`);
  });

  it("follow the browser's language and format like the app's", () => {
    expect(extensionLocale("zh-TW")).toBe("zh-CN");
    expect(extensionLocale("en-GB")).toBe("en");
    expect(extensionLocale("fr")).toBe("en");
    const zh = extensionTranslator("zh-CN");
    const english = extensionTranslator("en");
    expect(zh("popup.matches", { n: 2 })).toBe("2 个匹配");
    expect(english("popup.matches", { n: 1 })).toBe("1 match");
    expect(english("inline.matches", { site: "github.com", n: 2 })).toBe("github.com · 2 matches");
    expect(zh("refusal.other", { code: "later" })).toBe("没有完成（later）。");
  });
});
