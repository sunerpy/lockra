import type { KeyboardEvent } from "react";

/** For the controls inside a row or a card (its buttons, its menu, a footer): a key pressed on them
 *  stays with them, so the list around them does not act on it too (its arrows do not move the
 *  row), except the app's shortcuts, which go on to the window that listens for them: Ctrl or ⌘
 *  with a key (Ctrl , the settings, Ctrl K the palette) and "/", which goes to the search field
 *  like Ctrl F. */
export function keepKey(event: KeyboardEvent): void {
  if (!event.ctrlKey && !event.metaKey && event.key !== "/") event.stopPropagation();
}
