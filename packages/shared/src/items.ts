// The password list's logic, shared by the desktop and the phone: the order, the search, the
// filters (全部, 收藏, each group, 未分组) and the tags beside them, the sections of groups, and what a
// login shows of its sites.
import { ALL_GROUPS, FAVORITES, NO_GROUP, UNGROUPED } from "./entries";
import type { ItemSort, ItemView } from "./schema";

/** The name a login goes by: its title, else its user name or email, else its first site. */
export function itemName(item: ItemView): string {
  return item.title || item.username || item.email || (itemHost(item) ?? "");
}

/** Who a login signs in as: the user name, else the email address. */
export function itemAccount(item: ItemView): string {
  return item.username || item.email;
}

/** The host of a site saved as `url` (`https://login.example.com/x` → `login.example.com`), or
 *  `null` for text that names none. */
export function hostOf(url: string): string | null {
  const text = url.trim();
  if (text === "") return null;
  const withScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(text) ? text : `https://${text}`;
  try {
    const host = new URL(withScheme).hostname;
    return host === "" ? null : host.replace(/^www\./, "");
  } catch {
    return null;
  }
}

/** The host of a login's first site (a regular expression names no host), for the list. */
export function itemHost(item: ItemView): string | null {
  for (const rule of item.urls) {
    if (rule.matching === "regex") continue;
    const host = hostOf(rule.url);
    if (host !== null) return host;
  }
  return null;
}

function nameKey(item: ItemView): string {
  return itemName(item).toLocaleLowerCase();
}

function byName(a: ItemView, b: ItemView): number {
  return nameKey(a).localeCompare(nameKey(b)) || itemAccount(a).localeCompare(itemAccount(b));
}

/** Favourites first, then `order`; ties by name. */
export function sortItems(items: readonly ItemView[], order: ItemSort): ItemView[] {
  const key: (a: ItemView, b: ItemView) => number =
    order === "recent"
      ? (a, b) => (b.last_used_at_ms ?? 0) - (a.last_used_at_ms ?? 0)
      : order === "modified"
        ? (a, b) => b.updated_at_ms - a.updated_at_ms
        : order === "added"
          ? (a, b) => b.created_at_ms - a.created_at_ms
          : byName;
  // A sorted copy: `toSorted` is ES2023 (Safari 16), past the ES2022 lib kept for older macOS.
  // oxlint-disable-next-line unicorn/no-array-sort
  return [...items].sort(
    (a, b) => Number(b.favorite) - Number(a.favorite) || key(a, b) || byName(a, b),
  );
}

/** The logins `query` finds (in the title, the user name, the email, the sites, the tags or the
 *  group), in `group` (every one for {@link ALL_GROUPS}, the favourites for {@link FAVORITES},
 *  those in no group for {@link UNGROUPED}), carrying every tag of `tags`. */
export function filterItems(
  items: readonly ItemView[],
  query: string,
  group: string,
  tags: readonly string[] = [],
): ItemView[] {
  const q = query.trim().toLocaleLowerCase();
  const inGroup = (item: ItemView) =>
    group === ALL_GROUPS ||
    (group === FAVORITES
      ? item.favorite
      : group === UNGROUPED
        ? (item.group ?? "") === ""
        : item.group === group);
  const tagged = (item: ItemView) => tags.every((tag) => item.tags.includes(tag));
  return items.filter(
    (item) => inGroup(item) && tagged(item) && (q === "" || searchText(item).includes(q)),
  );
}

/** What a search looks through: the title, the user name, the email, the group, the sites, the
 *  tags. */
function searchText(item: ItemView): string {
  return [
    item.title,
    item.username,
    item.email,
    item.group ?? "",
    ...item.urls.map((u) => u.url),
    ...item.tags,
  ]
    .join("\n")
    .toLocaleLowerCase();
}

/** The groups the logins are in, sorted, each once. */
export function itemGroups(items: readonly ItemView[]): string[] {
  const groups = new Set<string>();
  for (const item of items) if (item.group !== null && item.group !== "") groups.add(item.group);
  // oxlint-disable-next-line unicorn/no-array-sort
  return [...groups].sort((a, b) => a.localeCompare(b));
}

/** One of the password list's filters, with its number of logins. */
export interface ItemFilter {
  value: string;
  kind: "all" | "favorites" | "group" | "ungrouped";
  count: number;
}

/** The password list's filters: 全部, 收藏 (when a login is a favourite), each group by name, and
 *  未分组 (when some logins are in no group beside groups). */
export function itemFilters(items: readonly ItemView[]): ItemFilter[] {
  const groups = itemGroups(items);
  const count = (group: string) => items.filter((i) => i.group === group).length;
  const favorites = items.filter((i) => i.favorite).length;
  const loose = items.filter((i) => (i.group ?? "") === "").length;
  return [
    { value: ALL_GROUPS, kind: "all", count: items.length },
    ...(favorites > 0 ? [{ value: FAVORITES, kind: "favorites" as const, count: favorites }] : []),
    ...groups.map((g) => ({ value: g, kind: "group" as const, count: count(g) })),
    ...(groups.length > 0 && loose > 0
      ? [{ value: UNGROUPED, kind: "ungrouped" as const, count: loose }]
      : []),
  ];
}

/** A tag and how many logins carry it. */
export interface TagCount {
  tag: string;
  count: number;
}

/** Every tag the logins carry, by name, with its count. */
export function itemTags(items: readonly ItemView[]): TagCount[] {
  const counts = new Map<string, number>();
  for (const item of items)
    for (const tag of item.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);
  const tags = [...counts.entries()].map(([tag, count]) => ({ tag, count }));
  // A sorted copy: `toSorted` is ES2023 (Safari 16), past the ES2022 lib kept for older macOS.
  // oxlint-disable-next-line unicorn/no-array-sort
  return tags.sort((a, b) => a.tag.localeCompare(b.tag));
}

/** One section of the password list: a group's logins, or those in no group (`key` ""). */
export interface ItemSection {
  key: string;
  items: ItemView[];
}

/** The logins in sections, each keeping the order it is given: the groups by name, then the logins
 *  in no group. */
export function itemSections(items: readonly ItemView[]): ItemSection[] {
  const byGroup = new Map<string, ItemView[]>();
  for (const item of items) {
    const key = item.group ?? NO_GROUP;
    const section = byGroup.get(key);
    if (section) section.push(item);
    else byGroup.set(key, [item]);
  }
  // oxlint-disable-next-line unicorn/no-array-sort
  const keys = [...byGroup.keys()].sort(
    (a, b) => Number(a === NO_GROUP) - Number(b === NO_GROUP) || a.localeCompare(b),
  );
  return keys.map((key) => ({ key, items: byGroup.get(key) ?? [] }));
}
