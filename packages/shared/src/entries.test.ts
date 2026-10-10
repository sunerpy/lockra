import { describe, expect, it } from "vitest";
import {
  cutMark,
  entryGroups,
  exportBlocker,
  filterEntries,
  groupSections,
  moveItem,
  parseKind,
  sortEntries,
} from "./entries";
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

  it("keeps a dragged order, favourites still first, accounts never dragged after it, oldest first", () => {
    const [beta, alpha, carol] = entries.map((e) => e.id);
    const names = (order: readonly string[]) =>
      sortEntries(entries, "manual", order).map((e) => e.issuer || e.account);
    expect(names([alpha ?? "", beta ?? "", carol ?? ""])).toEqual(["carol", "Alpha", "beta"]);
    expect(names([beta ?? ""])).toEqual(["carol", "beta", "Alpha"]);
    expect(names([])).toEqual(["carol", "Alpha", "beta"]);
    // Another order ignores the dragged one.
    expect(sortEntries(entries, "name", [beta ?? ""]).map((e) => e.issuer || e.account)).toEqual([
      "carol",
      "Alpha",
      "beta",
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

  it("puts dragged groups first in their order, the others after by name, no group last", () => {
    const keys = (order: readonly string[]) => groupSections(entries, order).map((s) => s.key);
    expect(keys(["Work"])).toEqual(["Work", "Home", ""]);
    expect(keys(["Work", "Home", "Gone"])).toEqual(["Work", "Home", ""]);
    expect(keys([])).toEqual(["Home", "Work", ""]);
  });

  it("moves one item to another place", () => {
    expect(moveItem(["a", "b", "c", "d"], "a", "c")).toEqual(["b", "c", "a", "d"]);
    expect(moveItem(["a", "b", "c", "d"], "d", "b")).toEqual(["a", "d", "b", "c"]);
    expect(moveItem(["a", "b"], "a", "a")).toEqual(["a", "b"]);
    expect(moveItem(["a", "b"], "x", "a")).toEqual(["a", "b"]);
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

describe("parseKind", () => {
  it("accepts the core's ranges only", () => {
    expect(parseKind("totp", "30", "0")).toEqual({ type: "totp", period: 30 });
    expect(parseKind("totp", "0", "0")).toBeUndefined();
    expect(parseKind("totp", "3601", "0")).toBeUndefined();
    expect(parseKind("totp", "1.5", "0")).toBeUndefined();
    expect(parseKind("hotp", "30", "7")).toEqual({ type: "hotp", counter: 7 });
    expect(parseKind("hotp", "30", "")).toBeUndefined();
    expect(parseKind("hotp", "30", "-1")).toBeUndefined();
  });
});

describe("exportBlocker", () => {
  it("takes every account into a file, and the core's reason for the others", () => {
    const entry = mockEntry("Bank", "card", {}).view;
    const blocked = {
      ...entry,
      export: { google: "hotp_not_supported" as const, microsoft: null },
    };
    expect(exportBlocker(blocked, "google")).toBe("hotp_not_supported");
    expect(exportBlocker(blocked, "microsoft")).toBeNull();
    expect(exportBlocker(blocked, "file")).toBeNull();
  });
});
