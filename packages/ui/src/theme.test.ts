import { readFileSync, readdirSync } from "node:fs";
import { ACCOUNT_COLORS } from "@lockra/shared";
import { resolve } from "node:path";
import {
  ACCENT_IDS,
  FONT_SIZE_MAX,
  THEME_IDS,
  type ThemeId,
  applyAppearance,
  applyTheme,
  followsSystem,
  isAccentId,
  isDarkTheme,
  isThemeId,
  pickTheme,
  readTheme,
  resolveTheme,
  THEME_COLUMNS,
  systemPrefersDark,
  systemPrefersReducedMotion,
  themePair,
} from "./theme";

const tokens = readFileSync(resolve(__dirname, "tokens.css"), "utf8");

const ROLES = [
  "canvas",
  "surface",
  "inset",
  "inset2",
  "border",
  "borderStrong",
  "fg",
  "fgMuted",
  "fgSubtle",
  "primary",
  "primaryFg",
  "accent",
  "accentFg",
  "accentSoft",
  "accentText",
  "accentTextHover",
  "thumb",
  "ok",
  "okSoft",
  "okText",
  "danger",
  "dangerSoft",
  "warning",
  "warningSoft",
  "info",
  "infoSoft",
  "nav",
  "navActive",
  "track",
  "keycapBg",
  "keycapBorder",
  "scrim",
  "qrPlate",
  "mark",
];

function kebab(role: string): string {
  return role.replaceAll(/[A-Z]/g, (m) => `-${m.toLowerCase()}`);
}

function themeBlock(id: string): string {
  const start = tokens.indexOf(`[data-theme="${id}"] {`);
  const end = tokens.indexOf("}", start);
  return tokens.slice(start, end);
}

const STRUCTURE = [
  "surface-fill",
  "nav-fill",
  "overlay-fill",
  "edge",
  "canvas-image",
  "card-shadow",
  "surface-blur",
  "radius-card",
  "radius-dialog",
  "radius-control",
  "sidebar-inset",
  "sidebar-radius",
  "sidebar-edge",
  "sidebar-border",
  "sidebar-shadow",
  "primary-shadow",
  "meter",
  "tile-copied-bg",
  "tile-copied-fg",
  "tile-copied-muted",
  "tile-copied-edge",
  "tile-copied-accent",
  "tile-pill-bg",
  "tile-pill-fg",
  "tile-pill-pad",
  "tile-hover-edge",
  "row-pill-bg",
  "row-pill-fg",
  "logo-body",
  "logo-dots",
  "titlebar-fill",
  "titlebar-rule",
];

describe("tokens.css", () => {
  it.each([...THEME_IDS])("theme %s carries every role as a six-digit hex", (id) => {
    const block = themeBlock(id);
    for (const role of ROLES) {
      const match = new RegExp(`--${kebab(role)}: (#[0-9A-Fa-f]{6});`).exec(block);
      if (!match) throw new Error(`palette missing ${id}.${role}`);
    }
    for (const shadow of ["shadow-pop", "shadow-win"]) expect(block).toContain(`--${shadow}:`);
  });

  it("the themes differ from each other on the canvas role", () => {
    const canvases = THEME_IDS.map((id) =>
      /--canvas: (#[0-9A-Fa-f]{6});/.exec(themeBlock(id))?.[1]?.toLowerCase(),
    );
    expect(new Set(canvases).size).toBe(THEME_IDS.length);
  });

  it("every theme sets its whole structure; the classic four keep the look they had", () => {
    const classic = tokens.slice(
      tokens.indexOf(':root,\n[data-theme="light"],\n[data-theme="dark"],'),
      tokens.indexOf("}", tokens.indexOf(':root,\n[data-theme="light"],\n[data-theme="dark"],')),
    );
    for (const name of STRUCTURE) expect(classic, `classic ${name}`).toContain(`--${name}:`);
    expect(classic).toContain("--surface-fill: var(--surface);");
    expect(classic).toContain("--canvas-image: none;");
    expect(classic).toContain("--radius-card: 10px;");
    expect(classic).toContain("--radius-control: 6px;");
    expect(classic).toContain("--sidebar-inset: 0px;");
    // The classic rail keeps its rule on the right and no frame around it.
    expect(classic).toContain("--sidebar-border: 0 1px 0 0;");
    expect(classic).toContain("--titlebar-fill: var(--surface);");
    for (const id of ["glass", "glass_night", "paper", "paper_night"]) {
      const block = themeBlock(id);
      for (const name of STRUCTURE) expect(block, `${id} ${name}`).toContain(`--${name}:`);
    }
    // 澄空 is translucent over an aurora; 墨白 meters in ink and inverts a copied card.
    expect(themeBlock("glass")).toMatch(/--surface-fill: rgb\(255 255 255 \/ 0\.\d+\);/);
    expect(themeBlock("glass")).toMatch(/--canvas-image:\s+radial-gradient\(/);
    expect(themeBlock("paper")).toContain("--meter: var(--fg);");
    expect(themeBlock("paper")).toContain("--tile-copied-bg: var(--fg);");
    // A hovered card's edge turns to ink in 墨白, as on board B1.
    expect(themeBlock("paper")).toContain("--tile-hover-edge: var(--fg);");
    // A copied row's pill: the accent's in the classic themes and 澄空, ink in 墨白 (boards A1, B6).
    expect(classic).toContain("--row-pill-bg: var(--accent-soft);");
    expect(themeBlock("paper")).toContain("--row-pill-bg: var(--fg);");
    expect(themeBlock("paper_night")).toContain("--row-pill-fg: var(--surface);");
    // The logo's padlock stays the brand's blue whatever the accent (the theme's own blue), its
    // dots white.
    expect(classic).toContain("--logo-body: #339cff;");
    expect(themeBlock("glass")).toContain("--logo-body: #3d7bff;");
    expect(themeBlock("paper")).toContain("--logo-body: #2142ff;");
    for (const id of ["glass", "glass_night", "paper", "paper_night"])
      expect(themeBlock(id), `logo dots in ${id}`).toContain("--logo-dots: #ffffff;");
    expect(classic).toContain("--tile-hover-edge: var(--edge);");
    // A copied card keeps the classic card and says it in an accent pill.
    expect(classic).toContain("--tile-copied-bg: var(--surface-fill);");
    expect(classic).toContain("--tile-pill-bg: var(--accent-soft);");
  });

  it("a copied card reads in 墨白: the account line on the ink, which the utility maps for the card", () => {
    for (const id of ["paper", "paper_night"]) {
      const block = themeBlock(id);
      const fg = roleIn(block, "fg") ?? "";
      const surface = roleIn(block, "surface") ?? "";
      const muted = /--tile-copied-muted: (#[0-9a-f]{6});/.exec(block)?.[1] ?? "";
      expect(contrast(surface, fg), `${id} copied text`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(muted, fg), `${id} copied account`).toBeGreaterThanOrEqual(4.5);
    }
    const utility = tokens.slice(tokens.indexOf("@utility tile-copied {"));
    for (const line of [
      "--surface-fill: var(--tile-copied-bg);",
      "--edge: var(--tile-copied-edge);",
      "--fg: var(--tile-copied-fg);",
      "--fg-muted: var(--tile-copied-muted);",
      "--accent-text: var(--tile-copied-accent);",
    ])
      expect(utility.slice(0, utility.indexOf("}"))).toContain(line);
  });

  it("exposes every role to Tailwind through @theme inline and never hardcodes hex in components", () => {
    // The surfaces and the nav reach Tailwind through their fills (the role itself where opaque).
    const FILLED = new Set(["surface", "nav"]);
    for (const role of ROLES)
      expect(tokens).toContain(
        `--color-${kebab(role)}: var(--${kebab(role)}${FILLED.has(role) ? "-fill" : ""});`,
      );
    expect(tokens).toContain("--color-overlay: var(--overlay-fill);");
    // regression (Windows test 2026-09-24): fonts came from Google Fonts, so an offline or
    // firewalled machine fell back to the system UI font and the app no longer matched its design.
    // The three families are bundled (fontsource variable builds) and Windows / macOS CJK fonts
    // are named as fallbacks.
    expect(tokens).toContain('@import "@fontsource-variable/instrument-sans/index.css";');
    expect(tokens).toContain('@import "@fontsource-variable/jetbrains-mono/index.css";');
    expect(tokens).toContain('@import "@fontsource-variable/noto-sans-sc/index.css";');
    expect(tokens).toMatch(
      /--font-ui:\s*"Instrument Sans Variable", "Instrument Sans", "Noto Sans SC Variable"/,
    );
    expect(tokens).toMatch(/--font-mono:\s*"JetBrains Mono Variable", "JetBrains Mono"/);
    expect(tokens).toContain('"Microsoft YaHei UI"');
    expect(tokens).toContain('"PingFang SC"');
    expect(tokens).not.toContain("googleapis");
    expect(tokens).toContain("--radius-10: var(--radius-card);");
    expect(tokens).toContain("--radius-pill: 999px");
  });
});

const THEMES = THEME_IDS;
const DARK_THEMES: ReadonlySet<string> = new Set([
  "dark",
  "graphite",
  "glass_night",
  "paper_night",
]);

function roleIn(block: string, role: string): string | undefined {
  return new RegExp(`--${kebab(role)}: (#[0-9A-Fa-f]{6});`).exec(block)?.[1]?.toLowerCase();
}

/** The roles of `theme` with `accent` chosen: the theme's block, then the accent's own block. */
function palette(theme: string, accent: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const role of ROLES) {
    const value = roleIn(themeBlock(theme), role);
    if (value !== undefined) out[role] = value;
  }
  if (accent !== "default") {
    const start = tokens.indexOf(`[data-accent="${accent}"][data-theme="${theme}"],`);
    if (start < 0) throw new Error(`no ${accent} block for ${theme}`);
    const block = tokens.slice(start, tokens.indexOf("}", start));
    for (const role of ["accent", "accentFg", "accentSoft", "accentText", "accentTextHover"]) {
      const value = roleIn(block, role);
      if (value === undefined) throw new Error(`${accent} × ${theme} lacks ${role}`);
      out[role] = value;
    }
  }
  return out;
}

function luminance(color: string): number {
  const channel = (i: number) => {
    const c = Number.parseInt(color.slice(1 + 2 * i, 3 + 2 * i), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2);
}

function contrast(a: string, b: string): number {
  const [la, lb] = [luminance(a), luminance(b)];
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

describe("accent colours (Codex's blue, ChatGPT's accent choices)", () => {
  it("the blue themes default to Codex's #339cff; the warm theme keeps its own accent", () => {
    // User decisions 2026-09-29: Codex's blue for the blue roles, the desktop app's #339cff.
    for (const theme of ["light", "dark", "graphite"])
      expect(palette(theme, "default").accent).toBe("#339cff");
    expect(palette("warm", "default").accent).toBe("#b85c38");
    // 澄空's own blue (lighter at night), 墨白's ink blue for links and focus.
    expect(palette("glass", "default").accent).toBe("#3d7bff");
    expect(palette("glass_night", "default").accent).toBe("#6e9bff");
    expect(palette("paper", "default").accent).toBe("#2142ff");
    expect(palette("paper_night", "default").accent).toBe("#8fa3ff");
    expect(tokens).toMatch(/:focus-visible \{\s*outline: 2px solid var\(--accent-text\);/);
  });

  it("every accent choice takes ChatGPT's light value in the light themes and its dark value in the dark ones", () => {
    const values: Record<string, readonly [string, string]> = {
      blue: ["#0285ff", "#339cff"],
      green: ["#04b84c", "#40c977"],
      yellow: ["#ffc300", "#ffd240"],
      pink: ["#ff66ad", "#ff8cc1"],
      orange: ["#fb6a22", "#ff8549"],
      purple: ["#924ff7", "#ad7bf9"],
    };
    expect([...ACCENT_IDS]).toEqual(["default", ...Object.keys(values), "ink"]);
    for (const theme of THEMES) {
      for (const [accent, [light, dark]] of Object.entries(values)) {
        expect(palette(theme, accent).accent, `${accent} × ${theme}`).toBe(
          DARK_THEMES.has(theme) ? dark : light,
        );
      }
      // 墨色 is the theme's own ink.
      expect(palette(theme, "ink").accent).toBe(palette(theme, "default").fg);
    }
  });

  it.each(THEMES)("text on and around every accent reads in the %s theme", (theme) => {
    for (const accent of ACCENT_IDS) {
      const p = palette(theme, accent);
      const at = `${accent} × ${theme}`;
      const need = (role: string) => {
        const value = p[role];
        if (value === undefined) throw new Error(`${at} lacks ${role}`);
        return value;
      };
      // Labels on an accent fill (the edit tag, the inserted check): 4.5:1.
      expect(contrast(need("accentFg"), need("accent")), `${at} accent-fg`).toBeGreaterThanOrEqual(
        4.5,
      );
      // Links and accent text, and their hover, on every surface they sit on: 4.5:1. The focus ring
      // is accent-text too, so it clears the 3:1 of a focus indicator.
      for (const background of ["surface", "canvas", "inset", "inset2", "accentSoft"]) {
        for (const role of ["accentText", "accentTextHover"]) {
          expect(
            contrast(need(role), need(background)),
            `${at} ${role} on ${background}`,
          ).toBeGreaterThanOrEqual(4.5);
        }
      }
    }
    // The information colour stays Codex's blue whatever the accent.
    const base = palette(theme, "default");
    expect(contrast(base.info ?? "", base.surface ?? ""), `${theme} info`).toBeGreaterThanOrEqual(
      4.5,
    );
    expect(
      contrast(base.info ?? "", base.infoSoft ?? ""),
      `${theme} info on info-soft`,
    ).toBeGreaterThanOrEqual(4.5);
  });

  it.each(THEMES)("a danger button's label reads in the %s theme", (theme) => {
    // The label is the opaque surface (`text-surface-solid`), never the translucent fill.
    const p = palette(theme, "default");
    expect(contrast(p.surface ?? "", p.danger ?? "")).toBeGreaterThanOrEqual(4.5);
  });

  it("regression (2026-10-11): an element's own border colour wins over its hairline", () => {
    // As a utility, `hairline` came after Tailwind's colour utilities, so its `border` shorthand hid
    // an input's `border-danger` and the chosen accent swatch's `border-fg` in every theme. In the
    // components layer, under every utility, it is only the default edge.
    expect(tokens).not.toContain("@utility hairline");
    const layer = tokens.slice(tokens.indexOf("@layer components {"));
    expect(layer).toMatch(
      /^@layer components \{\s*\.hairline \{\s*border: 1px solid var\(--edge\);\s*\}/,
    );
  });

  it("regression (澄空 · 夜, 2026-10-11): text never takes the surface's translucent fill", () => {
    // `bg-surface` is the fill (66 % white in 澄空, 7 % at night); a label in that colour all but
    // vanished on the danger button. Text in the surface's colour goes through `surface-solid`.
    expect(tokens).toContain("--color-surface-solid: var(--surface);");
    const roots = ["components", "../../../apps/desktop/src", "../../../apps/mobile/src"];
    const files = roots.flatMap((root) =>
      readdirSync(resolve(__dirname, root), { recursive: true, encoding: "utf8" })
        .filter((name) => name.endsWith(".tsx") && !name.endsWith(".test.tsx"))
        .map((name) => resolve(__dirname, root, name)),
    );
    expect(files.length).toBeGreaterThan(50);
    const offenders = files.filter((file) =>
      /\b(text|border|ring|outline|fill|stroke|divide)-surface\b(?!-)/.test(
        readFileSync(file, "utf8"),
      ),
    );
    expect(offenders).toEqual([]);
  });

  it("an element that names its own accent is not overridden by the page's", () => {
    // The appearance swatches carry data-accent + data-theme; nested theme previews only data-theme.
    for (const accent of ACCENT_IDS.filter((a) => a !== "default")) {
      for (const theme of THEMES) {
        expect(tokens).toContain(
          `[data-accent="${accent}"] [data-theme="${theme}"]:not([data-accent]) {`,
        );
      }
    }
  });
});

describe("theme helpers", () => {
  it("resolves follow-system and writes data attributes on <html>", () => {
    expect(resolveTheme({ theme: "warm", follow_system_theme: false }, true)).toBe("warm");
    expect(resolveTheme({ theme: "warm", follow_system_theme: true }, true)).toBe("dark");
    expect(resolveTheme({ theme: "warm", follow_system_theme: true }, false)).toBe("light");
    applyTheme("graphite");
    expect(document.documentElement.dataset.theme).toBe("graphite");
    expect(readTheme()).toBe("graphite");
    applyAppearance({
      theme: "dark",
      accent: "green",
      density: "compact",
      fontSizePx: 40,
      reduceMotion: true,
    });
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.dataset.accent).toBe("green");
    expect(document.documentElement.dataset.density).toBe("compact");
    expect(document.documentElement.dataset.reduceMotion).toBe("true");
    expect(document.documentElement.style.getPropertyValue("--ui-font-size")).toBe(
      `${FONT_SIZE_MAX}px`,
    );
    applyAppearance({
      theme: "light",
      accent: "default",
      density: "default",
      fontSizePx: 14,
      reduceMotion: false,
    });
    expect(document.documentElement.dataset.accent).toBe("default");
    expect(document.documentElement.style.getPropertyValue("--ui-font-size")).toBe("14px");
    document.documentElement.dataset.theme = "sepia";
    expect(readTheme()).toBe("light");
    delete document.documentElement.dataset.theme;
    expect(readTheme()).toBe("light");
    expect(isThemeId("dark")).toBe(true);
    expect(isThemeId("x")).toBe(false);
    expect(isAccentId("purple")).toBe(true);
    expect(isAccentId("red")).toBe(false);
    expect(isAccentId(undefined)).toBe(false);
  });

  it("follows the system within a theme's pair; warm and graphite follow it as light and dark", () => {
    const follow = (theme: ThemeId, dark: boolean) =>
      resolveTheme({ theme, follow_system_theme: true }, dark);
    expect([follow("glass", false), follow("glass", true), follow("glass_night", false)]).toEqual([
      "glass",
      "glass_night",
      "glass",
    ]);
    expect([follow("paper_night", false), follow("paper", true)]).toEqual(["paper", "paper_night"]);
    for (const theme of ["light", "dark", "warm", "graphite"] as const)
      expect([follow(theme, false), follow(theme, true)], `following ${theme}`).toEqual([
        "light",
        "dark",
      ]);
    expect(resolveTheme({ theme: "paper_night", follow_system_theme: false }, false)).toBe(
      "paper_night",
    );
    expect(themePair("glass_night")).toEqual(["glass", "glass_night"]);
    expect(themePair("graphite")).toEqual(["light", "dark"]);
    // Picking warm or graphite stops following the system (they have no pair of their own).
    expect(THEME_IDS.filter((id) => !followsSystem(id))).toEqual(["warm", "graphite"]);
    expect(THEME_IDS.filter(isDarkTheme)).toEqual([
      "dark",
      "graphite",
      "glass_night",
      "paper_night",
    ]);
  });

  it("lays the themes out in columns of a light and its dark, 暖纸 and 石墨 last", () => {
    expect(THEME_COLUMNS).toEqual([
      ["light", "dark"],
      ["glass", "glass_night"],
      ["paper", "paper_night"],
      ["warm", "graphite"],
    ]);
    // Every theme once.
    expect(THEME_COLUMNS.flat()).toHaveLength(THEME_IDS.length);
    expect(new Set(THEME_COLUMNS.flat())).toEqual(new Set(THEME_IDS));
    for (const [light, dark] of THEME_COLUMNS)
      expect([isDarkTheme(light), isDarkTheme(dark)]).toEqual([false, true]);
    // The columns that follow the system are its pairs.
    const following = THEME_COLUMNS.filter(([light]) => followsSystem(light));
    expect(following.map(([light]) => themePair(light))).toEqual(THEME_COLUMNS.slice(0, 3));
  });

  it("picking a tile: one of the system's own scheme keeps following; any other is a choice of its own", () => {
    const following = { follow_system_theme: true };
    // A light system.
    expect(pickTheme(following, "glass", false)).toEqual({
      theme: "glass",
      follow_system_theme: true,
    });
    expect(pickTheme(following, "light", false).follow_system_theme).toBe(true);
    // What the user clicked is what they see: the other scheme, or a theme without a pair.
    expect(pickTheme(following, "glass_night", false)).toEqual({
      theme: "glass_night",
      follow_system_theme: false,
    });
    expect(pickTheme(following, "warm", false).follow_system_theme).toBe(false);
    // A dark system.
    expect(pickTheme(following, "paper_night", true).follow_system_theme).toBe(true);
    expect(pickTheme(following, "paper", true).follow_system_theme).toBe(false);
    expect(pickTheme(following, "graphite", true).follow_system_theme).toBe(false);
    // Not following, a tile never starts it.
    expect(pickTheme({ follow_system_theme: false }, "glass", false)).toEqual({
      theme: "glass",
      follow_system_theme: false,
    });
    // The pick resolves to the tile itself.
    for (const theme of THEME_IDS)
      for (const dark of [false, true])
        expect(resolveTheme(pickTheme(following, theme, dark), dark), `${theme} ${dark}`).toBe(
          theme,
        );
  });

  it("reads system preferences defensively", () => {
    expect(systemPrefersDark({})).toBe(false);
    expect(systemPrefersDark({ matchMedia: () => ({ matches: true }) as MediaQueryList })).toBe(
      true,
    );
    expect(
      systemPrefersReducedMotion({ matchMedia: () => ({ matches: true }) as MediaQueryList }),
    ).toBe(true);
    expect(systemPrefersReducedMotion({})).toBe(false);
    expect(typeof systemPrefersDark()).toBe("boolean");
    expect(typeof systemPrefersReducedMotion()).toBe("boolean");
  });
});

describe("cursor", () => {
  const base = tokens.slice(tokens.indexOf("@layer base"));
  const rule = (cursor: string) =>
    new RegExp(`:where\\(([^{}]*)\\)\\s*\\{\\s*cursor:\\s*${cursor};`, "s").exec(base)?.[1] ?? "";

  it("regression: every control shows the hand cursor and a disabled one the not-allowed cursor, since Tailwind v4 leaves buttons on the arrow (user feedback 2026-09-28)", () => {
    const pointer = rule("pointer");
    for (const selector of [
      "button",
      "select",
      "summary",
      "a[href]",
      "label[for]",
      '[role="button"]',
      '[role="tab"]',
      '[role="option"]',
      '[role="radio"]',
      '[role="switch"]',
      '[role="checkbox"]',
      '[role="menuitem"]',
    ])
      expect(pointer).toContain(selector);
    expect(base).toMatch(
      /:where\(button, select, input, textarea\):disabled,\s*:where\(\[aria-disabled="true"\]\)\s*\{\s*cursor:\s*not-allowed;/,
    );
  });
});

const TAGS = [
  "red",
  "orange",
  "amber",
  "green",
  "teal",
  "blue",
  "indigo",
  "purple",
  "pink",
  "gray",
];

describe("account colours", () => {
  it.each(THEMES)(
    "theme %s carries every account colour with its text at 4.5:1 or more",
    (theme) => {
      const block = themeBlock(theme);
      for (const tag of TAGS) {
        const fill = new RegExp(`--tag-${tag}: (#[0-9a-f]{6});`).exec(block)?.[1];
        const text = new RegExp(`--tag-${tag}-text: (#[0-9a-f]{6});`).exec(block)?.[1];
        if (fill === undefined || text === undefined)
          throw new Error(`${theme} lacks the ${tag} pair`);
        expect(contrast(fill, text), `${theme} ${tag}`).toBeGreaterThanOrEqual(4.5);
      }
    },
  );

  it("picks the pair through data-tag, falls back to the neutral fill and reaches Tailwind", () => {
    for (const tag of TAGS)
      expect(tokens).toContain(
        `[data-tag="${tag}"] {\n  --tag-bg: var(--tag-${tag});\n  --tag-fg: var(--tag-${tag}-text);\n}`,
      );
    expect(tokens).toMatch(
      /:root \{\n {2}--tag-bg: var\(--inset2\);\n {2}--tag-fg: var\(--fg-muted\);/,
    );
    expect(tokens).toContain("--color-tag-bg: var(--tag-bg);");
    expect(tokens).toContain("--color-tag-fg: var(--tag-fg);");
  });

  it("are the contract's colours", () => {
    expect(TAGS).toEqual(ACCOUNT_COLORS.filter((c) => c !== "auto"));
  });
});
