// Site access (plan §五 「网站访问权限的流程」): `<all_urls>` is optional in both browsers. Granted
// (the popup's or the options page's button, `permissions.request` in the click), the content
// script is registered for every frame and kept across sessions; revoked (`permissions.onRemoved`,
// or the browser's own settings), it is unregistered; every start checks the two agree. Without it
// only the popup and the shortcut work, through activeTab at the moment the user acts.
import type { BrowserApi, RegisteredContentScript } from "../browser/api";

export const ALL_SITES = { origins: ["<all_urls>"] } as const;

export const CONTENT_SCRIPT: RegisteredContentScript = {
  id: "lockra-content",
  matches: ["http://*/*", "https://*/*"],
  js: ["content.js"],
  allFrames: true,
  runAt: "document_idle",
  persistAcrossSessions: true,
};

export async function pageAccess(api: BrowserApi): Promise<boolean> {
  try {
    return await api.permissions.contains({ origins: [...ALL_SITES.origins] });
  } catch {
    return false;
  }
}

/** Register or unregister the content script to match the permission; whether the extension may
 *  run in pages. */
export async function syncContentScript(api: BrowserApi): Promise<boolean> {
  const access = await pageAccess(api);
  const registered =
    (await api.scripting.getRegisteredContentScripts({ ids: [CONTENT_SCRIPT.id] })).length > 0;
  if (access && !registered) await api.scripting.registerContentScripts([CONTENT_SCRIPT]);
  if (!access && registered)
    await api.scripting.unregisterContentScripts({ ids: [CONTENT_SCRIPT.id] });
  return access;
}
