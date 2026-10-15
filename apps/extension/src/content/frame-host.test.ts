import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { FRAME_LOOK, FrameHost } from "./frame-host";

const tokens = readFileSync(
  resolve(import.meta.dirname, "../../../../packages/ui/src/tokens.css"),
  "utf8",
);

/** A theme's own block of tokens.css. */
function themeBlock(id: string): string {
  const start = tokens.indexOf(`[data-theme="${id}"] {`);
  return tokens.slice(start, tokens.indexOf("}", start));
}

function token(id: string, name: string): string | undefined {
  return new RegExp(`--${name}: ([^;]+);`).exec(themeBlock(id))?.[1];
}

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

  it("draws the cards' shadow and corners of 澄空 and 澄空 · 夜, as tokens.css has them", () => {
    expect(FRAME_LOOK.shadow.light).toBe(token("glass", "shadow-pop"));
    expect(FRAME_LOOK.shadow.dark).toBe(token("glass_night", "shadow-pop"));
    for (const id of ["glass", "glass_night"]) {
      expect(`${FRAME_LOOK.radius.menu}px`).toBe(token(id, "radius-card"));
      expect(`${FRAME_LOOK.radius.prompt}px`).toBe(token(id, "radius-dialog"));
    }
  });

  it("lifts the frame from the page: the shadow the frame would cut off, around the host", () => {
    const menu = host("menu");
    menu.show("corner");
    // No height yet: a shadow around nothing would show as a line.
    expect(declared(menu, "box-shadow")).toBe("none");
    menu.resize(120);
    expect(declared(menu, "box-shadow")).toBe(FRAME_LOOK.shadow.light);
    expect(declared(menu, "border-radius")).toBe(`${FRAME_LOOK.radius.menu}px`);
    menu.remove();

    const prompt = host("prompt", true);
    prompt.show("corner");
    prompt.resize(160);
    expect(declared(prompt, "box-shadow")).toBe(FRAME_LOOK.shadow.dark);
    expect(declared(prompt, "border-radius")).toBe(`${FRAME_LOOK.radius.prompt}px`);
    prompt.remove();
  });
});
