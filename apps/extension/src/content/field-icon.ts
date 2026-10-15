// The Lockra mark at the right end of a field that has a menu (board 33): a click opens the menu
// again after it was closed. It sits in the page like the field (not in the top layer: the page's
// own dialogs cover it), in a closed shadow root, and opens nothing by itself: the menu it asks for
// has its own checks.
const SIZE = 20;
const INSET = 8;

export class FieldIcon {
  readonly host: HTMLDivElement;
  private anchor: HTMLElement | null = null;

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
    if (!this.host.isConnected) this.doc.documentElement.append(this.host);
    this.layout();
  }

  layout(): void {
    const anchor = this.anchor;
    if (!anchor || !this.host.isConnected) return;
    const rect = anchor.getBoundingClientRect();
    const left = Math.round(rect.right - SIZE - INSET);
    const top = Math.round(rect.top + (rect.height - SIZE) / 2);
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
