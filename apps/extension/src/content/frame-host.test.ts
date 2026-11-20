import { FrameHost } from "./frame-host";

let now = 0;

function host(kind: "menu" | "prompt", dark = false, onTampered = () => undefined): FrameHost {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({ matches: dark && query === "(prefers-color-scheme: dark)" }),
  });
  return new FrameHost({
    doc: document,
    url: "chrome-extension://lockra/inline.html",
    token: "t",
    kind,
    now: () => now,
    onTampered,
    // jsdom draws nothing: nothing lies at any point.
    measurer: { elementsFromPoint: () => [], style: () => CLEAN },
  });
}

const CLEAN = {
  opacity: "1",
  filter: "none",
  clipPath: "none",
  mixBlendMode: "normal",
  visibility: "visible",
  display: "block",
};

/** A dialog the page opened modal, holding a field (jsdom has no `showModal` and no `:modal`). */
function modalDialog(): { dialog: HTMLDialogElement; inside: HTMLInputElement } {
  const dialog = document.createElement("dialog");
  dialog.setAttribute("open", "");
  const matches = dialog.matches.bind(dialog);
  Object.defineProperty(dialog, "matches", {
    value: (selectors: string) =>
      selectors === ":modal"
        ? dialog.isConnected && dialog.open && dialog.dataset.modal !== "no"
        : matches(selectors),
  });
  const inside = document.createElement("input");
  dialog.append(inside);
  document.body.append(dialog);
  return { dialog, inside };
}

/** The host's own declaration of `property`. */
function declared(frame: FrameHost, property: string): string | undefined {
  return new RegExp(`(?:^|; )${property}: ([^;]+) !important`).exec(
    frame.host.getAttribute("style") ?? "",
  )?.[1];
}

/** A field 36 px high whose top is `top` px down the window (and `left` px from its left edge). */
function field(top: number, left = 100, width = 360): HTMLInputElement {
  const el = document.createElement("input");
  document.body.append(el);
  el.getBoundingClientRect = () => {
    const rect = { x: left, y: top, left, top, width, height: 36 };
    return { ...rect, right: left + width, bottom: top + 36, toJSON: () => rect };
  };
  return el;
}

describe("FrameHost", () => {
  afterEach(() => {
    document.body.replaceChildren();
    document.documentElement.replaceChildren(document.createElement("head"), document.body);
    Reflect.deleteProperty(window, "matchMedia");
  });

  it("opens a menu under its field, or above it when there is no room under it", () => {
    // jsdom's window: 768 px high.
    const menu = host("menu");
    menu.show({ anchor: field(300) });
    menu.resize(200);
    expect(declared(menu, "top")).toBe("342px");
    menu.remove();
    const low = host("menu");
    low.show({ anchor: field(600) });
    low.resize(200);
    expect(declared(low, "top")).toBe("394px");
    low.remove();
  });

  it("opens a menu above its field where the browser lists its own passwords under it, beside the field when there is no room above", () => {
    // 2026-10-16, the user: Chrome's list of the passwords it keeps opens under the field (above
    // only when there is no room under it), right where Lockra's menu opened.
    const menu = host("menu");
    menu.show({ anchor: field(300), side: "above" });
    expect(declared(menu, "top")).toBe("294px");
    menu.resize(200);
    expect(declared(menu, "top")).toBe("94px");
    menu.remove();
    // 2026-10-20, a code field near the top of NDKEY's ADFS page: under it, Lockra's menu and
    // Chrome's list lay one on the other. Beside the field instead, its left side first.
    const left = host("menu");
    left.show({ anchor: field(120, 600, 170), side: "above" });
    left.resize(300);
    expect([declared(left, "left"), declared(left, "top")]).toEqual(["254px", "120px"]);
    left.remove();
    // Its right side clears the browser's list too, which is as wide as the field or wider.
    const right = host("menu");
    right.show({ anchor: field(120), side: "above" });
    right.resize(200);
    expect([declared(right, "left"), declared(right, "top")]).toEqual(["466px", "120px"]);
    right.remove();
    // A tall menu stays in the window; with no room on either side, under the field.
    const tall = host("menu");
    tall.show({ anchor: field(500, 600, 170), side: "above" });
    tall.resize(600);
    expect([declared(tall, "left"), declared(tall, "top")]).toEqual(["254px", "160px"]);
    tall.remove();
    const wide = host("menu");
    wide.show({ anchor: field(120, 100, 900), side: "above" });
    wide.resize(200);
    expect([declared(wide, "left"), declared(wide, "top")]).toEqual(["100px", "162px"]);
    wide.remove();
  });

  it("lives in the page's modal dialog that holds its field: outside it, the page is inert", () => {
    // 2026-11-20: outside the dialog the menu showed on top, and a click on it pressed the
    // dialog's button under it.
    // A field outside every modal dialog: at the end of the page, as before.
    const plain = host("menu");
    plain.show({ anchor: field(300) });
    expect(plain.host.parentElement).toBe(document.documentElement);
    plain.remove();
    const { dialog, inside } = modalDialog();
    const menu = host("menu");
    menu.show({ anchor: inside });
    expect(menu.host.parentElement).toBe(dialog);
    menu.remove();
    // A prompt has no field: the dialog the page is inert under.
    const prompt = host("prompt");
    prompt.show("corner");
    expect(prompt.host.parentElement).toBe(dialog);
    prompt.remove();
  });

  it("goes with the page's dialogs: a prompt moves in and out, a menu whose field they leave inert cannot stay", () => {
    now = 1_000;
    const prompt = host("prompt");
    prompt.show("corner");
    expect(prompt.host.parentElement).toBe(document.documentElement);
    const { dialog, inside } = modalDialog();
    now = 2_000;
    expect(prompt.raise()).toBe(true);
    expect(prompt.host.parentElement).toBe(dialog);
    // Moved, the 300 ms start again.
    expect(prompt.verify(null, true, null)).toEqual({ ok: false, reason: "too_soon" });
    dialog.removeAttribute("open");
    expect(prompt.raise()).toBe(true);
    expect(prompt.host.parentElement).toBe(document.documentElement);
    prompt.remove();
    // The menu of a field in the dialog, the dialog closed: the field went with it.
    dialog.setAttribute("open", "");
    const menu = host("menu");
    menu.show({ anchor: inside });
    expect(menu.raise()).toBe(true);
    dialog.dataset.modal = "no";
    expect(menu.raise()).toBe(false);
    menu.remove();
    // The menu of a field outside it, a modal dialog opened over the page: the field is inert.
    dialog.dataset.modal = "";
    const outside = host("menu");
    dialog.dataset.modal = "no";
    outside.show({ anchor: field(300) });
    dialog.dataset.modal = "";
    expect(outside.raise()).toBe(false);
    outside.remove();
  });

  it("starts its 300 ms again at a click too soon: a quick run of clicks gets none counted", () => {
    // A page that opens the menu under a run of clicks: every click comes too soon, and only one
    // after the user stopped for 300 ms counts.
    now = 0;
    const menu = host("menu");
    menu.show({ anchor: field(300) });
    for (const at of [150, 300, 450]) {
      now = at;
      expect(menu.verify(null, true, null)).toEqual({ ok: false, reason: "too_soon" });
    }
    now = 749;
    expect(menu.verify(null, true, null)).toEqual({ ok: false, reason: "too_soon" });
    now = 1_049;
    expect(menu.verify(null, true, null)).toMatchObject({ ok: false, reason: "covered" });
    menu.remove();
  });

  it("counts the page taking it out of the dialog as tampering, not its own move", () => {
    const tampered = vi.fn();
    const prompt = host("prompt", false, tampered);
    prompt.show("corner");
    const { dialog } = modalDialog();
    prompt.raise();
    return Promise.resolve()
      .then(() => {
        expect(tampered).not.toHaveBeenCalled();
        dialog.removeChild(prompt.host);
        return Promise.resolve();
      })
      .then(() => expect(tampered).toHaveBeenCalledTimes(1));
  });

  it("stays a moment where it was after a click it acted on, see-through, then goes", () => {
    vi.useFakeTimers();
    try {
      const menu = host("menu");
      menu.show({ anchor: field(300) });
      menu.resize(200);
      menu.remove(500);
      // Still in its place, catching a second click (a double click) the page would get.
      expect(menu.host.isConnected).toBe(true);
      expect(declared(menu, "opacity")).toBe("0");
      expect(declared(menu, "top")).toBe("342px");
      expect(menu.verify(null, true, null)).toEqual({ ok: false, reason: "tampered" });
      vi.advanceTimersByTime(499);
      expect(menu.host.isConnected).toBe(true);
      vi.advanceTimersByTime(1);
      expect(menu.host.isConnected).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("lifts the frame from the page: the shadow the frame would cut off, around the host", () => {
    const menu = host("menu");
    menu.show("corner");
    // No height yet: a shadow around nothing would show as a line.
    expect(declared(menu, "box-shadow")).toBe("none");
    menu.resize(120);
    expect(declared(menu, "box-shadow")).toBe(LOCKRA_FRAME_LOOK.light.shadow);
    expect(declared(menu, "border-radius")).toBe(`${LOCKRA_FRAME_LOOK.light.menu}px`);
    menu.remove();

    const prompt = host("prompt", true);
    prompt.show("corner");
    prompt.resize(160);
    expect(declared(prompt, "box-shadow")).toBe(LOCKRA_FRAME_LOOK.dark.shadow);
    expect(declared(prompt, "border-radius")).toBe(`${LOCKRA_FRAME_LOOK.dark.prompt}px`);
    prompt.remove();
  });
});
