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

/** Which theme the settings resolve to, honouring follow-system (light ↔ dark only). */
export function resolveTheme(
  settings: Pick<Settings, "theme" | "follow_system_theme">,
  systemDark: boolean,
): ThemeId {
  if (settings.follow_system_theme) return systemDark ? "dark" : "light";
  return settings.theme;
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
