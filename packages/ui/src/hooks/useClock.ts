import { useSyncExternalStore } from "react";

// One shared clock for every countdown: it ticks on each whole second (wall clock), so every ring
// and every "next code" switch moves together, and it stops when nothing is subscribed.
const listeners = new Set<() => void>();
let current = Date.now();
let timer: ReturnType<typeof setTimeout> | undefined;

function schedule(): void {
  timer = setTimeout(tick, 1000 - (Date.now() % 1000));
}

function tick(): void {
  current = Date.now();
  for (const listener of listeners) listener();
  schedule();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (timer === undefined) {
    current = Date.now();
    schedule();
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && timer !== undefined) {
      clearTimeout(timer);
      timer = undefined;
    }
  };
}

function snapshot(): number {
  return current;
}

/** Unix milliseconds, refreshed on every whole second while any component listens. */
export function useClock(): number {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/** For tests: whether the shared timer is running. */
export function clockRunning(): boolean {
  return timer !== undefined;
}
