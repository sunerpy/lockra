// The user name a form shows as text where it has no field for it: the password step of a sign-in
// that remembers who signs in (AWS IAM Identity Center, measured 2026-10-22: the user name in a
// <p class="username"> above the password, beside a 「不是您?」 button, no user name field, and an
// empty hidden one outside the form), or a form that sets the password of a named account. It
// becomes the user name of the save prompt and of a generated login, and only where the form has
// no user name field and none hidden (formUsername).
import { type FormScan, type View, words } from "./fields";

/** Elements whose text is a control's, a link's or a field's label: never a name the page shows. */
const NOT_SHOWN = [
  "input",
  "textarea",
  "select",
  "option",
  "button",
  "a",
  "label",
  '[role="button"]',
  '[role="link"]',
  '[role="textbox"]',
  "script",
  "style",
].join(", ");

/** The attributes that may name a user name. */
const NAMING = ["id", "class", "name", "data-testid"] as const;

/** The elements whose names hold a part of a user name's names in any case, as the browser finds
 *  them; their words decide (namesUser). */
const NAMED = NAMING.flatMap((attribute) =>
  ["user", "login", "mail", "identifier"].map((part) => `[${attribute}*="${part}" i]`),
).join(", ");

/** A user name's names, as single words and as words that go together (`userName`, `user-id`,
 *  `e-mail`); an account, a name or a user alone names none. */
const NAME_WORDS = new Set(["username", "userid", "loginname", "loginid", "email", "identifier"]);
const NAME_PAIRS: readonly (readonly [string, string])[] = [
  ["user", "name"],
  ["user", "id"],
  ["login", "name"],
  ["login", "id"],
  ["e", "mail"],
];

/** A field's label, with its colon or without: what a page writes before a user name. */
const LABEL = /^(?:用户名|账号|帐号|邮箱|手机号|username|user name|e-?mail|login)[:：]?$/i;
/** What a page masks or cuts short (`a***@example.com`, `138****1234`) is not the name itself. */
const MASKED = /[*•●…]/;
/** The longest text read as a user name. */
const NAME_MAX = 100;

function namesUser(el: Element): boolean {
  return NAMING.some((attribute) => {
    const all = words(el.getAttribute(attribute) ?? "");
    return all.some(
      (word, index) =>
        NAME_WORDS.has(word) ||
        NAME_PAIRS.some(([first, next]) => word === first && all[index + 1] === next),
    );
  });
}

/** Whether `el` is a control, a link or a label of `root`, or inside one. */
function inControl(el: Element, root: HTMLElement): boolean {
  const control = el.closest(NOT_SHOWN);
  return control !== null && root.contains(control);
}

/** Whether `el` holds text outside the controls, links and labels in it. */
function textOutsideControls(el: Element): boolean {
  const walker = el.ownerDocument.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    const control = node.parentElement?.closest(NOT_SHOWN);
    if ((node.textContent ?? "").trim() !== "" && !(control && el.contains(control))) return true;
  }
  return false;
}

/** The element's own text: its text nodes, with a space for each element between them (a button in
 *  it says nothing of the name). An element in it that holds text of its own (not a control's)
 *  splits the name: nothing is read then, not even the part left whole (`<b>alice</b>@example.com`).
 *  A comment counts for nothing: frameworks put one between the parts of one text
 *  (`@<!-- -->alice`). */
function ownText(el: Element): string {
  let text = "";
  for (const node of Array.from(el.childNodes)) {
    if (node instanceof Text) text += node.data;
    else if (node instanceof Element) {
      if (!node.matches(NOT_SHOWN) && textOutsideControls(node)) return "";
      text += " ";
    }
  }
  return text.trim();
}

/** A name alone: 1 to 100 characters without a space, a letter or a digit among them, neither a
 *  label's word nor a text that ends with a colon, and not masked. */
function nameAlone(text: string): boolean {
  return (
    text.length > 0 &&
    text.length <= NAME_MAX &&
    !/\s/.test(text) &&
    /[\p{L}\p{N}]/u.test(text) &&
    !LABEL.test(text) &&
    !/[:：]$/.test(text) &&
    !MASKED.test(text)
  );
}

/** The user name `root` shows as text: the own text of its visible elements that are named for a
 *  user name and hold a name alone, outside its controls and links. Two that differ say nothing
 *  (""); the same name twice is one. */
export function shownUsername(root: HTMLElement, view: View): string {
  const names = new Set<string>();
  for (const el of Array.from(root.querySelectorAll(NAMED))) {
    if (!(el instanceof HTMLElement) || !namesUser(el) || inControl(el, root)) continue;
    const text = ownText(el);
    if (nameAlone(text) && view.visible(el)) names.add(text);
  }
  const [name, ...others] = names;
  return name !== undefined && others.length === 0 ? name : "";
}

/** The user name a sign-in or sign-up form gives: what its user name field holds, empty too (the
 *  page wants it typed), else the one the page kept in a hidden field, else the one it shows. */
export function formUsername(scan: FormScan, view: View): string {
  if (scan.username) return scan.username.value;
  return scan.hiddenUsername?.value ?? shownUsername(scan.root, view);
}
