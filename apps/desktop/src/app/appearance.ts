// Applies the settings to `<html>` (theme, accent, density, font size, motion) and follows the
// system's light/dark and reduced-motion preferences while they are what the settings ask for.
import type { Settings } from "@lockra/shared";
import {
  appearanceOf,
  applyAppearance,
  systemPrefersDark,
  systemPrefersReducedMotion,
} from "@lockra/ui";
import { useEffect, useState } from "react";

function useMediaQuery(query: string, read: () => boolean): boolean {
  const [matches, setMatches] = useState(read);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return undefined;
    const list = window.matchMedia(query);
    const onChange = () => setMatches(list.matches);
    list.addEventListener("change", onChange);
    return () => list.removeEventListener("change", onChange);
  }, [query]);
  return matches;
}

export function useAppearance(settings: Settings | undefined): void {
  const dark = useMediaQuery("(prefers-color-scheme: dark)", () => systemPrefersDark());
  const reduced = useMediaQuery("(prefers-reduced-motion: reduce)", () =>
    systemPrefersReducedMotion(),
  );
  useEffect(() => {
    if (settings) applyAppearance(appearanceOf(settings, dark, reduced));
  }, [settings, dark, reduced]);
}

/** Whether motion is off (the setting or the system). */
export function motionReduced(settings: Settings): boolean {
  return settings.reduce_motion || systemPrefersReducedMotion();
}
