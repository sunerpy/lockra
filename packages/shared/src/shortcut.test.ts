import {
  SUGGESTED_SHORTCUT,
  heldModifiers,
  isModifierKey,
  shortcutFromKey,
  shortcutLabels,
  shortcutText,
} from "./shortcut";

const press = (
  code: string,
  mods: Partial<Record<"ctrl" | "alt" | "shift" | "meta", boolean>>,
) => ({
  code,
  ctrlKey: mods.ctrl ?? false,
  altKey: mods.alt ?? false,
  shiftKey: mods.shift ?? false,
  metaKey: mods.meta ?? false,
});

describe("the quick search's shortcut", () => {
  it("records the physical key with its modifiers, in the shell's accelerator syntax", () => {
    expect(shortcutFromKey(press("Space", { ctrl: true, shift: true }), "linux")).toBe(
      "Control+Shift+Space",
    );
    expect(shortcutFromKey(press("KeyK", { meta: true, alt: true }), "macos")).toBe(
      "Alt+Command+KeyK",
    );
    expect(shortcutFromKey(press("KeyK", { meta: true, ctrl: true }), "windows")).toBe(
      "Control+Super+KeyK",
    );
    expect(shortcutFromKey(press("Digit1", { ctrl: true, alt: true, shift: true }), "macos")).toBe(
      "Control+Alt+Shift+Digit1",
    );
    expect(shortcutFromKey(press("F12", { alt: true }), "windows")).toBe("Alt+F12");
    expect(shortcutFromKey(press("NumpadAdd", { ctrl: true }), "linux")).toBe("Control+NumpadAdd");
    expect(shortcutFromKey(press("ArrowUp", { ctrl: true }), "linux")).toBe("Control+ArrowUp");
  });

  it("refuses a key that would be taken from every app as it is typed, and keys it cannot register", () => {
    // No modifier, or Shift alone: typing would trigger it.
    expect(shortcutFromKey(press("KeyK", {}), "linux")).toBeNull();
    expect(shortcutFromKey(press("KeyK", { shift: true }), "linux")).toBeNull();
    expect(shortcutFromKey(press("F5", {}), "windows")).toBeNull();
    // Keys the shell does not take: Escape (Esc cancels the recording), the lock keys, the
    // modifiers themselves, a key the browser does not know.
    for (const code of [
      "Escape",
      "CapsLock",
      "NumLock",
      "ShiftLeft",
      "MetaRight",
      "Unidentified",
      "",
    ])
      expect(shortcutFromKey(press(code, { ctrl: true }), "linux")).toBeNull();
    expect(isModifierKey("ControlLeft")).toBe(true);
    expect(isModifierKey("AltRight")).toBe(true);
    expect(isModifierKey("OSLeft")).toBe(true);
    expect(isModifierKey("KeyA")).toBe(false);
  });

  it("reads an accelerator as each platform writes its keys", () => {
    expect(shortcutLabels(SUGGESTED_SHORTCUT, "macos")).toEqual(["⌘", "⇧", "Space"]);
    expect(shortcutLabels(SUGGESTED_SHORTCUT, "windows")).toEqual(["Ctrl", "Shift", "Space"]);
    expect(shortcutLabels("Control+Alt+Shift+Command+KeyK", "macos")).toEqual([
      "⌃",
      "⌥",
      "⇧",
      "⌘",
      "K",
    ]);
    expect(shortcutLabels("Control+Super+Digit1", "windows")).toEqual(["Ctrl", "Win", "1"]);
    expect(shortcutLabels("Alt+Super+Numpad7", "linux")).toEqual(["Alt", "Super", "Num 7"]);
    expect(shortcutLabels("Control+NumpadAdd", "linux")).toEqual(["Ctrl", "Num +"]);
    const keys = [
      ["ArrowUp", "↑"],
      ["ArrowDown", "↓"],
      ["ArrowLeft", "←"],
      ["ArrowRight", "→"],
      ["Minus", "-"],
      ["Equal", "="],
      ["BracketLeft", "["],
      ["BracketRight", "]"],
      ["Backslash", "\\"],
      ["Semicolon", ";"],
      ["Quote", "'"],
      ["Backquote", "`"],
      ["Comma", ","],
      ["Period", "."],
      ["Slash", "/"],
      ["F11", "F11"],
      ["PageDown", "PageDown"],
    ] as const;
    for (const [code, label] of keys)
      expect(shortcutLabels(`Control+${code}`, "linux")).toEqual(["Ctrl", label]);
    // Written by hand in settings.json, in the plugin's other spellings: read as well as can be.
    expect(shortcutLabels("ctrl + option + k", "macos")).toEqual(["⌃", "⌥", "K"]);
    expect(shortcutLabels("CmdOrCtrl+Meta+Esc", "linux")).toEqual(["Ctrl", "Super", "Esc"]);
    expect(shortcutLabels("Control++", "linux")).toEqual(["Ctrl"]);
  });

  it("shows the modifiers held while a shortcut is being chosen", () => {
    expect(heldModifiers(press("ShiftLeft", { ctrl: true, shift: true }), "linux")).toEqual([
      "Ctrl",
      "Shift",
    ]);
    expect(heldModifiers(press("MetaLeft", { meta: true, alt: true }), "macos")).toEqual([
      "⌥",
      "⌘",
    ]);
    expect(heldModifiers(press("MetaLeft", { meta: true }), "windows")).toEqual(["Win"]);
    expect(heldModifiers(press("KeyK", {}), "linux")).toEqual([]);
  });

  it("writes a shortcut on one line, for a sentence", () => {
    expect(shortcutText(SUGGESTED_SHORTCUT, "macos")).toBe("⌘⇧Space");
    expect(shortcutText(SUGGESTED_SHORTCUT, "linux")).toBe("Ctrl+Shift+Space");
  });
});
