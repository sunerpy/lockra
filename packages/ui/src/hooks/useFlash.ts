import { useCallback, useEffect, useState } from "react";

/** How long a card shows that it was copied. */
export const FLASH_MS = 1600;

/** A short-lived mark on one item (the card just copied): `flash(id)` sets it and it clears by
 *  itself after `ms`; another flash, of the same item or not, starts the time again. */
export function useFlash(ms = FLASH_MS): readonly [string | undefined, (id: string) => void] {
  // A new object on every flash, so a second flash of the same card restarts the timer.
  const [current, setCurrent] = useState<{ id: string } | undefined>(undefined);
  const flash = useCallback((id: string) => setCurrent({ id }), []);
  useEffect(() => {
    if (current === undefined) return;
    const timer = setTimeout(() => setCurrent(undefined), ms);
    return () => clearTimeout(timer);
  }, [current, ms]);
  return [current?.id, flash] as const;
}
