// The code list's logic, shared by the desktop and the phone: the order, the search, the sections
// of groups, the groups in use, an account's avatar text cut to its length, the kind a form's
// fields describe, and where an account can be exported.
import {
  type EntryView,
  type ExportTarget,
  MARK_CHARS,
  type OtpKind,
  type PinnedGroup,
  type SortOrder,
} from "./schema";

/** How long a revealed secret (an account's, the sync key, an invitation) stays on screen. */
export const REVEAL_SECONDS = 120;

/** lockra-core `EXPORT_IDLE`: an export page stays up this long after it was shown. */
export const EXPORT_SECONDS = 120;

/** Where accounts go out: another authenticator's QR codes, or a plain otpauth list file. */
export type ExportWay = ExportTarget | "file";

/** Why `entry` cannot go to `way`, or `null` when it can (a file takes every account). */
export function exportBlocker(entry: EntryView, way: ExportWay): EntryView["export"]["google"] {
  return way === "file" ? null : entry.export[way];
}

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

/** Favourites first, then `order`; ties by name. The `manual` order follows `dragged` (the ids in
 *  the order they were dragged into); the accounts never dragged come after it, oldest first. */
export function sortEntries(
  entries: readonly EntryView[],
  order: SortOrder,
  dragged: readonly string[] = [],
): EntryView[] {
  const place = new Map(dragged.map((id, at) => [id, at]));
  // Two accounts never dragged: Infinity − Infinity is NaN, which falls through to the next key.
  const placed = (a: EntryView, b: EntryView) =>
    (place.get(a.id) ?? Infinity) - (place.get(b.id) ?? Infinity) ||
    a.created_at_ms - b.created_at_ms;
  const key: (a: EntryView, b: EntryView) => number =
    order === "added"
      ? (a, b) => b.created_at_ms - a.created_at_ms
      : order === "recent"
        ? (a, b) => (b.last_used_at_ms ?? 0) - (a.last_used_at_ms ?? 0)
        : order === "manual"
          ? placed
          : byName;
  // A sorted copy: `toSorted` is ES2023 (Safari 16), past the ES2022 lib kept for older macOS.
  // oxlint-disable-next-line unicorn/no-array-sort
  return [...entries].sort(
    (a, b) => Number(b.favorite) - Number(a.favorite) || key(a, b) || byName(a, b),
  );
}

/** The codes page's chips beside a group's name: the pinned accounts, and those in no group. A
 *  group name cannot start with a control character, so neither can be one. */
export const FAVORITES = "\u0001favorites";
export const UNGROUPED = "\u0001ungrouped";

/** The accounts whose issuer, account or group contains `query`, in `group` (all when empty; the
 *  pinned ones for {@link FAVORITES}, those in no group for {@link UNGROUPED}). */
export function filterEntries(
  entries: readonly EntryView[],
  query: string,
  group: string,
): EntryView[] {
  const q = query.trim().toLocaleLowerCase();
  const inGroup = (e: EntryView) =>
    group === ALL_GROUPS ||
    (group === FAVORITES
      ? e.favorite
      : group === UNGROUPED
        ? (e.group ?? "") === ""
        : e.group === group);
  return entries.filter(
    (e) =>
      inGroup(e) &&
      (q === "" || `${e.issuer}\n${e.account}\n${e.group ?? ""}`.toLocaleLowerCase().includes(q)),
  );
}

/** The account copied last (the codes page's 「最近使用」), or none while nothing was copied. */
export function recentEntry(entries: readonly EntryView[]): EntryView | undefined {
  let best: EntryView | undefined;
  for (const e of entries)
    if (
      e.last_used_at_ms !== null &&
      (best === undefined || e.last_used_at_ms > (best.last_used_at_ms ?? 0))
    )
      best = e;
  return best;
}

/** One section of the code list: a group's accounts, those in no group (`key` ""), or the pinned
 *  accounts gathered at the top (`key` {@link FAVORITES}). */
export interface GroupSection {
  key: string;
  entries: EntryView[];
}

/** Undragged, the accounts in no group come after the groups. */
function lastIfNoGroup(key: string): number {
  return key === NO_GROUP ? 1 : 0;
}

/** The section `pinned` puts first, by its key (none for the pinned accounts' own section). */
function pinnedKey(pinned: PinnedGroup | null): string | undefined {
  if (pinned?.type === "group") return pinned.name;
  return pinned?.type === "ungrouped" ? NO_GROUP : undefined;
}

/** The accounts in sections, each keeping the order it is given: the pinned one first (the pinned
 *  accounts gathered out of their groups, one group, or those in no group), then the groups in
 *  `dragged` in that order ("" is the accounts in no group, which move like a group), the others by
 *  name, and the accounts in no group last unless they were dragged. */
export function groupSections(
  entries: readonly EntryView[],
  dragged: readonly string[] = [],
  pinned: PinnedGroup | null = null,
): GroupSection[] {
  const place = new Map(dragged.map((group, at) => [group, at]));
  const gathered = pinned?.type === "favorites" ? entries.filter((e) => e.favorite) : [];
  const byGroup = new Map<string, EntryView[]>();
  for (const entry of entries) {
    if (gathered.includes(entry)) continue;
    const key = entry.group ?? NO_GROUP;
    const section = byGroup.get(key);
    if (section) section.push(entry);
    else byGroup.set(key, [entry]);
  }
  const keys = [...byGroup.keys()];
  // A sorted copy: `toSorted` is ES2023 (Safari 16), past the ES2022 lib kept for older macOS.
  // oxlint-disable-next-line unicorn/no-array-sort
  keys.sort(
    (a, b) =>
      (place.get(a) ?? Infinity) - (place.get(b) ?? Infinity) ||
      lastIfNoGroup(a) - lastIfNoGroup(b) ||
      a.localeCompare(b),
  );
  const first = pinnedKey(pinned);
  if (first !== undefined && byGroup.has(first))
    keys.unshift(...keys.splice(keys.indexOf(first), 1));
  const sections = keys.map((key) => ({ key, entries: byGroup.get(key) ?? [] }));
  return gathered.length > 0 ? [{ key: FAVORITES, entries: gathered }, ...sections] : sections;
}

/** The filter a pin opens the code list on ({@link ALL_GROUPS} without one). */
export function pinnedFilter(pinned: PinnedGroup | null): string {
  if (pinned === null) return ALL_GROUPS;
  if (pinned.type === "favorites") return FAVORITES;
  return pinned.type === "ungrouped" ? UNGROUPED : pinned.name;
}

/** The pin a filter names: none for {@link ALL_GROUPS}. */
export function pinnedFromFilter(filter: string): PinnedGroup | null {
  if (filter === ALL_GROUPS) return null;
  if (filter === FAVORITES) return { type: "favorites" };
  return filter === UNGROUPED ? { type: "ungrouped" } : { type: "group", name: filter };
}

/** One of the code list's filters: everything, the pinned accounts, a group, those in no group. */
export interface EntryFilter {
  value: string;
  kind: "all" | "favorites" | "group" | "ungrouped";
  count: number;
  /** The group pinned on this device: first of the filters, and the one the list opens on. */
  pinned: boolean;
}

/** The code list's filters, each with its number of accounts: 全部, 收藏 (when an account is
 *  pinned), each group by name, and 未分组 (when some accounts are in no group beside groups); the
 *  pinned one first. */
export function entryFilters(
  entries: readonly EntryView[],
  pinned: PinnedGroup | null,
): EntryFilter[] {
  const counts = new Map<string, number>();
  for (const e of entries)
    counts.set(e.group ?? NO_GROUP, (counts.get(e.group ?? NO_GROUP) ?? 0) + 1);
  const groups = entryGroups(entries);
  const favorites = entries.filter((e) => e.favorite).length;
  const loose = counts.get(NO_GROUP) ?? 0;
  const pin = pinned === null ? undefined : pinnedFilter(pinned);
  const filter = (value: string, kind: EntryFilter["kind"], count: number): EntryFilter => ({
    value,
    kind,
    count,
    pinned: value === pin,
  });
  const filters = [
    filter(ALL_GROUPS, "all", entries.length),
    ...(favorites > 0 ? [filter(FAVORITES, "favorites", favorites)] : []),
    ...groups.map((g) => filter(g, "group", counts.get(g) ?? 0)),
    ...(groups.length > 0 && loose > 0 ? [filter(UNGROUPED, "ungrouped", loose)] : []),
  ];
  const at = filters.findIndex((f) => f.pinned);
  return at > 0 ? [...filters.splice(at, 1), ...filters] : filters;
}

/** `list` with `item` moved to where `target` is (a drop on it, or one step with the keyboard). */
export function moveItem<T>(list: readonly T[], item: T, target: T): T[] {
  const from = list.indexOf(item);
  const to = list.indexOf(target);
  const next = [...list];
  if (from < 0 || to < 0 || from === to) return next;
  next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
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
