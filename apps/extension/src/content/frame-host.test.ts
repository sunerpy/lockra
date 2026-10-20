import { FrameHost } from "./frame-host";

function host(kind: "menu" | "prompt", dark = false): FrameHost {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({ matches: dark && query === "(prefers-color-scheme: dark)" }),
  });
  return new FrameHost({
    doc: document,
    url: "chrome-extension://lockra/inline.html",
    token: "t",
    kind,
    now: () => 0,
    onTampered: () => undefined,
  });
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
