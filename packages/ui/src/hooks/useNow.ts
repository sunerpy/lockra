import { useSyncExternalStore } from "react";

const listeners = new Set<() => void>();
/** Now in milliseconds, whole seconds: the same value for every render between two ticks. */
const wholeSeconds = () => Math.floor(Date.now() / 1000) * 1000;

let cached = wholeSeconds();
let timer: ReturnType<typeof setInterval> | undefined;

export const NOW_REFRESH_MS = 30_000;

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (timer === undefined) {
    // The module-level value may be hours old by the time the first subscriber mounts.
    cached = wholeSeconds();
    timer = setInterval(() => {
      cached = wholeSeconds();
      for (const l of listeners) l();
    }, NOW_REFRESH_MS);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && timer !== undefined) {
      clearInterval(timer);
      timer = undefined;
    }
  };
}

function snapshot(): number {
  return cached;
}

/** The time in Unix milliseconds (whole seconds), refreshed every 30 s while any subscriber is
 *  mounted; pure during render. For the minutes-and-hours labels (`relativeTime`); the codes tick
 *  with `useClock`. */
export function useNow(): number {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}
