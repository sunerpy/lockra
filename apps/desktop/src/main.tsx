import { isTauri } from "@tauri-apps/api/core";
import { type Backend, TauriBackend } from "@lockra/shared";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { installContextMenuPolicy } from "./app/context-menu";
import "./index.css";

async function createBackend(): Promise<Backend> {
  // `pnpm dev` in a browser: an in-memory core with sample accounts. `import.meta.env.DEV` is a
  // build-time constant, so a release build carries neither this branch nor the mock module.
  if (import.meta.env.DEV && !isTauri()) {
    const { MockBackend, sampleBrowser, sampleEntries, sampleItems } =
      await import("@lockra/shared/mock");
    const entries = sampleEntries();
    const query = new URLSearchParams(location.search);
    // `?passwords`: password management, with the sample logins (a release build leaves it out).
    // `?browser` adds the browser extension's integration, paired and with some activity;
    // `?browser=pair` or `?browser=confirm` a request waiting for the user a few seconds in.
    const browser = query.get("browser");
    const passwords = query.has("passwords") || browser !== null;
    const backend = new MockBackend({
      entries,
      phase: "locked",
      ...(passwords ? { passwords, items: sampleItems(entries) } : {}),
      ...(browser !== null
        ? { browser: sampleBrowser(), settings: { browser_integration: true } }
        : {}),
    });
    if (browser !== null) await backend.browserEnable(["chrome", "edge", "firefox"]);
    if (browser === "pair")
      setTimeout(() => {
        backend.simulatePairRequest({
          code: "123456",
          browser: "chrome",
          label: "",
          extension: "egbpficliljjnpkjbigonldakfllebco",
        });
      }, 5_000);
    if (browser === "confirm")
      setTimeout(() => {
        backend.simulateConfirm({
          item_id: "00000000-0000-4000-8000-000000000000",
          title: "招商银行",
          username: "138 **** 5678",
          frame_url: "https://www.cmbchina.com/login",
          top_url: null,
          browser: "chrome",
          label: "",
          reasons: ["protected"],
          action: "fill",
          needs_proof: true,
        });
      }, 5_000);
    return backend;
  }
  return new TauriBackend();
}

// No webview context menu inside the client (text fields keep theirs); the browser preview keeps it.
installContextMenuPolicy(document, { enabled: isTauri() });

const container = document.getElementById("root");
if (!container) throw new Error("#root missing");
const backend = await createBackend();
/** The mounted app: a test unmounts it before its environment goes away. */
export const root = createRoot(container);
root.render(
  <StrictMode>
    <App backend={backend} />
  </StrictMode>,
);
