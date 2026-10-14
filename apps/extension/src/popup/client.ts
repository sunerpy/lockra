// What the popup and the options page need of the browser: a port to the background, the site
// access request (made in the click, as both browsers require), the options page, the shortcuts.
import { type BrowserApi, TARGET } from "../browser/api";
import { ALL_SITES } from "../background/access";
import { FILL_COMMAND, type PopupEvent, type PopupRequest } from "../shared/messages";

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
    target: TARGET,
  };
}
