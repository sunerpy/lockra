// Filling a form with what the app gave for it: only the form in question and only the fields the
// page shows (the user name, the password or a sign-up's new passwords, custom fields by the page
// field they name, a code field on the same page), through each field's own value setter and the
// events pages listen to. The values are not kept: the call ends, the strings go with it.
import type { FillValues } from "../shared/messages";
import { type FormScan, type View, findOtp, isFillable } from "./fields";

// Called with `.call(input, …)`: the input is its `this`.
// oxlint-disable-next-line typescript/unbound-method
const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;

/** Set a field as typing would: the native setter (frameworks that wrap `value` see the change on
 *  the input event), then input and change, both bubbling. */
export function setValue(input: HTMLInputElement, value: string): void {
  if (valueSetter) valueSetter.call(input, value);
  else input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

/** A code into its field, or a digit per box when it is split; nothing when it does not fit.
 *  Whether it went in. */
export function fillOtp(inputs: readonly HTMLInputElement[], code: string): boolean {
  if (inputs.length === 1 && inputs[0]) {
    setValue(inputs[0], code);
    return true;
  }
  const digits = Array.from(code);
  if (digits.length !== inputs.length) return false;
  inputs.forEach((input, index) => setValue(input, digits[index] ?? ""));
  return true;
}

const SUBMIT =
  /sign.?in|log.?in|submit|continue|next|verify|confirm|登录|登入|提交|继续|下一步|验证|确认|确定/i;
/** Buttons that do something else than send this form: send a code, cancel or go back, lead to
 *  another way in or another page, show the password. */
const NOT_SUBMIT =
  /获取|发送|重新发送|重发|send|resend|get.?code|cancel|取消|back|返回|上一步|forgot|忘记|register|注册|sign.?up|create|google|apple|microsoft|github|facebook|wechat|微信|qq|alipay|支付宝|sso|passkey|通行密钥|show|显示|hide|隐藏/i;
/** How long the page may take to enable its button once the values are in. */
const SUBMIT_WAIT_MS = 1000;
const nextMoment = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 50));

function labelOf(el: Element): string {
  return ["textContent", "value", "aria-label", "title", "alt"]
    .map((part) => (part === "textContent" ? el.textContent : el.getAttribute(part)))
    .filter(Boolean)
    .join(" ");
}

function submits(el: Element): el is HTMLButtonElement | HTMLInputElement {
  if (el instanceof HTMLButtonElement) return el.type === "submit";
  return el instanceof HTMLInputElement && (el.type === "submit" || el.type === "image");
}

/** The button that sends `root`: a form's default button (its first submit button shown, as
 *  Enter presses it), else one that says it signs in or goes on; never one that sends a code,
 *  cancels or leads elsewhere. */
function sendingButton(root: HTMLElement, view: View): HTMLElement | null {
  const shown = Array.from(
    root.querySelectorAll<HTMLElement>(
      'button, input[type="submit" i], input[type="image" i], [role="button"]',
    ),
  ).filter((el) => view.visible(el) && !NOT_SUBMIT.test(labelOf(el)));
  if (root instanceof HTMLFormElement) {
    const byDefault = shown.find((el) => submits(el) && el.form === root);
    if (byDefault) return byDefault;
  }
  return shown.find((el) => SUBMIT.test(labelOf(el))) ?? null;
}

function disabled(el: HTMLElement): boolean {
  return (
    ((el instanceof HTMLButtonElement || el instanceof HTMLInputElement) && el.disabled) ||
    el.getAttribute("aria-disabled") === "true"
  );
}

/** Send the form as the user would: its button clicked once the page has taken the values (and
 *  enabled the button), so the page's own handlers run, which `requestSubmit` alone skips (the
 *  AWS console posted the form itself and answered 400); a form without a button is sent as it
 *  is. A button the page keeps disabled leaves the form unsent. Whether it went. */
export async function submitForm(
  root: HTMLElement,
  view: View,
  pause: () => Promise<void> = nextMoment,
  now: () => number = Date.now,
): Promise<boolean> {
  const until = now() + SUBMIT_WAIT_MS;
  await pause();
  for (;;) {
    if (!root.isConnected) return false;
    const button = sendingButton(root, view);
    if (!button) {
      if (!(root instanceof HTMLFormElement)) return false;
      root.requestSubmit();
      return true;
    }
    if (!disabled(button)) {
      button.click();
      return true;
    }
    if (now() >= until) return false;
    // oxlint-disable-next-line no-await-in-loop -- one look per moment, up to SUBMIT_WAIT_MS
    await pause();
  }
}

/** What a fill put in: how many fields, the password, the code. */
export interface FillResult {
  filled: number;
  password: boolean;
  code: boolean;
}

/** Fill the scanned form. Sent (when the policy says so) once the login's last part is in: the
 *  password, and its code too when it has one (the user's rule, 2026-10-16: a login with a code
 *  signs in at its code's step). A first step that asks for the user name alone is never sent:
 *  the user goes on (the AWS console's root sign-in answered 400). */
export function fillForm(scan: FormScan, values: FillValues, view: View): FillResult {
  let filled = 0;
  let password = false;
  let code = false;
  const put = (input: HTMLInputElement | null | undefined, value: string) => {
    if (!input || value === "" || !isFillable(input) || !view.visible(input)) return false;
    setValue(input, value);
    filled++;
    return true;
  };
  // A sign-up keeps a user name the user typed.
  if (!values.newPassword || scan.username?.value === "") put(scan.username, values.username);
  const passwords = values.newPassword ? scan.newPasswords : scan.passwords.slice(0, 1);
  for (const field of passwords) password = put(field, values.password) || password;
  for (const { page_field, value } of values.fields) {
    const named = Array.from(scan.root.querySelectorAll("input")).find(
      (input) => input.name === page_field || input.id === page_field,
    );
    put(named, value);
  }
  if (values.otp) {
    const otp = findOtp(scan.root.ownerDocument, view);
    if (otp && fillOtp(otp, values.otp)) {
      filled += otp.length;
      code = true;
    }
  }
  // The login's last part: its password, or its code on a page that asks for the code alone.
  const last = password || (scan.kind === "otp" && code);
  if (values.submit && last && (values.otp === null || code)) void submitForm(scan.root, view);
  return { filled, password, code };
}
