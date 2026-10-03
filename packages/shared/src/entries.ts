// The code list's logic, shared by the desktop and the phone: the order, the search, the sections
// of groups, the groups in use, an account's avatar text cut to its length, and the kind a form's
// fields describe.
import { type EntryView, MARK_CHARS, type OtpKind, type SortOrder } from "./schema";

/** How long a revealed secret (an account's, the sync key, an invitation) stays on screen. */
export const REVEAL_SECONDS = 120;

/** The group menu's "all groups". */
export const ALL_GROUPS = "";
/** The section of the accounts in no group (its fold is kept under this name too). */
export const NO_GROUP = "";

function nameOf(entry: EntryView): string {
  return (entry.issuer || entry.account).toLocaleLowerCase();
}

function byName(a: EntryView, b: EntryView): number {
  return nameOf(a).localeCompare(nameOf(b)) || a.account.localeCompare(b.account);
}

/** Favourites first, then `order`; ties by name. */
export function sortEntries(entries: readonly EntryView[], order: SortOrder): EntryView[] {
  const key: (a: EntryView, b: EntryView) => number =
    order === "added"
      ? (a, b) => b.created_at_ms - a.created_at_ms
      : order === "recent"
        ? (a, b) => (b.last_used_at_ms ?? 0) - (a.last_used_at_ms ?? 0)
        : byName;
  // A sorted copy: `toSorted` is ES2023 (Safari 16), past the ES2022 lib kept for older macOS.
  // oxlint-disable-next-line unicorn/no-array-sort
  return [...entries].sort(
    (a, b) => Number(b.favorite) - Number(a.favorite) || key(a, b) || byName(a, b),
  );
}

/** The accounts whose issuer, account or group contains `query`, in `group` (all when empty). */
export function filterEntries(
  entries: readonly EntryView[],
  query: string,
  group: string,
): EntryView[] {
  const q = query.trim().toLocaleLowerCase();
  return entries.filter(
    (e) =>
      (group === ALL_GROUPS || e.group === group) &&
      (q === "" || `${e.issuer}\n${e.account}\n${e.group ?? ""}`.toLocaleLowerCase().includes(q)),
  );
}

/** One section of the code list: a group's accounts, or those in no group (`key` ""). */
export interface GroupSection {
  key: string;
  entries: EntryView[];
}

/** The accounts in sections: one per group by name, then the accounts in no group; each keeps the
 *  order it is given. */
export function groupSections(entries: readonly EntryView[]): GroupSection[] {
  const byGroup = new Map<string, EntryView[]>();
  for (const entry of entries) {
    const key = entry.group ?? NO_GROUP;
    const section = byGroup.get(key);
    if (section) section.push(entry);
    else byGroup.set(key, [entry]);
  }
  const keys = [...byGroup.keys()].filter((key) => key !== NO_GROUP);
  // A sorted copy: `toSorted` is ES2023 (Safari 16), past the ES2022 lib kept for older macOS.
  // oxlint-disable-next-line unicorn/no-array-sort
  keys.sort((a, b) => a.localeCompare(b));
  if (byGroup.has(NO_GROUP)) keys.push(NO_GROUP);
  return keys.map((key) => ({ key, entries: byGroup.get(key) ?? [] }));
}

/** The groups the accounts are in, sorted, each once. */
export function entryGroups(entries: readonly EntryView[]): string[] {
  const groups = new Set<string>();
  for (const entry of entries)
    if (entry.group !== null && entry.group !== "") groups.add(entry.group);
  // A sorted copy: `toSorted` is ES2023 (Safari 16), past the ES2022 lib kept for older macOS.
  // oxlint-disable-next-line unicorn/no-array-sort
  return [...groups].sort((a, b) => a.localeCompare(b));
}

/** `text` cut to `MARK_CHARS` characters as people count them (an emoji with its joiners is one). */
export function cutMark(text: string): string {
  const segments = new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(text);
  return Array.from(segments)
    .slice(0, MARK_CHARS)
    .map((s) => s.segment)
    .join("");
}

/** The kind from a form's fields: `period` for TOTP, `counter` for HOTP; `undefined` when out of
 *  the core's ranges. */
export function parseKind(
  type: OtpKind["type"],
  period: string,
  counter: string,
): OtpKind | undefined {
  if (type === "totp") {
    const value = Number(period);
    return Number.isInteger(value) && value >= 1 && value <= 3600
      ? { type, period: value }
      : undefined;
  }
  const value = Number(counter);
  return Number.isSafeInteger(value) && value >= 0 && counter.trim() !== ""
    ? { type, counter: value }
    : undefined;
}
