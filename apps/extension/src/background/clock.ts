/** Time and timers, injected so the background's state machine runs on a test's clock. */
export interface Clock {
  now(): number;
  setTimeout(fn: () => void, ms: number): number;
  clearTimeout(id: number | undefined): void;
}

export const systemClock: Clock = {
  now: () => Date.now(),
  setTimeout: (fn, ms) => Number(globalThis.setTimeout(fn, ms)),
  clearTimeout: (id) => globalThis.clearTimeout(id),
};
