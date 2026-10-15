// @vitest-environment node
// The inline frame's lift comes from tokens.css, the one place colours are written.
import { TOKENS, frameLook, readFrameLook } from "./frame-look.mjs";

const theme = (id: string, body: string) =>
  `:root[data-theme="${id}"],\n[data-theme="${id}"] {\n${body}\n}\n`;

describe("frameLook", () => {
  it("reads the shadow and the corners of 澄空 and 澄空 · 夜", () => {
    const css =
      theme(
        "glass",
        "  --shadow-pop: 0 1px 2px 0 rgb(1 2 3 / 0.5);\n  --radius-card: 18px;\n  --radius-dialog: 22px;",
      ) +
      theme(
        "glass_night",
        "  --shadow-pop: 0 3px 4px 0 rgb(0 0 0 / 0.5);\n  --radius-card: 16px;\n  --radius-dialog: 20px;",
      );
    expect(frameLook(css)).toEqual({
      light: { shadow: "0 1px 2px 0 rgb(1 2 3 / 0.5)", menu: 18, prompt: 22 },
      dark: { shadow: "0 3px 4px 0 rgb(0 0 0 / 0.5)", menu: 16, prompt: 20 },
    });
  });

  it("fails the build instead of drawing without a token", () => {
    expect(() => frameLook("")).toThrow("no glass theme");
    expect(() => frameLook(theme("glass", "  --radius-card: 18px;"))).toThrow(
      "glass has no --shadow-pop",
    );
    expect(() =>
      frameLook(
        theme("glass", "  --shadow-pop: none;\n  --radius-card: 1rem;\n  --radius-dialog: 2px;"),
      ),
    ).toThrow("not a length in px");
  });

  it("is what the builds put in, from the real tokens.css", () => {
    expect(TOKENS.endsWith("packages/ui/src/tokens.css")).toBe(true);
    expect(readFrameLook()).toEqual(LOCKRA_FRAME_LOOK);
  });
});
