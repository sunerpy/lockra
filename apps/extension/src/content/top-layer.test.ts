// The modal dialog a frame must live in: the one that holds a field, the flat tree's too, and the
// one the page is inert under, found by the focus, inside a component's shadow root too.
import { isModal, modalOf, topModal } from "./top-layer";

/** A dialog opened modal (jsdom has no `showModal` and no `:modal`). */
function modalDialog(parent: ParentNode = document.body): HTMLDialogElement {
  const dialog = document.createElement("dialog");
  dialog.setAttribute("open", "");
  const matches = dialog.matches.bind(dialog);
  Object.defineProperty(dialog, "matches", {
    value: (selectors: string) =>
      selectors === ":modal" ? dialog.isConnected && dialog.open : matches(selectors),
  });
  parent.append(dialog);
  return dialog;
}

afterEach(() => {
  document.body.replaceChildren();
});

describe("the page's modal dialogs", () => {
  it("are open, modal and in the page", () => {
    const dialog = modalDialog();
    expect(isModal(dialog)).toBe(true);
    dialog.removeAttribute("open");
    expect(isModal(dialog)).toBe(false);
    dialog.setAttribute("open", "");
    dialog.remove();
    expect(isModal(dialog)).toBe(false);
    expect(isModal(document.body)).toBe(false);
  });

  it("hold a field through shadow roots and slots", () => {
    const dialog = modalDialog();
    const field = document.createElement("input");
    dialog.append(field);
    expect(modalOf(field)).toBe(dialog);
    // A component in the dialog renders the field in its shadow root.
    const part = document.createElement("div");
    dialog.append(part);
    const inner = document.createElement("input");
    part.attachShadow({ mode: "open" }).append(inner);
    expect(modalOf(inner)).toBe(dialog);
    const outside = document.createElement("input");
    document.body.append(outside);
    expect(modalOf(outside)).toBeNull();
  });

  it("are found by the focus, in a component's shadow root too, else the last one open", () => {
    expect(topModal(document)).toBeNull();
    const first = modalDialog();
    const second = modalDialog();
    expect(topModal(document)).toBe(second);
    const field = document.createElement("input");
    first.append(field);
    field.focus();
    expect(topModal(document)).toBe(first);
    // A component whose dialog is in its own shadow root, the field in it focused.
    const component = document.createElement("div");
    document.body.append(component);
    const root = component.attachShadow({ mode: "open" });
    const shadowDialog = modalDialog(root);
    const inner = document.createElement("input");
    shadowDialog.append(inner);
    inner.focus();
    expect(topModal(document)).toBe(shadowDialog);
  });
});
