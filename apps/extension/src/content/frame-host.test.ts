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

/** A field 36 px high whose top is `top` px down the window. */
function field(top: number): HTMLInputElement {
  const el = document.createElement("input");
  document.body.append(el);
  el.getBoundingClientRect = () => {
    const rect = { x: 100, y: top, left: 100, top, width: 360, height: 36 };
    return { ...rect, right: 460, bottom: top + 36, toJSON: () => rect };
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

  it("opens a menu above its field where the browser lists its own passwords under it, under the field when there is no room above", () => {
    // 2026-10-16, the user: Chrome's list of the passwords it keeps opens under the field (above
    // only when there is no room under it), right where Lockra's menu opened.
    const menu = host("menu");
    menu.show({ anchor: field(300), side: "above" });
    expect(declared(menu, "top")).toBe("294px");
    menu.resize(200);
    expect(declared(menu, "top")).toBe("94px");
    menu.remove();
    const high = host("menu");
    high.show({ anchor: field(120), side: "above" });
    high.resize(200);
    expect(declared(high, "top")).toBe("162px");
    high.remove();
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
