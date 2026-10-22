// What the background remembers per tab, in memory only and for minutes: the user name of a
// sign-in's first step, the login just filled (for the code of a second step), a login whose step
// took the user name alone (its password comes next), a submitted form waiting for the user's
// answer to the save prompt, a sign-up form's generated password, and the shortcut's round of
// accounts. The background forgets it all when the tab closes, as Lockra locks, and when it
// restarts.
import type { ItemSummary } from "../protocol/schemas";
import type { TrustedFrame } from "./frames";
import type { Clock } from "./clock";

/** How long a step's user name, a fill and a submitted form are kept (plan §四). */
export const KEEP_MS = 5 * 60_000;
/** Pressing the shortcut again within this moves to the next account. */
export const CYCLE_MS = 10_000;
/** How long the password of a step that took the user name alone is waited for. */
export const STEP_MS = 2 * 60_000;

export interface Submission {
  username: string;
  password: string;
  /** The frame it was submitted in, as it was then: what a save or an update names. */
  frame: TrustedFrame;
  /** What the app advised: save a new login, update `item`'s password, or give `item` (saved
   *  without a user name) the one submitted. */
  action: "save" | "update" | "username";
  item: ItemSummary | null;
  /** What was sent looks like a one-time code, and this login of the site has none linked: the
   *  prompt offers to link one in Lockra. */
  link?: ItemSummary | null;
  /** The site the app named. */
  site: string;
}

/** A sign-up form's generated password: every menu of the form offers it, until the user asks for
 *  another or five minutes pass without a menu of the form. */
export interface Generated {
  /** The form: its frame, its document and the page's rules. */
  form: string;
  password: string;
  /** The login the form's password was saved as, and with which password: using the same one
   *  again saves nothing, a newer one updates that login. */
  saved: { itemId: string; password: string } | null;
}

interface Kept<T> {
  value: T;
  at: number;
}

export class TabMemory {
  private readonly usernames = new Map<number, Kept<string>>();
  private readonly fills = new Map<number, Kept<string>>();
  private readonly steps = new Map<number, Kept<string>>();
  private readonly submissions = new Map<number, Kept<Submission>>();
  private readonly generated = new Map<number, Kept<Generated>>();
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

  /** A step filled `itemId`'s user name alone in this tab: its password comes next. */
  rememberStep(tabId: number, itemId: string): void {
    this.steps.set(tabId, { value: itemId, at: this.clock.now() });
  }

  /** The login whose password the next step of this tab's sign-in takes, if one waits. */
  step(tabId: number): string | null {
    return this.fresh(this.steps, tabId, STEP_MS);
  }

  /** The waiting step's login, taken: a step is filled once. */
  takeStep(tabId: number): string | null {
    const item = this.step(tabId);
    this.steps.delete(tabId);
    return item;
  }

  dropStep(tabId: number): void {
    this.steps.delete(tabId);
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

  rememberGenerated(tabId: number, generated: Generated): void {
    this.generated.set(tabId, { value: generated, at: this.clock.now() });
  }

  /** The password generated for `form` in this tab, if a menu of the form showed it within five
   *  minutes; each look counts as one. */
  generatedFor(tabId: number, form: string): Generated | null {
    const kept = this.fresh(this.generated, tabId);
    if (!kept || kept.form !== form) return null;
    this.rememberGenerated(tabId, kept);
    return kept;
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
    this.steps.delete(tabId);
    this.submissions.delete(tabId);
    this.generated.delete(tabId);
    this.cycles.delete(tabId);
  }

  clear(): void {
    this.usernames.clear();
    this.fills.clear();
    this.steps.clear();
    this.submissions.clear();
    this.generated.clear();
    this.cycles.clear();
  }
}
