// The inline frame's port to the background.
import type { BrowserApi } from "../browser/api";
import { type BackgroundToInline, PORT_INLINE } from "../shared/messages";
import type { InlinePort } from "./page";

function isFromBackground(value: unknown): value is BackgroundToInline {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    (value.kind === "view" || value.kind === "close")
  );
}

export function inlinePort(api: BrowserApi): InlinePort {
  const port = api.runtime.connect({ name: PORT_INLINE });
  return {
    post: (message) => port.postMessage(message),
    onMessage: (listener) =>
      port.onMessage.addListener((message) => {
        if (isFromBackground(message)) listener(message);
      }),
    onDisconnect: (listener) => port.onDisconnect.addListener(() => listener()),
  };
}
