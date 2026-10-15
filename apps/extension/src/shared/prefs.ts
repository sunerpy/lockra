// The extension's own choices, made on its options page and kept in storage.local beside the
// pairing: how its menu opens. The policy (what the extension may fill) is Lockra's, not here.
import type { StorageArea } from "../browser/api";

/** The inline menu opens as a sign-in field gets the focus (`true`, the default), or only from the
 *  field's Lockra mark, where it does not cover the browser's own list of saved passwords. */
export const MENU_ON_FOCUS = "prefs.menu_on_focus";

export async function menuOnFocus(storage: StorageArea): Promise<boolean> {
  const got = await storage.get(MENU_ON_FOCUS).catch((): Record<string, unknown> => ({}));
  return got[MENU_ON_FOCUS] !== false;
}

export function setMenuOnFocus(storage: StorageArea, on: boolean): Promise<void> {
  return storage.set({ [MENU_ON_FOCUS]: on });
}
