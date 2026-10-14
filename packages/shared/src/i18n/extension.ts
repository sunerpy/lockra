// The browser extension's strings on their own (`@lockra/shared/i18n/extension`): its content and
// inline pages load these without the app's whole dictionaries. Keys are the `extension` namespace's,
// without the prefix (`popup.fill`).
import { extensionEn } from "./extension.en";
import { extensionZhCN } from "./extension.zh-CN";
import { type LeafPaths, type Locale, type Params, format } from "./runtime";

export { extensionEn } from "./extension.en";
export { extensionZhCN } from "./extension.zh-CN";
export { LOCALES, type Locale, type Params } from "./runtime";

export type ExtensionMessages = typeof extensionZhCN;
export type ExtensionKey = LeafPaths<ExtensionMessages>;

const DICTIONARIES: Readonly<Record<Locale, ExtensionMessages>> = {
  "zh-CN": extensionZhCN,
  en: extensionEn,
};

/** `zh-CN` for a Chinese browser, English otherwise (as `resolveLocale("system", …)`). */
export function extensionLocale(language: string): Locale {
  return language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}

export type ExtensionT = (key: ExtensionKey, params?: Params) => string;

export function extensionTranslator(locale: Locale): ExtensionT {
  return (key, params) => format(locale, DICTIONARIES[locale], key, params);
}
