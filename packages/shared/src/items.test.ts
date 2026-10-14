import { ALL_GROUPS, FAVORITES, NO_GROUP, UNGROUPED } from "./entries";
import {
  filterItems,
  hostOf,
  itemAccount,
  itemFilters,
  itemGroups,
  itemHost,
  itemName,
  itemSections,
  itemTags,
  sortItems,
} from "./items";
import { mockItemView } from "./mock-backend";

const github = mockItemView("GitHub", {
  username: "octocat",
  urls: [{ url: "https://github.com/login", matching: "domain" }],
  group: "Work",
  tags: ["dev", "daily"],
  favorite: true,
  created_at_ms: 3,
  updated_at_ms: 9,
  last_used_at_ms: 20,
});
const bank = mockItemView("Bank", {
  email: "me@example.com",
  urls: [
    { url: "^https://bank\\.example/", matching: "regex" },
    { url: "bank.example", matching: "domain" },
  ],
  tags: ["money"],
  created_at_ms: 5,
  updated_at_ms: 1,
});
const router = mockItemView("", {
  username: "admin",
  urls: [{ url: "http://192.168.1.1/", matching: "host" }],
  group: "Home",
  created_at_ms: 1,
  updated_at_ms: 5,
  last_used_at_ms: 30,
});

describe("the password list", () => {
  it("names a login by its title, then its user, then its site", () => {
    expect([
      itemName(github),
      itemName(router),
      itemName(mockItemView("", { urls: [{ url: "www.shop.example", matching: "domain" }] })),
    ]).toEqual(["GitHub", "admin", "shop.example"]);
    expect([itemAccount(github), itemAccount(bank)]).toEqual(["octocat", "me@example.com"]);
    expect([itemHost(github), itemHost(bank), itemHost(router)]).toEqual([
      "github.com",
      "bank.example",
      "192.168.1.1",
    ]);
    expect([hostOf(""), hostOf("not a url ::"), hostOf("example.com:8443/x")]).toEqual([
      null,
      null,
      "example.com",
    ]);
  });

  it("sorts favourites first, then by the chosen order", () => {
    const all = [bank, router, github];
    expect(sortItems(all, "name").map(itemName)).toEqual(["GitHub", "admin", "Bank"]);
    expect(sortItems(all, "recent").map(itemName)).toEqual(["GitHub", "admin", "Bank"]);
    expect(sortItems(all, "modified").map(itemName)).toEqual(["GitHub", "admin", "Bank"]);
    expect(sortItems(all, "added").map(itemName)).toEqual(["GitHub", "Bank", "admin"]);
  });

  it("searches titles, users, sites, tags and groups, within a filter and tags", () => {
    const all = [github, bank, router];
    expect(filterItems(all, "octo", ALL_GROUPS).map(itemName)).toEqual(["GitHub"]);
    expect(filterItems(all, "BANK.EXAMPLE", ALL_GROUPS).map(itemName)).toEqual(["Bank"]);
    expect(filterItems(all, "money", ALL_GROUPS).map(itemName)).toEqual(["Bank"]);
    expect(filterItems(all, "home", ALL_GROUPS).map(itemName)).toEqual(["admin"]);
    expect(filterItems(all, "", FAVORITES).map(itemName)).toEqual(["GitHub"]);
    expect(filterItems(all, "", UNGROUPED).map(itemName)).toEqual(["Bank"]);
    expect(filterItems(all, "", "Work").map(itemName)).toEqual(["GitHub"]);
    expect(filterItems(all, "", ALL_GROUPS, ["dev", "daily"]).map(itemName)).toEqual(["GitHub"]);
    expect(filterItems(all, "", ALL_GROUPS, ["dev", "money"])).toEqual([]);
    expect(filterItems(all, "  ", ALL_GROUPS)).toHaveLength(3);
  });

  it("offers a filter for everything, the favourites, each group and the rest", () => {
    expect(itemFilters([github, bank, router])).toEqual([
      { value: ALL_GROUPS, kind: "all", count: 3 },
      { value: FAVORITES, kind: "favorites", count: 1 },
      { value: "Home", kind: "group", count: 1 },
      { value: "Work", kind: "group", count: 1 },
      { value: UNGROUPED, kind: "ungrouped", count: 1 },
    ]);
    expect(itemFilters([bank])).toEqual([{ value: ALL_GROUPS, kind: "all", count: 1 }]);
    expect(itemGroups([github, bank, router])).toEqual(["Home", "Work"]);
    expect(itemTags([github, bank, mockItemView("x", { tags: ["dev"] })])).toEqual([
      { tag: "daily", count: 1 },
      { tag: "dev", count: 2 },
      { tag: "money", count: 1 },
    ]);
  });

  it("puts the logins in sections by group, those in none last", () => {
    const sections = itemSections([bank, router, github]);
    expect(sections.map((s) => [s.key, s.items.map(itemName)])).toEqual([
      ["Home", ["admin"]],
      ["Work", ["GitHub"]],
      [NO_GROUP, ["Bank"]],
    ]);
  });
});
