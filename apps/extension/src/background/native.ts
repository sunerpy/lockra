// The native-messaging port to the host `dev.lockra.browser` (PR 9's `lockra-desktop` in host mode),
// as the connection sees it. The browser frames the messages on stdio; the envelopes are JSON.
import type { BrowserApi } from "../browser/api";
import type { NativePort } from "./connection";

/** The host's name in every browser's manifest (crates/lockra-browser `HOST_NAME`). */
export const HOST_NAME = "dev.lockra.browser";

export function nativePort(api: BrowserApi): NativePort {
  const port = api.runtime.connectNative(HOST_NAME);
  return {
    post: (raw) => port.postMessage(raw),
    onMessage: (listener) => port.onMessage.addListener((message) => listener(message)),
    // Chromium says why in runtime.lastError (reading it there also keeps it off the console),
    // Firefox in port.error.
    onDisconnect: (listener) =>
      port.onDisconnect.addListener((closed) =>
        listener(api.runtime.lastError?.message ?? closed.error?.message ?? undefined),
      ),
    disconnect: () => port.disconnect(),
  };
}
