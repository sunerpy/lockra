// @vitest-environment node
// Every refusal of the protocol has its words, and the manifests' strings (_locales) are the i18n's.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { extensionEn, extensionTranslator, extensionZhCN } from "@lockra/shared/i18n/extension";
import { REFUSALS } from "../protocol/schemas";
import { isWorded, refusalText } from "./refusal";

function messages(locale: "en" | "zh_CN"): Record<string, { message: string }> {
  const path = resolve(import.meta.dirname, `../../public/_locales/${locale}/messages.json`);
  return JSON.parse(readFileSync(path, "utf8")) as Record<string, { message: string }>;
}

describe("the extension's words", () => {
  it("say every refusal of the protocol, and name a later one by its code", () => {
    expect(REFUSALS.filter((refusal) => !isWorded(refusal))).toEqual([]);
    const t = extensionTranslator("en");
    expect(refusalText(t, "no_match")).toBe("No login matches this site.");
    expect(refusalText(t, "later_refusal")).toBe("Not done (later_refusal).");
    expect(refusalText(t, "other")).toBe("Not done (other).");
  });

  it("give the manifests their name, description and the shortcut's name", () => {
    for (const [locale, dictionary] of [
      ["en", extensionEn],
      ["zh_CN", extensionZhCN],
    ] as const) {
      const file = messages(locale);
      expect({
        locale,
        name: file.name?.message,
        description: file.description?.message,
        command: file.commandFill?.message,
      }).toEqual({
        locale,
        name: dictionary.manifest.name,
        description: dictionary.manifest.description,
        command: dictionary.manifest.commandFill,
      });
    }
    // The stores' limit on a description.
    expect(extensionEn.manifest.description.length).toBeLessThanOrEqual(132);
  });
});
