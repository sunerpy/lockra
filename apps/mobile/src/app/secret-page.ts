// A secret on a page of its own (an account's secret, a new space's sync key, an invitation): it
// hides after REVEAL_SECONDS, closing its page, and however the page goes the core hears that the
// secret view ended, as on the desktop. The window never shows in screenshots (FLAG_SECURE,
// MainActivity.kt).
import { REVEAL_SECONDS } from "@lockra/shared";
import { useBackend, useClock } from "@lockra/ui";
import { useCallback, useEffect, useRef } from "react";
import { useNav } from "./nav";

/** Seconds left of a secret shown at `shownAt` (`undefined`: not shown yet). */
export function useSecretPage(shownAt: number | undefined): number {
  const { backend } = useBackend();
  const { back } = useNav();
  const now = useClock();
  const shown = shownAt !== undefined;
  useEffect(() => {
    if (!shown) return undefined;
    return () => {
      void backend.dispatch({ command: "secret_view_closed" }).catch(() => undefined);
    };
  }, [shown, backend]);
  // The shared clock ticks on whole seconds, so it can read just before the moment it showed.
  const left =
    shownAt === undefined
      ? REVEAL_SECONDS
      : Math.max(0, REVEAL_SECONDS - Math.max(0, Math.floor((now - shownAt) / 1000)));
  useEffect(() => {
    if (shown && left === 0) back();
  }, [shown, left, back]);
  return left;
}

/** `deliver(show)`: shows a secret answer while the page that asked for it is still up; once it is
 *  gone, nothing shows the secret, so the secret view ends at once. */
export function useSecretAnswer(): (show: () => void) => void {
  const { backend } = useBackend();
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  return useCallback(
    (show: () => void) => {
      if (mounted.current) show();
      else void backend.dispatch({ command: "secret_view_closed" }).catch(() => undefined);
    },
    [backend],
  );
}
