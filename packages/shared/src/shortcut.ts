// The quick search's global shortcut (Settings › General), as the desktop shell registers it
// (tauri-plugin-global-shortcut, global-hotkey): modifiers, then one key, joined by `+`. The key
// is the physical key's `KeyboardEvent.code` (`KeyK`, `Digit1`, `Space`), so the keyboard layout
// does not change what is registered.
import type { Platform } from "./schema";

/** The shortcut Settings suggests: Ctrl+Shift+Space, ⌘⇧Space on a Mac. */
export const SUGGESTED_SHORTCUT = "CommandOrControl+Shift+Space";

/** The keys a shortcut may end on: letters, digits, F1–F24, the keypad's, Space, Enter, Tab, the
 *  editing and arrow keys, and the punctuation keys. Escape cancels the recording; the lock keys
 *  and the modifiers end nothing. */
const KEY =
  /^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-9]|2[0-4])|Numpad([0-9]|Add|Subtract|Multiply|Divide|Decimal)|Space|Enter|Tab|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Arrow(Up|Down|Left|Right)|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Backquote|Comma|Period|Slash)$/;

const MODIFIER = /^(Control|Shift|Alt|Meta|OS)(Left|Right)$/;

/** What `shortcutFromKey` reads of a key press. */
export type ShortcutKeyPress = Pick<
  KeyboardEvent,
  "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
>;

/** The key pressed is a modifier: the shortcut is still being chosen. */
export function isModifierKey(code: string): boolean {
  return MODIFIER.test(code);
}

/** The modifiers of a key press as accelerator tokens, in the order the shell's syntax writes them:
 *  ⌃ ⌥ ⇧ ⌘ (Ctrl Alt Shift Win). The ⌘ / Windows key is `Command` on a Mac, `Super` elsewhere. */
function modifierTokens(press: Omit<ShortcutKeyPress, "code">, platform: Platform): string[] {
  const tokens: string[] = [];
  if (press.ctrlKey) tokens.push("Control");
  if (press.altKey) tokens.push("Alt");
  if (press.shiftKey) tokens.push("Shift");
  if (press.metaKey) tokens.push(platform === "macos" ? "Command" : "Super");
  return tokens;
}

/** The accelerator a key press records, or `null`: a key the shell does not take, or one without
 *  Ctrl, Alt or ⌘ (alone, or with Shift alone, it would be taken from every app as it is typed). */
export function shortcutFromKey(press: ShortcutKeyPress, platform: Platform): string | null {
  if (!KEY.test(press.code) || !(press.ctrlKey || press.altKey || press.metaKey)) return null;
  return [...modifierTokens(press, platform), press.code].join("+");
}

/** The modifiers held in a key press, as keycaps: what the recorder shows while a shortcut is being
 *  chosen. */
export function heldModifiers(press: Omit<ShortcutKeyPress, "code">, platform: Platform): string[] {
  return shortcutLabels(modifierTokens(press, platform).join("+"), platform);
}

const KEY_LABELS: Readonly<Record<string, string>> = {
  arrowup: "↑",
  arrowdown: "↓",
  arrowleft: "←",
  arrowright: "→",
  minus: "-",
  equal: "=",
  bracketleft: "[",
  bracketright: "]",
  backslash: "\\",
  semicolon: ";",
  quote: "'",
  backquote: "`",
  comma: ",",
  period: ".",
  slash: "/",
  escape: "Esc",
  numpadadd: "Num +",
  numpadsubtract: "Num -",
  numpadmultiply: "Num *",
  numpaddivide: "Num /",
  numpaddecimal: "Num .",
};

function modifierLabel(token: string, platform: Platform): string | undefined {
  const mac = platform === "macos";
  switch (token) {
    case "commandorcontrol":
    case "commandorctrl":
    case "cmdorctrl":
    case "cmdorcontrol":
      return mac ? "⌘" : "Ctrl";
    case "control":
    case "ctrl":
      return mac ? "⌃" : "Ctrl";
    case "alt":
    case "option":
      return mac ? "⌥" : "Alt";
    case "shift":
      return mac ? "⇧" : "Shift";
    case "command":
    case "cmd":
    case "super":
    case "meta":
      return mac ? "⌘" : platform === "windows" ? "Win" : "Super";
    default:
      return undefined;
  }
}

function keyLabel(token: string): string {
  const lower = token.toLowerCase();
  const named = KEY_LABELS[lower];
  if (named !== undefined) return named;
  const letter = /^key([a-z])$/.exec(lower);
  if (letter?.[1] !== undefined) return letter[1].toUpperCase();
  const digit = /^digit([0-9])$/.exec(lower);
  if (digit?.[1] !== undefined) return digit[1];
  const pad = /^numpad([0-9])$/.exec(lower);
  if (pad?.[1] !== undefined) return `Num ${pad[1]}`;
  return token.length === 1 ? token.toUpperCase() : token;
}

/** The keycaps of an accelerator, in the platform's words: `CommandOrControl` is ⌘ on a Mac and
 *  Ctrl elsewhere, `Super` is Win on Windows; a key reads as it is printed (`K`, `1`, `↑`). A
 *  shortcut written by hand in settings.json reads as well as it can. */
export function shortcutLabels(accelerator: string, platform: Platform): string[] {
  return accelerator
    .split("+")
    .map((token) => token.trim())
    .filter((token) => token.length > 0)
    .map((token) => modifierLabel(token.toLowerCase(), platform) ?? keyLabel(token));
}

/** The shortcut on one line, for a sentence: `⌘⇧Space` on a Mac, `Ctrl+Shift+Space` elsewhere. */
export function shortcutText(accelerator: string, platform: Platform): string {
  return shortcutLabels(accelerator, platform).join(platform === "macos" ? "" : "+");
}
