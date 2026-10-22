// Which fields a form has (plan §四): the user name and password of a sign-in, the first step of a
// sign-in in two (a lone user name field), sign-up forms (`autocomplete="new-password"`, a
// confirmation field, `passwordrules`), and one-time-code fields: `autocomplete="one-time-code"`,
// then a field named like a code with a code's length or a numeric keyboard, then one box per digit.
// Only what the page shows counts (`View`); the content script reads no address and decides nothing
// about what to fill: that is the app's, on the browser's facts.

/** What the page shows. The real one measures (visibility.ts); the tests' says it from markup. */
export interface View {
  visible(el: Element): boolean;
}

export type FormKind = "login" | "signup" | "username" | "otp" | "none";

export interface FormScan {
  kind: FormKind;
  /** The form, or the smallest box around a formless sign-in. */
  root: HTMLElement;
  /** The visible user name field. */
  username: HTMLInputElement | null;
  /** A user name the page kept in a hidden field (the second step of some sign-ins): read only. */
  hiddenUsername: HTMLInputElement | null;
  /** The visible password fields, in order. */
  passwords: HTMLInputElement[];
  /** Where a new password goes (sign-up, change of password). */
  newPasswords: HTMLInputElement[];
  /** The one-time-code field, or its boxes. */
  otp: HTMLInputElement[];
  /** The page's own password rules, for the generator. */
  rules: string | null;
  minlength: number | null;
  maxlength: number | null;
}

const FILLABLE = new Set(["", "text", "email", "tel", "password", "number", "url", "search"]);
const USER_HINT =
  /user|login|e-?mail|account|acct|identifier|phone|mobile|账号|帐号|用户|邮箱|手机/i;
const OTP_HINT =
  /one.?time|otp|totp|2fa|mfa|two.?factor|verif|security.?code|auth.?code|passcode|code|验证码|校验码|动态码|安全码/i;
const NOT_USER = /search|captcha|coupon|promo|otp|code|verif/i;
/** The authenticator's own words: such a field takes its code, whatever is beside it. */
const AUTHENTICATOR_HINT =
  /totp|authenticator|google.?auth|身份验证器|验证器应用|动态口令|令牌|mfa|2fa|two.?factor/i;
/** An image check's words, and a code the site sends by text message or mail. */
const CAPTCHA_HINT = /captcha|kaptcha|图形|图片|图像|看不清|换一张|人机/i;
const SENT_HINT = /sms|短信|手机验证码|text.?message|邮箱|邮件|e-?mail/i;
/** An image check's picture, by its address or its names. */
const CAPTCHA_IMAGE = /captcha|kaptcha|verif|vcode|validate|checkcode|piccode|imgcode|验证码/i;
/** A control beside the field that sends a code. */
const SEND_CODE = /获取|发送|重新发送|重发|send|resend|get.?code/i;

function typeOf(input: HTMLInputElement): string {
  return (input.getAttribute("type") ?? "").toLowerCase();
}

function autocomplete(input: HTMLInputElement): string {
  return (input.getAttribute("autocomplete") ?? "").toLowerCase();
}

/** The most of a label's text that names a field; longer labels are read only this far. */
const LABEL_TEXT_MAX = 100;

/** What the page shows as the field's name: its `<label>`s (by `for`, or around it) and the
 *  elements `aria-labelledby` names. AWS IAM Identity Center's user name field has no name and an
 *  empty placeholder: only its `<label for>` says 用户名. */
function labelText(input: HTMLInputElement): string {
  const doc = input.ownerDocument;
  const named = (input.getAttribute("aria-labelledby") ?? "")
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => doc.getElementById(id));
  return [...Array.from(input.labels ?? []), ...named]
    .map((el) => el?.textContent?.trim() ?? "")
    .filter(Boolean)
    .join(" ")
    .slice(0, LABEL_TEXT_MAX);
}

function hints(input: HTMLInputElement): string {
  return [
    input.id,
    input.name,
    input.getAttribute("placeholder"),
    input.getAttribute("aria-label"),
    autocomplete(input),
    labelText(input),
  ]
    .filter(Boolean)
    .join(" ");
}

/** A text-like input that is enabled and writable. */
export function isFillable(el: unknown): el is HTMLInputElement {
  return el instanceof HTMLInputElement && !el.disabled && !el.readOnly && FILLABLE.has(typeOf(el));
}

export function isPassword(input: HTMLInputElement): boolean {
  const type = typeOf(input);
  // A password some sites show in clear while it is typed keeps its autocomplete token.
  return (
    type === "password" || (type === "text" && /^(current|new)-password$/.test(autocomplete(input)))
  );
}

function lengthOf(input: HTMLInputElement, attribute: "minlength" | "maxlength"): number | null {
  const value = Number.parseInt(input.getAttribute(attribute) ?? "", 10);
  return Number.isFinite(value) && value >= 0 ? value : null;
}

/** A single field that takes a whole one-time code. */
function looksOtp(input: HTMLInputElement, view: View): boolean {
  if (!isFillable(input)) return false;
  if (isPassword(input)) return maskedCode(input, view) && !anotherCode(input);
  return codeField(input) && !anotherCode(input);
}

/** A code's names, as single words, and as words that go together (`dynamicPassword`,
 *  `one_time_code`), in Chinese as it is written. */
const CODE_WORDS = new Set(["otp", "totp", "hotp", "mfa", "2fa", "tfa", "passcode", "otpcode"]);
const CODE_PAIRS: readonly (readonly [string, readonly string[]])[] = [
  ["dynamic", ["password", "pass", "pwd", "code", "token"]],
  ["one", ["time"]],
  ["two", ["factor"]],
  ["verification", ["code"]],
  ["security", ["code"]],
  ["auth", ["code"]],
];
const CODE_TEXT = /动态密码|动态口令|动态码|一次性密码|令牌|验证码|身份验证器/;

/** The words of a name: `dynamicPassword` → dynamic, password; `one_time-code` → one, time, code. */
export function words(text: string): string[] {
  return text
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
}

function namesCode(text: string): boolean {
  if (CODE_TEXT.test(text)) return true;
  const all = words(text);
  return all.some(
    (word, index) =>
      CODE_WORDS.has(word) ||
      CODE_PAIRS.some(([first, next]) => word === first && next.includes(all[index + 1] ?? "")),
  );
}

/** A password-type field that takes a one-time code: some pages mask the code as they would a
 *  password (NDKEY's ADFS adapter: `type="password" name="OTP"`; NWCD's SSO: "MFA 令牌" after the
 *  password). The page's token says so, or the field's own names do in so many words while it is
 *  the only field of its form to type in, or comes after the sign-in's password: a sign-in's
 *  password field, beside its user name or named a password by the page, never reads as one. */
function maskedCode(input: HTMLInputElement, view: View): boolean {
  const token = autocomplete(input);
  if (token === "one-time-code") return true;
  if (typeOf(input) !== "password" || /password/.test(token)) return false;
  const own = [
    input.id,
    input.name,
    input.getAttribute("aria-label"),
    input.getAttribute("placeholder"),
    labelText(input),
  ];
  if (!own.some((text) => text && namesCode(text))) return false;
  return standsAlone(input, view) || afterPassword(input, view);
}

/** Another visible password field of its form comes before it: the sign-in's password. */
function afterPassword(input: HTMLInputElement, view: View): boolean {
  return Array.from(formOf(input).querySelectorAll("input")).some(
    (other) =>
      other !== input &&
      isFillable(other) &&
      isPassword(other) &&
      view.visible(other) &&
      other.form === input.form &&
      before(other, input),
  );
}

/** The only field of its form to type in: every other input of the form is the field itself, one
 *  that takes nothing, one out of sight, or one of another form. */
function standsAlone(input: HTMLInputElement, view: View): boolean {
  const root = formOf(input);
  return Array.from(root.querySelectorAll("input")).every(
    (other) =>
      other === input ||
      !isFillable(other) ||
      !view.visible(other) ||
      (input.form !== null && other.form !== input.form),
  );
}

/** Whether a code the user asked for may go into `input`, focused where Lockra tells no code
 *  field: any field but a password field, and that one only alone in its form and named no
 *  password by the page (a code step Lockra cannot tell from a password step, never a sign-in's
 *  password field). */
export function takesAskedCode(input: HTMLInputElement, view: View): boolean {
  if (!isPassword(input)) return true;
  return !/password/.test(autocomplete(input)) && standsAlone(input, view);
}

/** A field named as a code's takes one when it says how: a code's length, a numeric keyboard, a
 *  pattern of digits alone, or the authenticator's own words (Google's `totpPin` has neither a
 *  length nor a keyboard, only `pattern="[0-9 ]*"`). */
function codeField(input: HTMLInputElement): boolean {
  if (autocomplete(input) === "one-time-code") return true;
  if (!["", "text", "tel", "number"].includes(typeOf(input))) return false;
  const own = hints(input);
  if (!OTP_HINT.test(own)) return false;
  const max = lengthOf(input, "maxlength");
  return (
    (max !== null && max >= 4 && max <= 8) ||
    input.getAttribute("inputmode")?.toLowerCase() === "numeric" ||
    digitsOnly(input) ||
    AUTHENTICATOR_HINT.test(own)
  );
}

/** A `pattern` that takes digits alone, and the spaces some codes are typed with: `[0-9 ]*`, `\d{6}`. */
function digitsOnly(input: HTMLInputElement): boolean {
  const pattern = input.getAttribute("pattern") ?? "";
  if (!/0-9|\\d/.test(pattern)) return false;
  return pattern.replace(/0-9|\\d/g, "").replace(/[[\]\s{}(),*+?^$\d|]/g, "") === "";
}

/** A code field that is not the authenticator's: an image check, or a code the site sends by text
 *  message or mail. Its own words say so, or what is beside it: the check's picture (an image of
 *  the check's, a canvas), a button that sends a code. A field that names the authenticator is
 *  its, whatever is beside it. */
function anotherCode(input: HTMLInputElement): boolean {
  const own = hints(input);
  if (AUTHENTICATOR_HINT.test(own)) return false;
  if (CAPTCHA_HINT.test(own) || SENT_HINT.test(own)) return true;
  const around = neighbourhood(input);
  if (!around) return false;
  if (around.querySelector("canvas")) return true;
  const picture = Array.from(around.querySelectorAll("img")).some((img) =>
    CAPTCHA_IMAGE.test(
      [img.getAttribute("src"), img.alt, img.title, img.id, img.getAttribute("class")].join(" "),
    ),
  );
  if (picture) return true;
  return Array.from(
    around.querySelectorAll('button, a, [role="button"], input[type="button" i]'),
  ).some((control) =>
    SEND_CODE.test(
      `${control.textContent ?? ""} ${control.getAttribute("value") ?? ""} ${control.getAttribute("aria-label") ?? ""}`,
    ),
  );
}

/** The box a field sits in with its neighbours: its parent, or the one above when the parent holds
 *  the field alone; never a whole form or the page. */
function neighbourhood(input: HTMLInputElement): Element | null {
  let box = input.parentElement;
  if (box && box.children.length === 1) box = box.parentElement;
  const doc = input.ownerDocument;
  if (!box || box instanceof HTMLFormElement || box === doc.body || box === doc.documentElement)
    return null;
  return box;
}

function isBox(input: HTMLInputElement, view: View): boolean {
  return (
    isFillable(input) &&
    !isPassword(input) &&
    lengthOf(input, "maxlength") === 1 &&
    view.visible(input)
  );
}

/** The boxes of a code split one digit per box (4–8 boxes at the same depth under a container
 *  at most three levels up), when `field` is one of them. */
export function otpGroup(field: HTMLInputElement, view: View): HTMLInputElement[] | null {
  if (!isBox(field, view)) return null;
  let container: Element | null = field;
  for (let depth = 1; depth <= 3; depth++) {
    container = container?.parentElement ?? null;
    if (!container) return null;
    const around = container;
    const boxes = Array.from(around.querySelectorAll("input")).filter(
      (input) => isBox(input, view) && depthBelow(input, around) === depth,
    );
    if (boxes.length >= 4 && boxes.length <= 8 && boxes.includes(field)) return boxes;
    if (boxes.length > 8) return null;
  }
  return null;
}

function depthBelow(el: Element, ancestor: Element): number {
  let depth = 0;
  for (let node: Element | null = el; node && node !== ancestor; node = node.parentElement) depth++;
  return depth;
}

/** The form a field belongs to: its <form>, else the smallest box around it that holds a password
 *  field (a formless sign-in), else the page. A password field's own box may hold it alone, each
 *  field in a box of its own (NWCD's SSO): its sign-in is then the first box around that holds
 *  other fields too, where one of them names a user name before it. */
export function formOf(field: HTMLInputElement): HTMLElement {
  if (field.form) return field.form;
  const doc = field.ownerDocument;
  let smallest: HTMLElement | null = null;
  for (let node = field.parentElement; node && node !== doc.body; node = node.parentElement) {
    // A password field of its own, not one inside a form further down.
    const formless = Array.from(
      node.querySelectorAll<HTMLInputElement>('input[type="password" i]'),
    ).some((p) => !p.form);
    if (!formless) continue;
    if (!isPassword(field)) return node;
    smallest ??= node;
    const others = Array.from(node.querySelectorAll("input")).filter(
      (input) => input !== field && !input.form && isFillable(input),
    );
    if (others.length === 0) continue;
    return node === smallest || others.some((input) => userLike(input) && before(input, field))
      ? node
      : smallest;
  }
  return smallest ?? doc.body;
}

/** The fillable inputs of `root` that belong to the same form as `field` (none: formless ones). */
function inputsOf(root: HTMLElement, field?: HTMLInputElement): HTMLInputElement[] {
  return Array.from(root.querySelectorAll("input")).filter(
    (input) => isFillable(input) && (!field || input.form === field.form),
  );
}

function userLike(input: HTMLInputElement): boolean {
  const type = typeOf(input);
  const token = autocomplete(input);
  if (token === "username" || token === "email" || type === "email") return true;
  if (type === "search" || type === "password" || type === "number") return false;
  const text = hints(input);
  return USER_HINT.test(text) && !NOT_USER.test(text);
}

function before(a: Element, b: Element): boolean {
  return Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
}

/** The visible user name field of a form: one that names itself so, else the nearest text field
 *  before the first password field. */
function usernameOf(
  root: HTMLElement,
  field: HTMLInputElement,
  passwords: HTMLInputElement[],
  view: View,
): HTMLInputElement | null {
  const first = passwords[0];
  const candidates = inputsOf(root, field).filter(
    (input) =>
      !isPassword(input) &&
      view.visible(input) &&
      typeOf(input) !== "search" &&
      !looksOtp(input, view) &&
      (!first || before(input, first)),
  );
  const named = candidates.filter(userLike);
  if (named.length > 0) return named.at(-1) ?? null;
  if (!first) return null;
  return candidates.findLast((input) => ["", "text", "tel"].includes(typeOf(input))) ?? null;
}

function hiddenUsernameOf(root: HTMLElement, view: View): HTMLInputElement | null {
  return (
    Array.from(root.querySelectorAll("input")).find(
      (input) =>
        !view.visible(input) &&
        (autocomplete(input) === "username" || autocomplete(input) === "email") &&
        input.value !== "",
    ) ?? null
  );
}

/** Where a new password goes: the fields that say `new-password`, else (without tokens) every
 *  password field after the first of three, or all of two. */
function newPasswordsOf(passwords: HTMLInputElement[]): HTMLInputElement[] {
  const marked = passwords.filter((p) => autocomplete(p) === "new-password");
  if (marked.length > 0) return marked;
  if (passwords.length >= 3) return passwords.slice(1);
  if (passwords.length === 2) return passwords;
  return passwords.filter((p) => p.hasAttribute("passwordrules"));
}

function signUp(passwords: HTMLInputElement[]): boolean {
  if (passwords.some((p) => autocomplete(p) === "new-password" || p.hasAttribute("passwordrules")))
    return true;
  return passwords.length >= 2;
}

/** What the focused `field` belongs to. */
export function scanField(field: HTMLInputElement, view: View): FormScan {
  const root = formOf(field);
  const base: FormScan = {
    kind: "none",
    root,
    username: null,
    hiddenUsername: null,
    passwords: [],
    newPasswords: [],
    otp: [],
    rules: null,
    minlength: null,
    maxlength: null,
  };
  if (typeOf(field) === "search") return base;
  const group = otpGroup(field, view);
  if (group) return { ...base, kind: "otp", otp: group };
  if (looksOtp(field, view)) return { ...base, kind: "otp", otp: [field] };
  // A code masked as a password is not one (NWCD's SSO: the MFA code after the password).
  const passwords = inputsOf(root, field).filter(
    (input) => isPassword(input) && view.visible(input) && !looksOtp(input, view),
  );
  const username = usernameOf(root, field, passwords, view);
  const hiddenUsername = hiddenUsernameOf(root, view);
  if (passwords.length === 0) {
    if (username && username === field && userLike(field))
      return { ...base, kind: "username", username, hiddenUsername };
    return base;
  }
  const scan: FormScan = { ...base, kind: "login", username, hiddenUsername, passwords };
  if (!signUp(passwords)) return scan;
  const newPasswords = newPasswordsOf(passwords);
  const target = newPasswords[0] ?? passwords[0];
  return {
    ...scan,
    kind: "signup",
    newPasswords,
    rules: target?.getAttribute("passwordrules") ?? null,
    minlength: target ? lengthOf(target, "minlength") : null,
    maxlength: target ? lengthOf(target, "maxlength") : null,
  };
}

/** The sign-in forms the page shows (a fill as the page loads takes only these). */
export function findLoginForms(doc: Document, view: View): FormScan[] {
  const seen = new Set<HTMLElement>();
  const forms: FormScan[] = [];
  for (const input of Array.from(doc.querySelectorAll("input"))) {
    if (!isFillable(input) || !isPassword(input) || !view.visible(input)) continue;
    const scan = scanField(input, view);
    if (scan.kind !== "login" || seen.has(scan.root)) continue;
    seen.add(scan.root);
    forms.push(scan);
  }
  return forms;
}

/** The page's one-time-code field (or boxes), the first it shows. */
export function findOtp(doc: Document, view: View): HTMLInputElement[] | null {
  const inputs = Array.from(doc.querySelectorAll("input")).filter((input) => view.visible(input));
  for (const input of inputs) {
    const group = otpGroup(input, view);
    if (group) return group;
    if (looksOtp(input, view)) return [input];
  }
  return null;
}
