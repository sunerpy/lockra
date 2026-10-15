// The browser's own password saving and its address and card autofill, taken over for Lockra and
// given back through the optional `privacy` permission, on the fake browser's settings.
import { FakeBrowser } from "../test/fake-browser";
import { passwordManagerUrl, readFeatures, setFeature } from "./browser-features";

const CHROME =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/141.0.0.0 Safari/537.36";
const EDGE = `${CHROME} Edg/141.0.0.0`;

describe("the browser's own password features", () => {
  it("are the browser's until the permission is granted", async () => {
    const browser = new FakeBrowser();
    expect(await readFeatures(browser, "chromium")).toEqual({
      passwords: "browser",
      forms: "browser",
    });
    // Firefox lets an extension change the password saving alone.
    expect(await readFeatures(browser, "firefox")).toEqual({ passwords: "browser", forms: null });
  });

  it("takes the password saving in the click, and gives it back with the permission", async () => {
    const browser = new FakeBrowser();
    expect(await setFeature(browser, "passwords", true, "chromium")).toEqual({
      passwords: "lockra",
      forms: "browser",
    });
    expect(browser.privacyGranted).toBe(true);
    expect([
      browser.services.passwordSavingEnabled.holder,
      browser.services.passwordSavingEnabled.value,
    ]).toEqual(["us", false]);
    expect(browser.services.autofillAddressEnabled.holder).toBe("user");
    expect(await setFeature(browser, "passwords", false, "chromium")).toEqual({
      passwords: "browser",
      forms: "browser",
    });
    expect([
      browser.services.passwordSavingEnabled.holder,
      browser.services.passwordSavingEnabled.value,
    ]).toEqual(["user", true]);
    // Nothing held any more: the permission goes as well.
    expect(browser.privacyGranted).toBe(false);
  });

  it("turns the address and card autofill off together, apart from the password saving", async () => {
    const browser = new FakeBrowser();
    await setFeature(browser, "forms", true, "chromium");
    expect(await setFeature(browser, "passwords", true, "chromium")).toEqual({
      passwords: "lockra",
      forms: "lockra",
    });
    expect([
      browser.services.autofillAddressEnabled.value,
      browser.services.autofillCreditCardEnabled.value,
    ]).toEqual([false, false]);
    // One given back, the other still held: the permission stays.
    expect(await setFeature(browser, "passwords", false, "chromium")).toEqual({
      passwords: "browser",
      forms: "lockra",
    });
    expect(browser.privacyGranted).toBe(true);
  });

  it("says when another extension or a policy holds a setting, and changes nothing when the user declines", async () => {
    const browser = new FakeBrowser();
    browser.services.passwordSavingEnabled.holder = "other";
    expect((await setFeature(browser, "passwords", true, "chromium")).passwords).toBe("locked");
    browser.services.passwordSavingEnabled.holder = "policy";
    expect((await readFeatures(browser, "chromium")).passwords).toBe("locked");

    const declined = new FakeBrowser();
    declined.privacyAnswer = false;
    expect(await setFeature(declined, "passwords", true, "chromium")).toEqual({
      passwords: "browser",
      forms: "browser",
    });
    expect(declined.services.passwordSavingEnabled.holder).toBe("user");
  });

  it("opens the browser's own password manager where an extension may", () => {
    expect(passwordManagerUrl(CHROME, "chromium")).toBe("chrome://password-manager/passwords");
    expect(passwordManagerUrl(EDGE, "chromium")).toBe("edge://wallet/passwords");
    expect(passwordManagerUrl(CHROME, "firefox")).toBeNull();
  });
});
