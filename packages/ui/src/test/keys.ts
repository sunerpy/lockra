import userEvent from "@testing-library/user-event";

/** Focuses `target`, types each of `keys` (user-event's syntax) and returns what a listener on
 *  `window` heard, where the app's shortcuts listen (apps/desktop/src/app/shortcuts.ts): "Ctrl+,",
 *  "Meta+k", "ArrowDown". */
export async function heardByWindow(target: HTMLElement, ...keys: string[]): Promise<string[]> {
  const user = userEvent.setup();
  const heard: string[] = [];
  const onKey = (event: KeyboardEvent) =>
    heard.push(`${event.ctrlKey ? "Ctrl+" : ""}${event.metaKey ? "Meta+" : ""}${event.key}`);
  window.addEventListener("keydown", onKey);
  try {
    target.focus();
    for (const key of keys) await user.keyboard(key);
  } finally {
    window.removeEventListener("keydown", onKey);
  }
  return heard;
}
