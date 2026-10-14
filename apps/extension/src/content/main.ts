// The content script, registered for every frame with site access, or put in one frame for a fill
// (activeTab). Once per frame, however often it is injected.
import { browserApi } from "../browser/api";
import { ContentAgent } from "./agent";
import { contentRuntime } from "./runtime";
import { pageView } from "./visibility";

const started = Symbol.for("lockra.content");
if (!Reflect.has(globalThis, started)) {
  Reflect.set(globalThis, started, true);
  const api = browserApi();
  void new ContentAgent({
    doc: document,
    win: window,
    view: pageView,
    now: () => Date.now(),
    runtime: contentRuntime(api),
  }).start();
}
