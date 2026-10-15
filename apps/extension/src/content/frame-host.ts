// The page's end of an inline frame (plan §四 「防点击劫持」): an extension-origin iframe in a closed
// shadow root, shown with `popover="manual"` in the top layer, styled inline with !important so
// that no page rule reaches it, and raised again when the page opens its own top-layer element.
// The page touching the host (an attribute, its place) counts as tampering: the frame goes. A click
// in the frame only counts after `verify` (clickjack.ts) says so.
import {
  type ClickRefusal,
  type Measurer,
  type Rect,
  clickAllowed,
  domMeasurer,
  measure,
  rectOf,
  sameRect,
} from "./clickjack";

export const MENU_WIDTH = 340;
export const PROMPT_WIDTH = 360;
const GAP = 6;
const EDGE = 8;

/** The cards' `shadow-pop` and corners in 澄空 and 澄空 · 夜, from tokens.css through the build
 *  (scripts/frame-look.mjs), drawn by the host in the page: inside the frame the shadow would end
 *  at the frame's edge. */
const LOOK = LOCKRA_FRAME_LOOK;

export interface HostOptions {
  doc: Document;
  /** The inline frame's page: `runtime.getURL("inline.html")` (a dynamic URL in Chromium). */
  url: string;
  token: string;
  kind: "menu" | "prompt";
  now: () => number;
  /** The page changed the host: the frame must go. */
  onTampered: () => void;
  measurer?: Measurer;
}

type Place = { anchor: HTMLElement } | "corner";

export class FrameHost {
  readonly host: HTMLDivElement;
  /** The iframe, for the tests (the shadow root is closed to the page). */
  readonly frame: HTMLIFrameElement;
  readonly token: string;
  readonly kind: "menu" | "prompt";
  private readonly hostObserver: MutationObserver;
  private readonly placeObserver: MutationObserver;
  private place: Place = "corner";
  private height = 0;
  private stableSince = 0;
  private stableRect: Rect = { left: 0, top: 0, width: 0, height: 0 };
  private tampered = false;
  private shown = false;

  constructor(private readonly options: HostOptions) {
    const { doc } = options;
    this.token = options.token;
    this.kind = options.kind;
    this.host = doc.createElement("div");
    this.host.setAttribute("popover", "manual");
    const root = this.host.attachShadow({ mode: "closed" });
    // The frame is the extension's own page: it needs its origin (for the extension's messaging)
    // and its scripts, which is all a sandbox would leave it.
    // oxlint-disable-next-line react/iframe-missing-sandbox
    this.frame = doc.createElement("iframe");
    this.frame.src = `${options.url}#${options.token}`;
    this.frame.title = "Lockra";
    this.frame.setAttribute(
      "style",
      "all: initial !important; display: block !important; width: 100% !important; height: 100% !important; border: 0 !important; color-scheme: light dark !important; background: transparent !important;",
    );
    root.append(this.frame);
    this.hostObserver = new MutationObserver(() => this.tamper());
    this.placeObserver = new MutationObserver((records) => {
      if (records.some((r) => Array.from(r.removedNodes).includes(this.host))) this.tamper();
    });
  }

  /** Show it under `anchor` (a menu) or at the top right (a prompt). */
  show(place: Place): void {
    this.place = place;
    this.options.doc.documentElement.append(this.host);
    this.hostObserver.observe(this.host, { attributes: true });
    this.placeObserver.observe(this.options.doc.documentElement, { childList: true });
    this.openPopover();
    this.shown = true;
    this.layout();
    // Shown now: the 300 ms start here, wherever it landed.
    this.stableRect = rectOf(this.host);
    this.stableSince = this.options.now();
  }

  /** The frame's content height, as it measured itself. */
  resize(height: number): void {
    this.height = height;
    this.layout();
  }

  /** Follow the anchor (a scroll, a resize): a move restarts the 300 ms. */
  layout(): void {
    if (!this.shown) return;
    const doc = this.options.doc;
    const win = doc.defaultView;
    const viewportWidth = win?.innerWidth ?? 1024;
    const viewportHeight = win?.innerHeight ?? 768;
    let left: number;
    let top: number;
    let width: number;
    if (this.place === "corner") {
      width = Math.min(PROMPT_WIDTH, viewportWidth - 2 * EDGE);
      left = viewportWidth - width - 16;
      top = EDGE;
    } else {
      const anchor = this.place.anchor.getBoundingClientRect();
      width = Math.min(Math.max(MENU_WIDTH, Math.round(anchor.width)), viewportWidth - 2 * EDGE);
      left = Math.min(Math.max(EDGE, anchor.left), viewportWidth - width - EDGE);
      top = anchor.bottom + GAP;
      // No room below: above the field.
      if (top + this.height > viewportHeight - EDGE && anchor.top - GAP - this.height >= EDGE)
        top = anchor.top - GAP - this.height;
    }
    this.style(Math.round(left), Math.round(top), Math.round(width), Math.round(this.height));
    const rect = rectOf(this.host);
    if (!sameRect(rect, this.stableRect, 0)) {
      this.stableRect = rect;
      this.stableSince = this.options.now();
    }
  }

  /** The page opened a top-layer element of its own: back on top, and the 300 ms start over. */
  raise(): void {
    if (!this.shown) return;
    try {
      this.host.hidePopover();
    } catch {
      // Not open (or no popover support): showing it again is enough.
    }
    this.openPopover();
    this.stableSince = this.options.now();
  }

  /** May a click at `point` (the frame's coordinates) count? */
  verify(
    point: { x: number; y: number } | null,
    trusted: boolean,
    visible: boolean | null,
  ): { ok: true } | { ok: false; reason: ClickRefusal } {
    if (!this.shown || !this.host.isConnected) return { ok: false, reason: "tampered" };
    const facts = measure(
      this.host,
      point,
      {
        trusted,
        now: this.options.now(),
        stableSince: this.stableSince,
        stableRect: this.stableRect,
        ioVisible: visible,
        tampered: this.tampered,
      },
      this.options.measurer ?? domMeasurer,
    );
    return clickAllowed(facts);
  }

  remove(): void {
    this.shown = false;
    this.hostObserver.disconnect();
    this.placeObserver.disconnect();
    try {
      this.host.hidePopover();
    } catch {
      // Not open.
    }
    this.host.remove();
  }

  private openPopover(): void {
    try {
      this.host.showPopover();
    } catch {
      // A page without the top layer (or the tests' DOM): the fixed position still shows it.
    }
  }

  private style(left: number, top: number, width: number, height: number): void {
    const win = this.options.doc.defaultView;
    const dark =
      typeof win?.matchMedia === "function" &&
      win.matchMedia("(prefers-color-scheme: dark)").matches;
    const look = LOOK[dark ? "dark" : "light"];
    // No height yet: a shadow around nothing would show as a line.
    const shadow = height > 0 ? look.shadow : "none";
    const css = [
      "all: initial",
      "position: fixed",
      `left: ${left}px`,
      `top: ${top}px`,
      "right: auto",
      "bottom: auto",
      `width: ${width}px`,
      `height: ${height}px`,
      "margin: 0",
      "padding: 0",
      "border: 0",
      `border-radius: ${look[this.kind]}px`,
      `box-shadow: ${shadow}`,
      "display: block",
      "overflow: visible",
      "background: transparent",
      "z-index: 2147483647",
      "opacity: 1",
      "visibility: visible",
      "filter: none",
      "clip-path: none",
      "mask: none",
      "mix-blend-mode: normal",
      "transform: none",
      "pointer-events: auto",
      "color-scheme: normal",
    ]
      .map((declaration) => `${declaration} !important`)
      .join("; ");
    this.host.setAttribute("style", css);
    // Our own change, not the page's.
    this.hostObserver.takeRecords();
  }

  private tamper(): void {
    if (this.tampered) return;
    this.tampered = true;
    this.options.onTampered();
  }
}
