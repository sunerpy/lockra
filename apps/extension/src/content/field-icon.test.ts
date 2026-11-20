// The field's mark in a page that opened a modal dialog: in the dialog that holds the field (out of
// it, the dialog covers it and the page is inert), not in the top layer, at the field's end even
// where the dialog moves itself with a transform.
import { FieldIcon } from "./field-icon";

/** A dialog the page opened modal (jsdom has no `showModal` and no `:modal`). */
function modalDialog(): HTMLDialogElement {
  const dialog = document.createElement("dialog");
  dialog.setAttribute("open", "");
  const matches = dialog.matches.bind(dialog);
  Object.defineProperty(dialog, "matches", {
    value: (selectors: string) =>
      selectors === ":modal" ? dialog.open && dialog.dataset.modal !== "no" : matches(selectors),
  });
  document.body.append(dialog);
  return dialog;
}

function rect(left: number, top: number, width: number, height: number): DOMRect {
  const r = {
    x: left,
    y: top,
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
  };
  return { ...r, toJSON: () => r };
}

/** A field 36 px high at (100, 300), 200 px wide. */
function field(parent: HTMLElement): HTMLInputElement {
  const el = document.createElement("input");
  el.getBoundingClientRect = () => rect(100, 300, 200, 36);
  parent.append(el);
  return el;
}

function declared(el: HTMLElement, property: string): string | undefined {
  return new RegExp(`(?:^|; )${property}: ([^;]+) !important`).exec(
    el.getAttribute("style") ?? "",
  )?.[1];
}

afterEach(() => {
  document.body.replaceChildren();
  document.documentElement.replaceChildren(document.createElement("head"), document.body);
});

describe("FieldIcon", () => {
  it("sits at the end of a field, at the end of the page outside every modal dialog", () => {
    const icon = new FieldIcon(document, "icon.png", () => undefined);
    const anchor = field(document.body);
    icon.show(anchor);
    expect(icon.host.parentElement).toBe(document.documentElement);
    expect([declared(icon.host, "left"), declared(icon.host, "top")]).toEqual(["272px", "308px"]);
    icon.hide();
    expect(icon.host.isConnected).toBe(false);
  });

  it("goes into the modal dialog that holds its field, not in the top layer", () => {
    const dialog = modalDialog();
    const icon = new FieldIcon(document, "icon.png", () => undefined);
    const anchor = field(dialog);
    icon.show(anchor);
    expect(icon.host.parentElement).toBe(dialog);
    expect(icon.host.hasAttribute("popover")).toBe(false);
    // The dialog closes, or another opens over it: the field went, or lies inert.
    icon.raise();
    expect(icon.host.isConnected).toBe(true);
    dialog.dataset.modal = "no";
    icon.raise();
    expect(icon.host.isConnected).toBe(false);
    dialog.dataset.modal = "";
    icon.show(anchor);
    const other = modalDialog();
    other.dataset.modal = "";
    icon.raise();
    expect(icon.host.isConnected).toBe(false);
  });

  it("is put back at the field where a dialog that moves itself would move it too", () => {
    // A dialog centred with a transform is the containing block of what is fixed in it.
    const dialog = modalDialog();
    const icon = new FieldIcon(document, "icon.png", () => undefined);
    icon.host.getBoundingClientRect = () =>
      rect(
        Number.parseInt(declared(icon.host, "left") ?? "0", 10) + 40,
        Number.parseInt(declared(icon.host, "top") ?? "0", 10) + 25,
        20,
        20,
      );
    icon.show(field(dialog));
    expect([declared(icon.host, "left"), declared(icon.host, "top")]).toEqual(["232px", "283px"]);
  });
});
