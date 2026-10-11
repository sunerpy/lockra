// Theme, accent, density and font size on `<html>`: every colour in the app follows from the CSS
// variables `tokens.css` defines per `data-theme` and `data-accent`.
import {
  ACCENT_IDS,
  type AccentId,
  type Density,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  type Settings,
  THEME_IDS,
  type ThemeId,
} from "@lockra/shared";

export { ACCENT_IDS, FONT_SIZE_MAX, FONT_SIZE_MIN, THEME_IDS };
export type { AccentId, Density, ThemeId };

export const FONT_SIZE_DEFAULT = 14;

export interface Appearance {
  theme: ThemeId;
  accent: AccentId;
  density: Density;
  fontSizePx: number;
  reduceMotion: boolean;
}

export function isThemeId(value: string): value is ThemeId {
  return (THEME_IDS as readonly string[]).includes(value);
}

export function isAccentId(value: unknown): value is AccentId {
  return typeof value === "string" && (ACCENT_IDS as readonly string[]).includes(value);
}

/** The dark themes: the second of each pair, and graphite. */
const DARK_THEMES: ReadonlySet<ThemeId> = new Set([
  "dark",
  "graphite",
  "glass_night",
  "paper_night",
]);

export function isDarkTheme(theme: ThemeId): boolean {
  return DARK_THEMES.has(theme);
}

/** The light and the dark theme that following the system switches between: 澄空 ↔ 澄空 · 夜 and
 *  墨白 ↔ 墨白 · 夜; warm and graphite have no pair of their own and follow it as light ↔ dark, as
 *  every theme did before the pairs. */
export function themePair(theme: ThemeId): readonly [ThemeId, ThemeId] {
  if (theme === "glass" || theme === "glass_night") return ["glass", "glass_night"];
  if (theme === "paper" || theme === "paper_night") return ["paper", "paper_night"];
  return ["light", "dark"];
}

/** Whether picking `theme` keeps following the system: warm and graphite stand alone. */
export function followsSystem(theme: ThemeId): boolean {
  return theme !== "warm" && theme !== "graphite";
}

/** Settings › 外观's grid: a column per light theme with its dark one below it (the pairs that
 *  following the system switches between), 暖纸 and 石墨 last. */
export const THEME_COLUMNS: readonly (readonly [ThemeId, ThemeId])[] = [
  ["light", "dark"],
  ["glass", "glass_night"],
  ["paper", "paper_night"],
  ["warm", "graphite"],
];

/** What picking a theme writes. Following the system, a theme of the system's own scheme chooses
 *  its column and keeps following; any other (the other scheme, or 暖纸 / 石墨, which have no pair)
 *  is a choice of its own and stops following, so what was picked is what shows. */
export function pickTheme(
  settings: Pick<Settings, "follow_system_theme">,
  theme: ThemeId,
  systemDark: boolean,
): Pick<Settings, "theme" | "follow_system_theme"> {
  return {
    theme,
    follow_system_theme:
      settings.follow_system_theme && followsSystem(theme) && isDarkTheme(theme) === systemDark,
  };
}

/** Which theme the settings resolve to, following the system within the theme's pair. */
export function resolveTheme(
  settings: Pick<Settings, "theme" | "follow_system_theme">,
  systemDark: boolean,
): ThemeId {
  if (!settings.follow_system_theme) return settings.theme;
  const [light, dark] = themePair(settings.theme);
  return systemDark ? dark : light;
}

/** The appearance the settings ask for on this system. */
export function appearanceOf(
  settings: Settings,
  systemDark: boolean,
  systemReducedMotion: boolean,
): Appearance {
  return {
    theme: resolveTheme(settings, systemDark),
    accent: settings.accent,
    density: settings.density,
    fontSizePx: settings.font_size_px,
    reduceMotion: settings.reduce_motion || systemReducedMotion,
  };
}

export interface MediaQueryHost {
  matchMedia?: Window["matchMedia"];
}

export function systemPrefersDark(win: MediaQueryHost = globalThis.window): boolean {
  if (typeof win.matchMedia !== "function") return false;
  return win.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function systemPrefersReducedMotion(win: MediaQueryHost = globalThis.window): boolean {
  if (typeof win.matchMedia !== "function") return false;
  return win.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Writes `data-theme` on `<html>`. */
export function applyTheme(theme: ThemeId, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = theme;
}

export function applyAppearance(
  appearance: Appearance,
  root: HTMLElement = document.documentElement,
): void {
  applyTheme(appearance.theme, root);
  root.dataset.accent = appearance.accent;
  root.dataset.density = appearance.density;
  root.dataset.reduceMotion = appearance.reduceMotion ? "true" : "false";
  const size = Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, appearance.fontSizePx));
  root.style.setProperty("--ui-font-size", `${size}px`);
}

export function readTheme(root: HTMLElement = document.documentElement): ThemeId {
  const current = root.dataset.theme ?? "light";
  return isThemeId(current) ? current : "light";
}
