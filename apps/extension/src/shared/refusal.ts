// How a refusal of the app's (docs/formats.md §10), or one of the extension's own failures, reads.
import { type ExtensionT, extensionZhCN } from "@lockra/shared/i18n/extension";

type Worded = Exclude<keyof typeof extensionZhCN.refusal, "other">;

const WORDED = new Set(Object.keys(extensionZhCN.refusal).filter((key) => key !== "other"));

export function isWorded(code: string): code is Worded {
  return WORDED.has(code);
}

/** A refusal in words; one of a later version names its code. */
export function refusalText(t: ExtensionT, code: string): string {
  return isWorded(code) ? t(`refusal.${code}`) : t("refusal.other", { code });
}
