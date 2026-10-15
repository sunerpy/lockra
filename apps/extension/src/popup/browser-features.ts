// The browser's own password saving and its address and card autofill, which compete with the
// extension in the same fields: who holds them, and taking them over for Lockra or giving them back
// (options page). Both go through the optional `privacy` permission, asked for in the click and
// removed once nothing is held. The browser's list of passwords it already keeps is none of
// these: it shows them in its own drop-down until the user deletes them there.
import { type BrowserApi, type BrowserSetting, TARGET } from "../browser/api";

export const PRIVACY = { permissions: ["privacy"] };

/** Who has a feature: the browser (its user's value), Lockra (turned off for it), or neither can
 *  change it (another extension or a policy holds it). */
export type Holder = "browser" | "lockra" | "locked";

export interface BrowserFeatures {
  /** The browser's offer to save passwords. */
  passwords: Holder;
  /** Its address and card autofill; `null` where an extension cannot change them (Firefox). */
  forms: Holder | null;
}

export type Feature = "passwords" | "forms";

type Target = "chromium" | "firefox";

function settings(api: BrowserApi, feature: Feature): BrowserSetting<boolean>[] {
  const services = api.privacy?.services;
  if (!services) return [];
  if (feature === "passwords") return [services.passwordSavingEnabled];
  return [services.autofillAddressEnabled, services.autofillCreditCardEnabled].filter(
    (setting): setting is BrowserSetting<boolean> => setting !== undefined,
  );
}

async function holder(list: BrowserSetting<boolean>[]): Promise<Holder> {
  if (list.length === 0) return "browser";
  const read = await Promise.all(list.map((setting) => setting.get({}).catch(() => null)));
  if (read.every((r) => r?.levelOfControl === "controlled_by_this_extension" && !r.value))
    return "lockra";
  const blocked = read.some(
    (r) =>
      r?.levelOfControl === "not_controllable" ||
      r?.levelOfControl === "controlled_by_other_extensions",
  );
  return blocked ? "locked" : "browser";
}

export async function readFeatures(
  api: BrowserApi,
  target: Target = TARGET,
): Promise<BrowserFeatures> {
  const forms = target === "chromium";
  const granted = await api.permissions.contains(PRIVACY).catch(() => false);
  if (!granted) return { passwords: "browser", forms: forms ? "browser" : null };
  return {
    passwords: await holder(settings(api, "passwords")),
    forms: forms ? await holder(settings(api, "forms")) : null,
  };
}

/** Turn `feature` off for Lockra (`take`), or give the browser its own value back. Taking asks for
 *  the permission, so it is called from the click: nothing is awaited before the request. */
export async function setFeature(
  api: BrowserApi,
  feature: Feature,
  take: boolean,
  target: Target = TARGET,
): Promise<BrowserFeatures> {
  if (take) {
    if (!(await api.permissions.request(PRIVACY).catch(() => false)))
      return readFeatures(api, target);
    await Promise.all(
      settings(api, feature).map((s) => s.set({ value: false }).catch(() => undefined)),
    );
    return readFeatures(api, target);
  }
  await Promise.all(settings(api, feature).map((s) => s.clear({}).catch(() => undefined)));
  const now = await readFeatures(api, target);
  if (now.passwords === "lockra" || now.forms === "lockra") return now;
  // Nothing held any more: the permission goes too.
  await api.permissions.remove(PRIVACY).catch(() => false);
  return readFeatures(api, target);
}

/** The page of the browser's own password manager, where an extension may open one (Chromium). */
export function passwordManagerUrl(userAgent: string, target: Target = TARGET): string | null {
  if (target !== "chromium") return null;
  return /\bEdg\//.test(userAgent)
    ? "edge://wallet/passwords"
    : "chrome://password-manager/passwords";
}
