// The Lockra mark at the right end of a field that has a menu (board 33): a click opens the menu
// again after it was closed. It sits in the page like the field (not in the top layer: the page's
// own dialogs cover it), in a closed shadow root, and opens nothing by itself: the menu it asks for
// has its own checks. A field in the page's modal dialog has it in the dialog: outside, the dialog
// would cover it, and the page around the dialog is inert. Not in the top layer there either: a
// top-layer element shown after Lockra's menu makes Chrome's IntersectionObserver v2 report the
// menu unseen, overlapping it or not, and every click on the menu was refused (measured in Chrome
// 155, 2026-11-20).
import { modalOf, topModal } from "./top-layer";

const SIZE = 20;
const INSET = 8;

export class FieldIcon {
  readonly host: HTMLDivElement;
  private anchor: HTMLElement | null = null;
  /** The modal dialog it is in, null at the end of <html>. */
  private dialog: HTMLDialogElement | null = null;

  constructor(
    private readonly doc: Document,
    iconUrl: string,
    onClick: (anchor: HTMLElement) => void,
    /** Only what the user does counts (`isTrusted`); the tests' clicks are not. */
    trusted: (event: Event) => boolean = (event) => event.isTrusted,
  ) {
    this.host = doc.createElement("div");
    const root = this.host.attachShadow({ mode: "closed" });
    const image = doc.createElement("img");
    image.src = iconUrl;
    image.alt = "";
    image.setAttribute(
      "style",
      `all: initial !important; display: block !important; width: ${SIZE}px !important; height: ${SIZE}px !important; cursor: pointer !important;`,
    );
    root.append(image);
    this.host.addEventListener("mousedown", (event) => {
      // Keep the field's focus: the menu opens under it.
      event.preventDefault();
    });
    this.host.addEventListener("click", (event) => {
      if (trusted(event) && this.anchor) onClick(this.anchor);
    });
  }

  show(anchor: HTMLElement): void {
    this.anchor = anchor;
    const dialog = modalOf(anchor);
    if (!this.host.isConnected || dialog !== this.dialog) {
      this.dialog = dialog;
      (dialog ?? this.doc.documentElement).append(this.host);
    }
    this.layout();
  }

  /** The page opened or closed a dialog: gone where the field went with its dialog or lies inert
   *  under another. */
  raise(): void {
    const anchor = this.anchor;
    if (!anchor || !this.host.isConnected) return;
    const dialog = modalOf(anchor);
    const top = topModal(this.doc);
    if (dialog !== this.dialog || (top !== null && top !== dialog)) this.hide();
  }

  layout(): void {
    const anchor = this.anchor;
    if (!anchor || !this.host.isConnected) return;
    const rect = anchor.getBoundingClientRect();
    const left = Math.round(rect.right - SIZE - INSET);
    const top = Math.round(rect.top + (rect.height - SIZE) / 2);
    this.place(left, top);
    if (!this.dialog) return;
    // A dialog that moves itself with a transform (a centring translate) is the mark's containing
    // block, and the mark lands off by the dialog's own place: moved back to the field's.
    const at = this.host.getBoundingClientRect();
    const dx = left - Math.round(at.left);
    const dy = top - Math.round(at.top);
    if (dx !== 0 || dy !== 0) this.place(left + dx, top + dy);
  }

  private place(left: number, top: number): void {
    this.host.setAttribute(
      "style",
      `all: initial !important; position: fixed !important; left: ${left}px !important; top: ${top}px !important; width: ${SIZE}px !important; height: ${SIZE}px !important; z-index: 2147483646 !important; display: block !important;`,
    );
  }

  /** The field it is shown at, if it is. */
  get field(): HTMLElement | null {
    return this.anchor;
  }

  hide(): void {
    this.anchor = null;
    this.host.remove();
  }
}
