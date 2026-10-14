// What the background remembers per tab, in memory only and for minutes: the user name of a
// sign-in's first step, the login just filled (for the code of a second step), a submitted form
// waiting for the user's answer to the save prompt, and the shortcut's round of accounts. The
// background forgets it all when the tab closes, as Lockra locks, and when it restarts.
import type { ItemSummary } from "../protocol/schemas";
import type { TrustedFrame } from "./frames";
import type { Clock } from "./clock";

/** How long a step's user name, a fill and a submitted form are kept (plan §四). */
export const KEEP_MS = 5 * 60_000;
/** Pressing the shortcut again within this moves to the next account. */
export const CYCLE_MS = 10_000;

export interface Submission {
  username: string;
  password: string;
  /** The frame it was submitted in, as it was then: what a save or an update names. */
  frame: TrustedFrame;
  /** What the app advised: save a new login or update `item`'s password. */
  action: "save" | "update";
  item: ItemSummary | null;
  /** The site the app named. */
  site: string;
}

interface Kept<T> {
  value: T;
  at: number;
}

export class TabMemory {
  private readonly usernames = new Map<number, Kept<string>>();
  private readonly fills = new Map<number, Kept<string>>();
  private readonly submissions = new Map<number, Kept<Submission>>();
  private readonly cycles = new Map<
    number,
    Kept<{ frameId: number; items: string[]; index: number }>
  >();

  constructor(private readonly clock: Clock) {}

  private fresh<T>(map: Map<number, Kept<T>>, tabId: number, within = KEEP_MS): T | null {
    const kept = map.get(tabId);
    if (!kept) return null;
    if (this.clock.now() - kept.at >= within) {
      map.delete(tabId);
      return null;
    }
    return kept.value;
  }

  rememberUsername(tabId: number, username: string): void {
    if (username.trim() === "") return;
    this.usernames.set(tabId, { value: username, at: this.clock.now() });
  }

  username(tabId: number): string | null {
    return this.fresh(this.usernames, tabId);
  }

  rememberFill(tabId: number, itemId: string): void {
    this.fills.set(tabId, { value: itemId, at: this.clock.now() });
  }

  filledItem(tabId: number): string | null {
    return this.fresh(this.fills, tabId);
  }

  rememberSubmission(tabId: number, submission: Submission): void {
    this.submissions.set(tabId, { value: submission, at: this.clock.now() });
  }

  submission(tabId: number): Submission | null {
    return this.fresh(this.submissions, tabId);
  }

  dropSubmission(tabId: number): void {
    this.submissions.delete(tabId);
  }

  /** The shortcut's account for this frame: the first of `ids` on a new round, the one after the
   *  last within ten seconds (the same frame, the same matches). */
  cycle(tabId: number, frameId: number, ids: readonly string[]): string | null {
    if (ids.length === 0) {
      this.cycles.delete(tabId);
      return null;
    }
    const round = this.fresh(this.cycles, tabId, CYCLE_MS);
    const same = round !== null && round.frameId === frameId && round.items.join() === ids.join();
    const index = same ? (round.index + 1) % ids.length : 0;
    this.cycles.set(tabId, { value: { frameId, items: [...ids], index }, at: this.clock.now() });
    return ids[index] ?? null;
  }

  forget(tabId: number): void {
    this.usernames.delete(tabId);
    this.fills.delete(tabId);
    this.submissions.delete(tabId);
    this.cycles.delete(tabId);
  }

  clear(): void {
    this.usernames.clear();
    this.fills.clear();
    this.submissions.clear();
    this.cycles.clear();
  }
}
