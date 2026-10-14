import type { Clock } from "../background/clock";

/** A clock the test moves: timers run only when it says so. */
export class ManualClock implements Clock {
  private t = 1_760_000_000_000;
  private n = 0;
  private timers: { at: number; id: number; fn: () => void }[] = [];

  now(): number {
    return this.t;
  }

  setTimeout(fn: () => void, ms: number): number {
    const id = ++this.n;
    this.timers.push({ at: this.t + Math.max(0, ms), id, fn });
    return id;
  }

  clearTimeout(id: number | undefined): void {
    this.timers = this.timers.filter((timer) => timer.id !== id);
  }

  /** Timers waiting. */
  get pending(): number {
    return this.timers.length;
  }

  /** Move on by `ms`, running every timer that comes due on the way, in order. */
  advance(ms: number): void {
    const until = this.t + ms;
    for (;;) {
      const next = this.timers.toSorted((a, b) => a.at - b.at || a.id - b.id)[0];
      if (!next || next.at > until) break;
      this.timers = this.timers.filter((timer) => timer !== next);
      this.t = next.at;
      next.fn();
    }
    this.t = until;
  }
}
