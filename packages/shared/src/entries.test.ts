import { describe, expect, it } from "vitest";
import { cutMark, entryGroups, filterEntries, groupSections, sortEntries } from "./entries";
import { mockEntry } from "./mock-backend";

describe("sortEntries / filterEntries", () => {
  const entries = [
    mockEntry("beta", "b", { at: 3, last_used_at_ms: 10 }),
    mockEntry("Alpha", "a", { at: 1 }),
    mockEntry("", "carol", { at: 2, favorite: true, group: "home", last_used_at_ms: 20 }),
  ].map((e) => e.view);

  it("puts favourites first, then the chosen order", () => {
    expect(sortEntries(entries, "name").map((e) => e.issuer || e.account)).toEqual([
      "carol",
      "Alpha",
      "beta",
    ]);
    expect(sortEntries(entries, "added").map((e) => e.issuer || e.account)).toEqual([
      "carol",
      "beta",
      "Alpha",
    ]);
    expect(sortEntries(entries, "recent").map((e) => e.issuer || e.account)).toEqual([
      "carol",
      "beta",
      "Alpha",
    ]);
  });

  it("matches issuer, account and group, case-insensitively", () => {
    expect(filterEntries(entries, "ALP", "")).toHaveLength(1);
    expect(filterEntries(entries, "home", "")).toHaveLength(1);
    expect(filterEntries(entries, "", "home")).toHaveLength(1);
    expect(filterEntries(entries, "  ", "")).toHaveLength(3);
  });
});

describe("groupSections / entryGroups", () => {
  const entries = [
    mockEntry("b", "1", { group: "Work" }),
    mockEntry("a", "2"),
    mockEntry("c", "3", { group: "Home" }),
    mockEntry("d", "4", { group: "Work" }),
  ].map((e) => e.view);

  it("makes a section per group by name, the accounts in no group last, each in its order", () => {
    expect(groupSections(entries).map((s) => [s.key, s.entries.map((e) => e.issuer)])).toEqual([
      ["Home", ["c"]],
      ["Work", ["b", "d"]],
      ["", ["a"]],
    ]);
    expect(groupSections([])).toEqual([]);
  });

  it("lists the groups in use, once each, sorted", () => {
    expect(entryGroups(entries)).toEqual(["Home", "Work"]);
  });
});

describe("cutMark", () => {
  it("keeps two characters as people count them", () => {
    expect(cutMark("GitHub")).toBe("Gi");
    expect(cutMark("👨‍💻🚀x")).toBe("👨‍💻🚀");
    expect(cutMark("")).toBe("");
  });
});
