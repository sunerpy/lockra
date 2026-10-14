// When a click in the inline frame counts (plan §四 「防点击劫持」): every check below must pass,
// else the menu closes and nothing is filled.
// - the frame has shown, without moving, for at least 300 ms;
// - what lies on top at the click is the frame's host (`elementsFromPoint`), open in the top layer;
// - the host and every ancestor up to <html> are fully opaque, visible, and have no filter,
//   clip-path, mask or blend mode;
// - the frame's own IntersectionObserver v2 saw it visible (Chromium; Firefox has none: unknown);
// - the page did not touch the host (its attributes, its place) since it showed;
// - the click was the user's (`isTrusted`, checked in the frame and sent with it).
// The predicates are pure: jsdom has no layout, so the tests give the geometry and the styles.

export const MIN_VISIBLE_MS = 300;

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface StyleFacts {
  opacity: string;
  filter: string;
  clipPath: string;
  mixBlendMode: string;
  visibility: string;
  display: string;
  mask?: string | undefined;
}

export interface ClickFacts {
  trusted: boolean;
  now: number;
  /** When the frame last showed, moved or was raised again. */
  stableSince: number;
  rect: Rect;
  /** Its place then. */
  stableRect: Rect;
  /** The host is open in the top layer (`:popover-open`). */
  open: boolean;
  /** The topmost element at the click is the host. */
  topmost: boolean;
  /** The host's styles, then each ancestor's up to <html>. */
  styles: StyleFacts[];
  /** IntersectionObserver v2 in the frame: null where the browser has none. */
  ioVisible: boolean | null;
  tampered: boolean;
}

export type ClickRefusal =
  | "untrusted"
  | "tampered"
  | "too_soon"
  | "moved"
  | "covered"
  | "occluded"
  | "styled";

export function sameRect(a: Rect, b: Rect, tolerance = 1): boolean {
  return (
    Math.abs(a.left - b.left) <= tolerance &&
    Math.abs(a.top - b.top) <= tolerance &&
    Math.abs(a.width - b.width) <= tolerance &&
    Math.abs(a.height - b.height) <= tolerance
  );
}

const NONE = new Set(["none", ""]);

/** Nothing about this element makes what it shows other than what is drawn. */
export function styleClean(style: StyleFacts): boolean {
  return (
    Number(style.opacity) === 1 &&
    NONE.has(style.filter) &&
    NONE.has(style.clipPath) &&
    (style.mixBlendMode === "normal" || style.mixBlendMode === "") &&
    style.visibility === "visible" &&
    style.display !== "none" &&
    (style.mask === undefined || NONE.has(style.mask))
  );
}

export function clickAllowed(
  facts: ClickFacts,
): { ok: true } | { ok: false; reason: ClickRefusal } {
  if (!facts.trusted) return { ok: false, reason: "untrusted" };
  if (facts.tampered) return { ok: false, reason: "tampered" };
  if (!sameRect(facts.rect, facts.stableRect)) return { ok: false, reason: "moved" };
  if (facts.now - facts.stableSince < MIN_VISIBLE_MS) return { ok: false, reason: "too_soon" };
  if (!facts.open || !facts.topmost) return { ok: false, reason: "covered" };
  if (facts.ioVisible === false) return { ok: false, reason: "occluded" };
  if (!facts.styles.every(styleClean)) return { ok: false, reason: "styled" };
  return { ok: true };
}

/** What `measure` reads the page with: the real DOM, or a test's. */
export interface Measurer {
  elementsFromPoint(x: number, y: number): Element[];
  style(el: Element): StyleFacts;
}

export const domMeasurer: Measurer = {
  elementsFromPoint: (x, y) => document.elementsFromPoint(x, y),
  style: (el) => {
    const style = getComputedStyle(el);
    return {
      opacity: style.opacity,
      filter: style.filter,
      clipPath: style.clipPath,
      mixBlendMode: style.mixBlendMode,
      visibility: style.visibility,
      display: style.display,
      mask: style.maskImage || style.getPropertyValue("-webkit-mask-image") || undefined,
    };
  },
};

export function rectOf(el: Element): Rect {
  const r = el.getBoundingClientRect();
  return { left: r.left, top: r.top, width: r.width, height: r.height };
}

function popoverOpen(host: HTMLElement): boolean {
  // A DOM without popovers (the tests' jsdom): its place in the document is what can be checked.
  if (typeof host.showPopover !== "function") return host.isConnected;
  try {
    return host.matches(":popover-open");
  } catch {
    return host.isConnected;
  }
}

/** The facts of a click at `point` (in the frame's coordinates; null: a key, at the frame's middle). */
export function measure(
  host: HTMLElement,
  point: { x: number; y: number } | null,
  known: Pick<
    ClickFacts,
    "trusted" | "now" | "stableSince" | "stableRect" | "ioVisible" | "tampered"
  >,
  measurer: Measurer = domMeasurer,
): ClickFacts {
  const rect = rectOf(host);
  const x = rect.left + (point ? point.x : rect.width / 2);
  const y = rect.top + (point ? point.y : rect.height / 2);
  const styles: StyleFacts[] = [];
  for (let el: Element | null = host; el; el = el.parentElement) styles.push(measurer.style(el));
  return {
    ...known,
    rect,
    open: popoverOpen(host),
    topmost: measurer.elementsFromPoint(x, y)[0] === host,
    styles,
  };
}
