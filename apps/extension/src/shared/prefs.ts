// The extension's own choices, made on its options page and kept in storage.local beside the
// pairing: how and where its menu opens. The policy (what the extension may fill) is Lockra's, not
// here.
import type { StorageArea } from "../browser/api";

/** The inline menu opens as a sign-in field gets the focus (`true`, the default), or only from the
 *  field's Lockra mark. */
export const MENU_ON_FOCUS = "prefs.menu_on_focus";

export async function menuOnFocus(storage: StorageArea): Promise<boolean> {
  const got = await storage.get(MENU_ON_FOCUS).catch((): Record<string, unknown> => ({}));
  return got[MENU_ON_FOCUS] !== false;
}

export function setMenuOnFocus(storage: StorageArea, on: boolean): Promise<void> {
  return storage.set({ [MENU_ON_FOCUS]: on });
}

/** The inline menu opens above its field where the browser may list its own saved passwords under
 *  it (`true`, the default), or below the field like other menus, once the browser keeps none. */
export const AVOID_BROWSER_LIST = "prefs.avoid_browser_list";

export async function avoidBrowserList(storage: StorageArea): Promise<boolean> {
  const got = await storage.get(AVOID_BROWSER_LIST).catch((): Record<string, unknown> => ({}));
  return got[AVOID_BROWSER_LIST] !== false;
}

export function setAvoidBrowserList(storage: StorageArea, on: boolean): Promise<void> {
  return storage.set({ [AVOID_BROWSER_LIST]: on });
}
