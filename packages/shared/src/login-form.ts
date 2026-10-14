// A login's form as the desktop's dialog and the phone's page hold it, and what saving it sends:
// `item_add`'s draft or `item_update`'s patch. The secrets of a login being edited never come to the
// form by themselves: the password and the hidden fields stay unless something is typed, and the
// notes only once the user asked to edit them. A password made by the generator is sent as the
// generator's (`{"type": "generated"}`), so it does not go back from the webview.
import { hostOf } from "./items";
import type {
  AccountColor,
  FieldChange,
  ItemDraft,
  ItemPatch,
  ItemView,
  Matching,
  Protection,
  SecretChange,
  Tri,
} from "./schema";

/** A password as the form holds it: kept (an edit), typed, or made by the generator. */
export type LoginPassword =
  | { kind: "keep" }
  | { kind: "set"; value: string }
  | { kind: "generated"; value: string };

export interface LoginUrl {
  /** The row's own key (the list's), not saved. */
  key: number;
  url: string;
  matching: Matching;
}

export interface LoginFieldDraft {
  key: number;
  /** The field's id, for one the login has. */
  id?: string;
  label: string;
  /** What was typed; `undefined` keeps the value of a hidden field being edited. */
  value: string | undefined;
  hidden: boolean;
  pageField: string;
  removed: boolean;
}

export interface LoginForm {
  title: string;
  username: string;
  email: string;
  password: LoginPassword;
  urls: LoginUrl[];
  totp: string | null;
  /** `undefined` keeps the notes of a login being edited (not shown until asked for). */
  notes: string | undefined;
  fields: LoginFieldDraft[];
  group: string;
  tags: string[];
  favorite: boolean;
  color: AccountColor;
  mark: string;
  protection: Protection;
  autofill: { on_page_load: Tri; auto_submit: Tri; fill_otp: Tri };
}

let lastKey = 0;

/** A key for a new row of the form. */
export function nextFormKey(): number {
  lastKey += 1;
  return lastKey;
}

export function blankUrl(): LoginUrl {
  return { key: nextFormKey(), url: "", matching: "domain" };
}

/** A custom field just added, empty. */
export function blankField(): LoginFieldDraft {
  return { key: nextFormKey(), label: "", value: "", hidden: false, pageField: "", removed: false };
}

/** A new login's form, its title given (from a search that found nothing). */
export function blankLoginForm(title = ""): LoginForm {
  return {
    title,
    username: "",
    email: "",
    password: { kind: "set", value: "" },
    urls: [blankUrl()],
    totp: null,
    notes: "",
    fields: [],
    group: "",
    tags: [],
    favorite: false,
    color: "auto",
    mark: "",
    protection: "fill",
    autofill: { on_page_load: "default", auto_submit: "default", fill_otp: "default" },
  };
}

/** The form of a login being edited: everything but its secrets. */
export function loginFormOf(item: ItemView): LoginForm {
  return {
    title: item.title,
    username: item.username,
    email: item.email,
    password: { kind: "keep" },
    urls:
      item.urls.length > 0
        ? item.urls.map((u) => ({ key: nextFormKey(), url: u.url, matching: u.matching }))
        : [blankUrl()],
    totp: item.totp_entry,
    notes: item.has_notes ? undefined : "",
    fields: item.fields.map((f) => ({
      key: nextFormKey(),
      id: f.id,
      label: f.label,
      value: f.hidden ? undefined : (f.value ?? ""),
      hidden: f.hidden,
      pageField: f.page_field ?? "",
      removed: false,
    })),
    group: item.group ?? "",
    tags: [...item.tags],
    favorite: item.favorite,
    color: item.color,
    mark: item.mark ?? "",
    protection: item.protection,
    autofill: { ...item.autofill },
  };
}

/** What a typed password becomes: emptied while editing, the password stays as it is (never lost
 *  to a slip of the backspace key). */
export function typedPassword(value: string, editing: boolean): LoginPassword {
  return value === "" && editing ? { kind: "keep" } : { kind: "set", value };
}

/** What the form would get wrong: the title, and the sites that are no address (a regex aside). */
export function loginProblems(form: LoginForm): {
  title: boolean;
  urls: Set<number>;
  count: number;
} {
  const urls = new Set<number>();
  for (const row of form.urls)
    if (row.url.trim() !== "" && row.matching !== "regex" && hostOf(row.url) === null)
      urls.add(row.key);
  const title = form.title.trim() === "";
  return { title, urls, count: (title ? 1 : 0) + urls.size };
}

/** The host of the form's first site: the generator's rules are that site's. */
export function loginFormHost(form: LoginForm): string | undefined {
  return form.urls.map((u) => hostOf(u.url)).find((h) => h !== null) ?? undefined;
}

/** The password's change: none when kept or left empty. */
function passwordChange(value: LoginPassword): SecretChange | undefined {
  if (value.kind === "keep") return undefined;
  if (value.kind === "generated") return { type: "generated" };
  return value.value === "" ? undefined : { type: "set", value: value.value };
}

function urlsOf(form: LoginForm) {
  return form.urls
    .filter((u) => u.url.trim() !== "")
    .map((u) => ({ url: u.url.trim(), matching: u.matching }));
}

function fieldChanges(fields: readonly LoginFieldDraft[]): FieldChange[] {
  return fields.flatMap((f): FieldChange[] => {
    if (f.id === undefined) {
      if (f.removed || (f.label.trim() === "" && (f.value ?? "") === "")) return [];
      return [
        {
          label: f.label,
          value: { type: "set", value: f.value ?? "" },
          hidden: f.hidden,
          page_field: f.pageField,
        },
      ];
    }
    if (f.removed) return [{ id: f.id, remove: true }];
    return [
      {
        id: f.id,
        label: f.label,
        hidden: f.hidden,
        page_field: f.pageField,
        ...(f.value === undefined ? {} : { value: { type: "set", value: f.value } }),
      },
    ];
  });
}

/** A new login, for `item_add`. */
export function loginDraft(form: LoginForm): ItemDraft {
  const password = passwordChange(form.password);
  return {
    title: form.title.trim(),
    username: form.username,
    email: form.email,
    ...(password === undefined ? {} : { password }),
    urls: urlsOf(form),
    notes: form.notes ?? "",
    tags: form.tags,
    group: form.group.trim() === "" ? null : form.group.trim(),
    favorite: form.favorite,
    color: form.color,
    mark: form.mark.trim() === "" ? null : form.mark,
    protection: form.protection,
    autofill: form.autofill,
    totp_entry: form.totp,
    fields: fieldChanges(form.fields),
  };
}

/** A login's edit, for `item_update`. The core writes only what changed: a part named here with its
 *  old value stays as it was. */
export function loginPatch(form: LoginForm): ItemPatch {
  const password = passwordChange(form.password);
  return {
    title: form.title.trim(),
    username: form.username,
    email: form.email,
    ...(password === undefined ? {} : { password }),
    urls: urlsOf(form),
    ...(form.notes === undefined
      ? {}
      : { notes: form.notes === "" ? { type: "clear" } : { type: "set", value: form.notes } }),
    tags: form.tags,
    group: form.group.trim(),
    favorite: form.favorite,
    color: form.color,
    mark: form.mark,
    protection: form.protection,
    autofill: form.autofill,
    totp_entry: form.totp,
    fields: fieldChanges(form.fields),
  };
}
