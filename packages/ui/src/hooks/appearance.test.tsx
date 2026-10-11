import { defaultSettings } from "@lockra/shared";
import { act, renderHook } from "@testing-library/react";
import { motionReduced, useAppearance } from "./appearance";

describe("useAppearance", () => {
  it("writes the settings onto <html>, following the system theme when asked", () => {
    const listeners: (() => void)[] = [];
    let dark = true;
    vi.stubGlobal("matchMedia", (query: string) => ({
      get matches() {
        return query.includes("dark") ? dark : false;
      },
      addEventListener: (_: string, l: () => void) => listeners.push(l),
      removeEventListener: () => undefined,
    }));
    try {
      const settings = {
        ...defaultSettings(),
        accent: "purple" as const,
        density: "compact" as const,
      };
      const { rerender } = renderHook(({ s }) => useAppearance(s), {
        initialProps: { s: settings },
      });
      expect(document.documentElement.dataset.theme).toBe("dark");
      expect(document.documentElement.dataset.accent).toBe("purple");
      expect(document.documentElement.dataset.density).toBe("compact");
      rerender({ s: { ...settings, follow_system_theme: false, theme: "warm" } });
      expect(document.documentElement.dataset.theme).toBe("warm");
      dark = false;
      rerender({ s: settings });
      act(() => {
        for (const l of listeners) l();
      });
      expect(document.documentElement.dataset.theme).toBe("light");
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("does nothing before the first state", () => {
    renderHook(() => useAppearance(undefined));
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("reduces motion for the setting", () => {
    expect(motionReduced({ ...defaultSettings(), reduce_motion: true })).toBe(true);
    expect(motionReduced(defaultSettings())).toBe(false);
  });
});
