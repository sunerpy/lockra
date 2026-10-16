// What the popup and the options page need of the browser: a port to the background, the site
// access request (made in the click, as both browsers require), the options page, the shortcuts,
// the browser's own password features and how and where the inline menu opens.
import { type BrowserApi, TARGET } from "../browser/api";
import { ALL_SITES } from "../background/access";
import { FILL_COMMAND, type PopupEvent, type PopupRequest } from "../shared/messages";
import {
  avoidBrowserList,
  menuOnFocus,
  setAvoidBrowserList,
  setMenuOnFocus,
} from "../shared/prefs";
import {
  type BrowserFeatures,
  type Feature,
  passwordManagerUrl,
  readFeatures,
  setFeature,
} from "./browser-features";

export interface PageClient {
  connect: (onEvent: (event: PopupEvent) => void) => {
    send: (request: PopupRequest) => void;
    close: () => void;
  };
  /** `permissions.request` for every site: call it from the click handler. */
  requestAccess: () => Promise<boolean>;
  /** `permissions.remove` for every site. */
  removeAccess: () => Promise<boolean>;
  openOptions: () => void;
  /** Close the popup (after a fill). */
  close: () => void;
  /** Copy text the extension shows (the source link), never a secret. */
  copyText: (text: string) => Promise<void>;
  /** The keys of the fill shortcut and of the popup's, as the browser shows them ("" when unset). */
  shortcuts: () => Promise<{ fill: string; popup: string }>;
  /** Who holds the browser's own password saving and its address and card autofill. */
  browserFeatures: () => Promise<BrowserFeatures>;
  /** Take one over for Lockra or give it back; taking asks for a permission: call it from the
   *  click. */
  setBrowserFeature: (feature: Feature, take: boolean) => Promise<BrowserFeatures>;
  /** Open the browser's own password manager, where an extension may (Chromium). */
  openPasswordManager: (() => void) | null;
  /** The inline menu opens as a sign-in field gets the focus, or only from the field's mark. */
  menuOnFocus: () => Promise<boolean>;
  setMenuOnFocus: (on: boolean) => Promise<void>;
  /** The inline menu opens above the field where the browser may list its own passwords under it,
   *  or below the field. */
  avoidBrowserList: () => Promise<boolean>;
  setAvoidBrowserList: (on: boolean) => Promise<void>;
  target: "chromium" | "firefox";
}

const EVENT_KINDS = new Set(["state", "search", "result"]);

function isEvent(value: unknown): value is PopupEvent {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    typeof value.kind === "string" &&
    EVENT_KINDS.has(value.kind)
  );
}

export function browserClient(api: BrowserApi, portName: string, win: Window): PageClient {
  return {
    connect(onEvent) {
      const port = api.runtime.connect({ name: portName });
      port.onMessage.addListener((message) => {
        if (isEvent(message)) onEvent(message);
      });
      return {
        send: (request) => {
          try {
            port.postMessage(request);
          } catch {
            // The background restarted: the next opening connects again.
          }
        },
        close: () => port.disconnect(),
      };
    },
    requestAccess: () =>
      api.permissions.request({ origins: [...ALL_SITES.origins] }).catch(() => false),
    removeAccess: () =>
      api.permissions.remove({ origins: [...ALL_SITES.origins] }).catch(() => false),
    openOptions: () => void api.runtime.openOptionsPage().catch(() => undefined),
    close: () => win.close(),
    copyText: (text) => win.navigator.clipboard.writeText(text),
    shortcuts: async () => {
      const commands = await api.commands.getAll().catch(() => []);
      const keys = (name: string) => commands.find((c) => c.name === name)?.shortcut ?? "";
      return { fill: keys(FILL_COMMAND), popup: keys("_execute_action") };
    },
    browserFeatures: () => readFeatures(api),
    setBrowserFeature: (feature, take) => setFeature(api, feature, take),
    openPasswordManager: (() => {
      const url = passwordManagerUrl(win.navigator.userAgent);
      return url ? () => void api.tabs.create({ url }).catch(() => undefined) : null;
    })(),
    menuOnFocus: () => menuOnFocus(api.storage.local),
    setMenuOnFocus: (on) => setMenuOnFocus(api.storage.local, on),
    avoidBrowserList: () => avoidBrowserList(api.storage.local),
    setAvoidBrowserList: (on) => setAvoidBrowserList(api.storage.local, on),
    target: TARGET,
  };
}
