// The content script's messaging with the background, over the browser's runtime.
import type { BrowserApi } from "../browser/api";
import type { Runtime } from "./agent";

export function contentRuntime(api: BrowserApi): Runtime {
  return {
    send: (message) => api.runtime.sendMessage(message),
    listen: (handler) =>
      api.runtime.onMessage.addListener((message, sender, sendResponse) => {
        // Only the extension itself reaches a content script; anything else is not ours.
        if (sender.id !== api.runtime.id) return undefined;
        const answer = handler(message);
        if (!answer) return undefined;
        answer.then(sendResponse, () => sendResponse(undefined));
        return true;
      }),
    url: (path) => api.runtime.getURL(path),
  };
}
