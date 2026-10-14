import {
  blankField,
  blankLoginForm,
  loginDraft,
  loginFormHost,
  loginFormOf,
  loginPatch,
  loginProblems,
  typedPassword,
} from "./login-form";
import { mockItemView } from "./mock-backend";

const github = () =>
  mockItemView("GitHub", {
    username: "octocat",
    has_password: true,
    has_notes: true,
    urls: [{ url: "https://github.com/login", matching: "domain" }],
    group: "工作",
    tags: ["开发"],
    fields: [
      { id: "f-pin", label: "PIN", hidden: true, value: null, has_value: true, page_field: null },
      {
        id: "f-note",
        label: "Plan",
        hidden: false,
        value: "Pro",
        has_value: true,
        page_field: "plan",
      },
    ],
  });

describe("a login's form", () => {
  it("starts empty for a new login, its title given, and builds the draft", () => {
    const form = blankLoginForm("Example");
    expect(form.title).toBe("Example");
    expect(form.urls).toHaveLength(1);
    form.username = "me";
    form.password = { kind: "set", value: "s3cret" };
    form.urls = [
      { ...form.urls[0], key: 1, url: " example.com ", matching: "host" },
      { key: 2, url: "", matching: "domain" },
    ];
    form.group = " 工作 ";
    form.mark = " ";
    form.fields = [{ ...blankField(), label: "PIN", value: "1234", hidden: true }, blankField()];
    expect(loginDraft(form)).toEqual({
      title: "Example",
      username: "me",
      email: "",
      password: { type: "set", value: "s3cret" },
      urls: [{ url: "example.com", matching: "host" }],
      notes: "",
      tags: [],
      group: "工作",
      favorite: false,
      color: "auto",
      mark: null,
      protection: "fill",
      autofill: { on_page_load: "default", auto_submit: "default", fill_otp: "default" },
      totp_entry: null,
      // The empty field just added is left out.
      fields: [
        { label: "PIN", value: { type: "set", value: "1234" }, hidden: true, page_field: "" },
      ],
    });
    // No password typed, none sent; the generator's is sent as the generator's.
    expect(loginDraft({ ...form, password: { kind: "set", value: "" } })).not.toHaveProperty(
      "password",
    );
    expect(loginDraft({ ...form, password: { kind: "generated", value: "x" } }).password).toEqual({
      type: "generated",
    });
  });

  it("holds an edited login without its secrets and patches what the user touched", () => {
    const form = loginFormOf(github());
    expect(form.password).toEqual({ kind: "keep" });
    expect(form.notes).toBeUndefined();
    expect(form.fields.map((f) => f.value)).toEqual([undefined, "Pro"]);
    form.title = " GitHub 公司 ";
    const patch = loginPatch(form);
    expect(patch.title).toBe("GitHub 公司");
    expect(patch).not.toHaveProperty("password");
    expect(patch).not.toHaveProperty("notes");
    expect(patch.fields).toEqual([
      { id: "f-pin", label: "PIN", hidden: true, page_field: "" },
      {
        id: "f-note",
        label: "Plan",
        hidden: false,
        page_field: "plan",
        value: { type: "set", value: "Pro" },
      },
    ]);
    // Notes emptied are cleared; a field removed goes.
    form.notes = "";
    form.fields = form.fields.map((f) => (f.id === "f-pin" ? { ...f, removed: true } : f));
    expect(loginPatch(form).notes).toEqual({ type: "clear" });
    expect(loginPatch(form).fields?.[0]).toEqual({ id: "f-pin", remove: true });
    expect(loginPatch({ ...form, notes: "new" }).notes).toEqual({ type: "set", value: "new" });
  });

  it("keeps the password of a login being edited when its field is emptied", () => {
    expect(typedPassword("", true)).toEqual({ kind: "keep" });
    expect(typedPassword("", false)).toEqual({ kind: "set", value: "" });
    expect(typedPassword("x", true)).toEqual({ kind: "set", value: "x" });
  });

  it("finds the title missing and the sites that are no address, a regular expression aside", () => {
    const form = blankLoginForm();
    form.urls = [
      { key: 1, url: "not a url", matching: "domain" },
      { key: 2, url: "^https://(a|b)\\.example\\.com", matching: "regex" },
      { key: 3, url: "app.example.com/login", matching: "starts_with" },
    ];
    expect(loginProblems(form)).toEqual({ title: true, urls: new Set([1]), count: 2 });
    expect(loginFormHost(form)).toBe("app.example.com");
    expect(loginFormHost(blankLoginForm())).toBeUndefined();
  });
});
