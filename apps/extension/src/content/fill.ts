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

/** A code into its field, or a digit per box when it is split; nothing when it does not fit. */
export function fillOtp(inputs: readonly HTMLInputElement[], code: string): void {
  if (inputs.length === 1 && inputs[0]) return setValue(inputs[0], code);
  const digits = Array.from(code);
  if (digits.length !== inputs.length) return;
  inputs.forEach((input, index) => setValue(input, digits[index] ?? ""));
}

const SUBMIT = /sign.?in|log.?in|submit|continue|next|登录|登入|提交|继续|下一步/i;

/** Send the form: its own submit (with the page's handlers), else the button that looks like it. */
export function submitForm(root: HTMLElement): void {
  if (root instanceof HTMLFormElement) {
    root.requestSubmit();
    return;
  }
  const buttons = Array.from(
    root.querySelectorAll<HTMLElement>('button, input[type="submit" i], [role="button"]'),
  );
  const button = buttons.find((b) =>
    SUBMIT.test(
      `${b.textContent ?? ""} ${b.getAttribute("value") ?? ""} ${b.getAttribute("aria-label") ?? ""}`,
    ),
  );
  button?.click();
}

/** Fill the scanned form; how many fields took a value. Sent (when the policy says so) only once a
 *  password went in: a first step that asks for the user name alone stays for the user to send,
 *  as some pages refuse it sent any other way (the AWS console's root sign-in answered 400). */
export function fillForm(scan: FormScan, values: FillValues, view: View): number {
  let filled = 0;
  let password = false;
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
    if (otp) {
      fillOtp(otp, values.otp);
      filled += otp.length;
    }
  }
  if (values.submit && password) submitForm(scan.root);
  return filled;
}
