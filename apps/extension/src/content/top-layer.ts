// Where Lockra's frames and its mark go in a page that opened a modal dialog. A dialog opened with
// `showModal()` makes the rest of the page inert: what is outside it still shows, in the top layer
// on top of the dialog too, but takes no click, which goes to whatever of the dialog lies under it
// (the user's report, 2026-11-20: choosing a login in the menu pressed the dialog's button under
// it, and nothing was filled; measured in Chrome 155). Inside the dialog they take clicks again,
// and in the top layer they still show above it, where they are placed.

/** Whether `el` is a dialog the page opened modal. */
export function isModal(el: Element): el is HTMLDialogElement {
  if (!(el instanceof HTMLDialogElement) || !el.open || !el.isConnected) return false;
  try {
    return el.matches(":modal");
  } catch {
    // A browser without `:modal` has no inert page around a dialog either.
    return false;
  }
}

/** The node above `node` in the flat tree: the slot it shows in, its parent, a shadow root's host. */
function flatParent(node: Node): Node | null {
  if (node instanceof Element && node.assignedSlot) return node.assignedSlot;
  const parent = node.parentNode;
  return parent instanceof ShadowRoot ? parent.host : parent;
}

/** The modal dialog `el` is in, the nearest one; null outside every one. */
export function modalOf(el: Element): HTMLDialogElement | null {
  for (let node = flatParent(el); node; node = flatParent(node))
    if (node instanceof Element && isModal(node)) return node;
  return null;
}

/** The page's modal dialog the rest of the page is inert under: the one that holds the focus
 *  (nothing else can), followed into open shadow roots (a component's own dialog), else the last
 *  one open in the document. */
export function topModal(doc: Document): HTMLDialogElement | null {
  let focused = doc.activeElement;
  while (focused?.shadowRoot?.activeElement) focused = focused.shadowRoot.activeElement;
  const holding = focused ? (isModal(focused) ? focused : modalOf(focused)) : null;
  return holding ?? Array.from(doc.querySelectorAll("dialog")).findLast((d) => isModal(d)) ?? null;
}
