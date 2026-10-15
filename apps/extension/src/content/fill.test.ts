// Filling: only the visible fields of the form in question, through the field's own value setter and
// the events a page listens to; a code split over one box per digit gets a digit per box.
import type { View } from "./fields";
import { scanField } from "./fields";
import { fillForm, fillOtp, setValue } from "./fill";

const view: View = {
  visible: (el) =>
    el instanceof HTMLElement &&
    !el.closest("[hidden], [data-hidden]") &&
    el.style.display !== "none",
};

function input(id: string): HTMLInputElement {
  const el = document.getElementById(id);
  if (!(el instanceof HTMLInputElement)) throw new Error(`no input #${id}`);
  return el;
}

const VALUES = { username: "alice", password: "s3cret", fields: [], otp: null, submit: false };

describe("setting a value", () => {
  it("goes through the value setter and fires input and change, bubbling", () => {
    document.body.innerHTML = `<form id="f"><input id="a"></form>`;
    const events: string[] = [];
    const form = document.getElementById("f");
    form?.addEventListener("input", (e) =>
      events.push(`input:${(e.target as HTMLInputElement).value}`),
    );
    form?.addEventListener("change", () => events.push("change"));
    setValue(input("a"), "hello");
    expect(input("a").value).toBe("hello");
    expect(events).toEqual(["input:hello", "change"]);
  });
});

describe("filling a sign-in form", () => {
  it("fills the user name and the password of the form, nothing hidden and nothing elsewhere", () => {
    document.body.innerHTML = `
      <form id="login">
        <input id="trap" name="username" style="display: none">
        <input id="user" autocomplete="username">
        <input type="password" id="pass">
        <input id="pin" name="pin">
      </form>
      <form id="other"><input id="user2" autocomplete="username"><input type="password" id="pass2"></form>`;
    const filled = fillForm(
      scanField(input("pass"), view),
      {
        ...VALUES,
        fields: [
          { page_field: "pin", value: "4242" },
          { page_field: "nowhere", value: "x" },
        ],
      },
      view,
    );
    expect(filled).toBe(3);
    expect([input("user").value, input("pass").value, input("pin").value]).toEqual([
      "alice",
      "s3cret",
      "4242",
    ]);
    expect([input("trap").value, input("user2").value, input("pass2").value]).toEqual(["", "", ""]);
  });

  it("fills the password alone on the second step of a split sign-in", () => {
    document.body.innerHTML = `<form><input type="password" id="pass"></form>`;
    expect(fillForm(scanField(input("pass"), view), VALUES, view)).toBe(1);
    expect(input("pass").value).toBe("s3cret");
  });

  it("puts a generated password in every new-password field, and the user name only where none was typed", () => {
    document.body.innerHTML = `
      <form>
        <input id="user" autocomplete="username" value="typed">
        <input type="password" id="old" autocomplete="current-password">
        <input type="password" id="new" autocomplete="new-password">
        <input type="password" id="again" autocomplete="new-password">
      </form>`;
    fillForm(
      scanField(input("new"), view),
      { ...VALUES, password: "Generated-1", newPassword: true },
      view,
    );
    expect([
      input("user").value,
      input("old").value,
      input("new").value,
      input("again").value,
    ]).toEqual(["typed", "", "Generated-1", "Generated-1"]);
  });

  it("fills a code on the same page and submits when told", () => {
    document.body.innerHTML = `
      <form id="f"><input id="user" autocomplete="username"><input type="password" id="pass"><input id="code" autocomplete="one-time-code"></form>`;
    const form = document.getElementById("f") as HTMLFormElement;
    const submitted = vi.fn((event: Event) => event.preventDefault());
    form.addEventListener("submit", submitted);
    fillForm(scanField(input("pass"), view), { ...VALUES, otp: "123456", submit: true }, view);
    expect(input("code").value).toBe("123456");
    expect(submitted).toHaveBeenCalledTimes(1);
  });

  it("regression: a first step with the user name alone is filled but never submitted", () => {
    // 2026-10-16, the AWS console's root sign-in: its first page asks for the e-mail alone; sent
    // by Lockra, it answered 400. Only a step that took a password is sent.
    document.body.innerHTML = `
      <form id="f"><input id="email" type="email" autocomplete="username"><button>Next</button></form>`;
    const form = document.getElementById("f") as HTMLFormElement;
    const submitted = vi.fn((event: Event) => event.preventDefault());
    form.addEventListener("submit", submitted);
    // A valid address: an invalid one would stop the browser's own submit before the page's.
    const values = { ...VALUES, username: "root@example.com", submit: true };
    expect(fillForm(scanField(input("email"), view), values, view)).toBe(1);
    expect(input("email").value).toBe("root@example.com");
    expect(submitted).not.toHaveBeenCalled();
  });

  it("does not submit when the password field is there but hidden, as a step before it shows", () => {
    document.body.innerHTML = `
      <form id="f"><input id="user" autocomplete="username"><input type="password" id="pass" style="display: none"></form>`;
    const form = document.getElementById("f") as HTMLFormElement;
    const submitted = vi.fn((event: Event) => event.preventDefault());
    form.addEventListener("submit", submitted);
    fillForm(scanField(input("user"), view), { ...VALUES, submit: true }, view);
    expect([input("user").value, input("pass").value]).toEqual(["alice", ""]);
    expect(submitted).not.toHaveBeenCalled();
  });

  it("submits the password step of a split sign-in when told", () => {
    document.body.innerHTML = `<form id="f"><input type="password" id="pass"></form>`;
    const form = document.getElementById("f") as HTMLFormElement;
    const submitted = vi.fn((event: Event) => event.preventDefault());
    form.addEventListener("submit", submitted);
    fillForm(scanField(input("pass"), view), { ...VALUES, submit: true }, view);
    expect(submitted).toHaveBeenCalledTimes(1);
  });

  it("submits a form without a <form> by its submit button", () => {
    document.body.innerHTML = `<div><input id="user" autocomplete="username"><input type="password" id="pass"><button id="go" type="button">Sign in</button><button>Help</button></div>`;
    const clicked = vi.fn();
    document.getElementById("go")?.addEventListener("click", clicked);
    fillForm(scanField(input("pass"), view), { ...VALUES, submit: true }, view);
    expect(clicked).toHaveBeenCalledTimes(1);
  });
});

describe("filling a code", () => {
  it("puts it in one field, or a digit per box", () => {
    document.body.innerHTML = `<input id="one"><div>${[1, 2, 3, 4, 5, 6].map((n) => `<input id="d${n}" maxlength="1">`).join("")}</div>`;
    fillOtp([input("one")], "654321");
    expect(input("one").value).toBe("654321");
    const boxes = [1, 2, 3, 4, 5, 6].map((n) => input(`d${n}`));
    fillOtp(boxes, "654321");
    expect(boxes.map((b) => b.value).join("")).toBe("654321");
    // Fewer boxes than digits: the code does not fit, nothing goes in.
    boxes.forEach((b) => setValue(b, ""));
    fillOtp(boxes.slice(0, 4), "654321");
    expect(boxes.map((b) => b.value).join("")).toBe("");
  });
});
