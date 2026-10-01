// The idle timer of the auto-lock restarts on every key press and click; the core hears about it
// at most every 15 seconds.
import type { Backend } from "@lockra/shared";
import { useEffect } from "react";

export const ACTIVITY_THROTTLE_MS = 15_000;

export function useActivityPing(
  backend: Backend,
  active: boolean,
  now: () => number = Date.now,
): void {
  useEffect(() => {
    if (!active) return undefined;
    let last = 0;
    const onActivity = () => {
      const at = now();
      if (at - last < ACTIVITY_THROTTLE_MS) return;
      last = at;
      void backend.dispatch({ command: "activity" }).catch(() => undefined);
    };
    window.addEventListener("pointerdown", onActivity, true);
    window.addEventListener("keydown", onActivity, true);
    return () => {
      window.removeEventListener("pointerdown", onActivity, true);
      window.removeEventListener("keydown", onActivity, true);
    };
  }, [backend, active, now]);
}
