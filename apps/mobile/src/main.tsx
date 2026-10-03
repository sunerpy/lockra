import { isTauri } from "@tauri-apps/api/core";
import { type Backend, TauriBackend } from "@lockra/shared";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import "./index.css";

async function createBackend(): Promise<Backend> {
  // `pnpm dev` in a browser: an in-memory core with sample accounts. `import.meta.env.DEV` is a
  // build-time constant, so a release bundle carries neither this branch nor the mock module.
  if (import.meta.env.DEV && !isTauri()) {
    const { MockBackend, sampleEntries } = await import("@lockra/shared/mock");
    return new MockBackend({ entries: sampleEntries(), phase: "locked" });
  }
  return new TauriBackend();
}

const container = document.getElementById("root");
if (!container) throw new Error("#root missing");
const backend = await createBackend();
createRoot(container).render(
  <StrictMode>
    <App backend={backend} />
  </StrictMode>,
);
