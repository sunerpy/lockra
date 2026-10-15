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

describe("FrameHost", () => {
  afterEach(() => {
    document.documentElement.replaceChildren(document.createElement("head"), document.body);
    Reflect.deleteProperty(window, "matchMedia");
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
