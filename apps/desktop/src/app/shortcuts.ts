// The global shortcuts of the unlocked app (DESIGN.md, footer): Ctrl/⌘ K palette, N add, L lock,
// "," settings, F or "/" search. "/" only fires outside text fields.
import { useEffect } from "react";

export interface ShortcutHandlers {
  palette: () => void;
  add: () => void;
  lock: () => void;
  settings: () => void;
  search: () => void;
}

function typing(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
  );
}

export function shortcutFor(
  event: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey" | "target">,
): keyof ShortcutHandlers | undefined {
  const mod = event.ctrlKey || event.metaKey;
  if (event.altKey) return undefined;
  if (mod && !event.shiftKey) {
    switch (event.key.toLowerCase()) {
      case "k":
        return "palette";
      case "n":
        return "add";
      case "l":
        return "lock";
      case ",":
        return "settings";
      case "f":
        return "search";
      default:
        return undefined;
    }
  }
  if (!mod && event.key === "/" && !typing(event.target)) return "search";
  return undefined;
}

export function useShortcuts(handlers: ShortcutHandlers, enabled: boolean): void {
  useEffect(() => {
    if (!enabled) return undefined;
    const onKey = (event: KeyboardEvent) => {
      const which = shortcutFor(event);
      if (which === undefined) return;
      event.preventDefault();
      handlers[which]();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [handlers, enabled]);
}
