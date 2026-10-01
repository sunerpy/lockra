import type { EntryView } from "@lockra/shared";

/** The groups the accounts are in, sorted, each once. */
export function entryGroups(entries: readonly EntryView[]): string[] {
  const groups = new Set<string>();
  for (const entry of entries)
    if (entry.group !== null && entry.group !== "") groups.add(entry.group);
  // A sorted copy: `toSorted` is ES2023 (Safari 16), past the ES2022 lib kept for older macOS.
  // oxlint-disable-next-line unicorn/no-array-sort
  return [...groups].sort((a, b) => a.localeCompare(b));
}
