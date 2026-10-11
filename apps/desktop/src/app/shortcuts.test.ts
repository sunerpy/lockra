import { shortcutFor } from "./shortcuts";

function key(
  k: string,
  mods: Partial<Pick<KeyboardEvent, "ctrlKey" | "metaKey" | "altKey" | "shiftKey">> = {},
  target: EventTarget | null = document.body,
) {
  return {
    key: k,
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    target,
    ...mods,
  };
}

describe("shortcutFor", () => {
  it("maps Ctrl or ⌘ with K N L , F", () => {
    expect(shortcutFor(key("k", { ctrlKey: true }))).toBe("palette");
    expect(shortcutFor(key("K", { metaKey: true }))).toBe("palette");
    expect(shortcutFor(key("n", { ctrlKey: true }))).toBe("add");
    expect(shortcutFor(key("l", { ctrlKey: true }))).toBe("lock");
    expect(shortcutFor(key(",", { metaKey: true }))).toBe("settings");
    expect(shortcutFor(key("f", { ctrlKey: true }))).toBe("search");
    expect(shortcutFor(key("x", { ctrlKey: true }))).toBeUndefined();
  });

  it("leaves Alt and Shift combinations to the system", () => {
    expect(shortcutFor(key("k", { ctrlKey: true, altKey: true }))).toBeUndefined();
    expect(shortcutFor(key("k", { ctrlKey: true, shiftKey: true }))).toBeUndefined();
  });

  it("takes / for search only outside text fields", () => {
    expect(shortcutFor(key("/"))).toBe("search");
    for (const tag of ["input", "textarea", "select"])
      expect(shortcutFor(key("/", {}, document.createElement(tag)))).toBeUndefined();
    const editable = document.createElement("div");
    editable.contentEditable = "true";
    Object.defineProperty(editable, "isContentEditable", { value: true });
    expect(shortcutFor(key("/", {}, editable))).toBeUndefined();
    expect(shortcutFor(key("/", {}, null))).toBe("search");
    expect(shortcutFor(key("a"))).toBeUndefined();
  });
});
