// The inline frame's lift, read from tokens.css: the cards' `shadow-pop` and corners
// (`--radius-card` for the menu, `--radius-dialog` for the prompts) in 澄空 (light) and 澄空 · 夜
// (dark). The frame's host draws them in the page (src/content/frame-host.ts), where tokens.css is
// not loaded, so the builds put them in as LOCKRA_FRAME_LOOK (vite.config.ts, vitest.config.ts).
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const THEMES = { light: "glass", dark: "glass_night" };

export const TOKENS = join(
  dirname(fileURLToPath(import.meta.url)),
  "../../../packages/ui/src/tokens.css",
);

/** `--name` in the theme's own block of `css`. */
function token(css, theme, name) {
  const start = css.indexOf(`[data-theme="${theme}"] {`);
  if (start < 0) throw new Error(`frame-look: tokens.css has no ${theme} theme`);
  const block = css.slice(start, css.indexOf("}", start));
  const value = new RegExp(`--${name}:([^;]+);`).exec(block)?.[1]?.trim();
  if (!value) throw new Error(`frame-look: ${theme} has no --${name}`);
  return value;
}

function pixels(css, theme, name) {
  const value = token(css, theme, name);
  const match = /^(\d+)px$/.exec(value);
  if (!match) throw new Error(`frame-look: ${theme}'s --${name} is ${value}, not a length in px`);
  return Number(match[1]);
}

export function frameLook(css) {
  const look = (theme) => ({
    shadow: token(css, theme, "shadow-pop"),
    menu: pixels(css, theme, "radius-card"),
    prompt: pixels(css, theme, "radius-dialog"),
  });
  return { light: look(THEMES.light), dark: look(THEMES.dark) };
}

export function readFrameLook(path = TOKENS) {
  return frameLook(readFileSync(path, "utf8"));
}
