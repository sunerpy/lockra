// What the page shows, measured: a field counts when it is rendered (CSS visibility, opacity,
// display, through `checkVisibility` where the browser has it), has a size, and is not pushed off
// the page (where honeypot fields hide).
import type { View } from "./fields";

export function shown(el: Element): boolean {
  if (!(el instanceof HTMLElement) || !el.isConnected) return false;
  if (
    typeof el.checkVisibility === "function" &&
    !el.checkVisibility({
      checkOpacity: true,
      checkVisibilityCSS: true,
      opacityProperty: true,
      visibilityProperty: true,
    })
  ) {
    return false;
  }
  const rect = el.getBoundingClientRect();
  if (rect.width < 2 || rect.height < 2) return false;
  const win = el.ownerDocument.defaultView;
  return rect.right + (win?.scrollX ?? 0) > 0 && rect.bottom + (win?.scrollY ?? 0) > 0;
}

export const pageView: View = { visible: shown };
