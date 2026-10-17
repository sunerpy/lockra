// Filling: only the visible fields of the form in question, through the field's own value setter and
// the events a page listens to; a code split over one box per digit gets a digit per box.
import type { View } from "./fields";
import { scanField } from "./fields";
import { fillForm, fillOtp, setValue, submitForm } from "./fill";

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
    ).filled;
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
    expect(fillForm(scanField(input("pass"), view), VALUES, view)).toEqual({
      filled: 1,
      password: true,
      code: false,
    });
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

  it("fills a code on the same page and submits when told", async () => {
    document.body.innerHTML = `
      <form id="f"><input id="user" autocomplete="username"><input type="password" id="pass"><input id="code" autocomplete="one-time-code"></form>`;
    const form = document.getElementById("f") as HTMLFormElement;
    const submitted = vi.fn((event: Event) => event.preventDefault());
    form.addEventListener("submit", submitted);
    const result = fillForm(
      scanField(input("pass"), view),
      { ...VALUES, otp: "123456", submit: true },
      view,
    );
    expect(input("code").value).toBe("123456");
    expect(result).toEqual({ filled: 3, password: true, code: true });
    await vi.waitFor(() => expect(submitted).toHaveBeenCalledTimes(1));
  });

  it("fills the code of a page that asks for it alone, and submits it when told", async () => {
    document.body.innerHTML = `
      <form id="f"><input type="tel" id="totpPin" name="totpPin" pattern="[0-9 ]*"><button>Next</button></form>`;
    const form = document.getElementById("f") as HTMLFormElement;
    const submitted = vi.fn((event: Event) => event.preventDefault());
    form.addEventListener("submit", submitted);
    const result = fillForm(
      scanField(input("totpPin"), view),
      { ...VALUES, otp: "123456", submit: true },
      view,
    );
    expect(input("totpPin").value).toBe("123456");
    expect(result).toEqual({ filled: 1, password: false, code: true });
    await vi.waitFor(() => expect(submitted).toHaveBeenCalledTimes(1));
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
    expect(fillForm(scanField(input("email"), view), values, view)).toEqual({
      filled: 1,
      password: false,
      code: false,
    });
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

  it("submits the password step of a split sign-in when told", async () => {
    document.body.innerHTML = `<form id="f"><input type="password" id="pass"></form>`;
    const form = document.getElementById("f") as HTMLFormElement;
    const submitted = vi.fn((event: Event) => event.preventDefault());
    form.addEventListener("submit", submitted);
    fillForm(scanField(input("pass"), view), { ...VALUES, submit: true }, view);
    await vi.waitFor(() => expect(submitted).toHaveBeenCalledTimes(1));
  });

  it("does not send the password step of a login with a code: the code's step does", async () => {
    // The user's rule, 2026-10-16: a login with a linked code is signed in once its code is in.
    document.body.innerHTML = `<form id="f"><input type="password" id="pass"><button>Sign in</button></form>`;
    const form = document.getElementById("f") as HTMLFormElement;
    const submitted = vi.fn((event: Event) => event.preventDefault());
    form.addEventListener("submit", submitted);
    const result = fillForm(
      scanField(input("pass"), view),
      { ...VALUES, otp: "123456", submit: true },
      view,
    );
    expect(result).toEqual({ filled: 1, password: true, code: false });
    await new Promise((resolve) => setTimeout(resolve, 120));
    expect(submitted).not.toHaveBeenCalled();
  });

  it("submits a form without a <form> by its submit button", async () => {
    document.body.innerHTML = `<div><input id="user" autocomplete="username"><input type="password" id="pass"><button id="go" type="button">Sign in</button><button>Help</button></div>`;
    const clicked = vi.fn();
    document.getElementById("go")?.addEventListener("click", clicked);
    fillForm(scanField(input("pass"), view), { ...VALUES, submit: true }, view);
    await vi.waitFor(() => expect(clicked).toHaveBeenCalledTimes(1));
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

describe("sending a form", () => {
  function form(html: string): HTMLFormElement {
    document.body.innerHTML = html;
    const found = document.querySelector("form");
    if (!found) throw new Error("no form");
    return found;
  }
  const wait = () => Promise.resolve();

  it("clicks its default button as Enter would, so the page's own click handler runs", async () => {
    // 2026-10-16, the AWS console: `requestSubmit` skipped the button's handler and the page
    // posted the form itself (400).
    const root = form(`<form><input id="u"><button id="go">Next</button></form>`);
    const clicked = vi.fn((event: Event) => event.preventDefault());
    document.getElementById("go")?.addEventListener("click", clicked);
    const native = vi.fn((event: Event) => event.preventDefault());
    root.addEventListener("submit", native);
    expect(await submitForm(root, view, wait)).toBe(true);
    expect(clicked).toHaveBeenCalledTimes(1);
    expect(native).not.toHaveBeenCalled();
  });

  it("passes over buttons that send a code, cancel or go elsewhere, and hidden ones", async () => {
    const root = form(`
      <form>
        <input id="code">
        <button id="hidden" style="display: none">Verify</button>
        <button id="resend">重新发送验证码</button>
        <button id="cancel">取消</button>
        <button id="verify">验证</button>
      </form>`);
    const clicks: string[] = [];
    for (const button of Array.from(root.querySelectorAll("button")))
      button.addEventListener("click", (event) => {
        event.preventDefault();
        clicks.push(button.id);
      });
    await submitForm(root, view, wait);
    expect(clicks).toEqual(["verify"]);
  });

  it("waits for the page to enable its button, and leaves one it keeps disabled", async () => {
    const root = form(`<form><input id="u"><button id="go" disabled>Next</button></form>`);
    const go = document.getElementById("go") as HTMLButtonElement;
    const clicked = vi.fn((event: Event) => event.preventDefault());
    go.addEventListener("click", clicked);
    let polls = 0;
    const enableLater = async () => {
      polls++;
      if (polls === 3) go.disabled = false;
    };
    expect(await submitForm(root, view, enableLater)).toBe(true);
    expect(clicked).toHaveBeenCalledTimes(1);
    // Kept disabled (or marked aria-disabled): not sent, not even as a bare form.
    go.disabled = true;
    const native = vi.fn((event: Event) => event.preventDefault());
    root.addEventListener("submit", native);
    let now = 0;
    expect(
      await submitForm(
        root,
        view,
        async () => {
          now += 100;
        },
        () => now,
      ),
    ).toBe(false);
    expect([clicked.mock.calls.length, native.mock.calls.length]).toEqual([1, 0]);
  });

  it("clicks a plain button that says it signs in, else sends a form that has no button", async () => {
    const root = form(`<form><input id="u"><button type="button" id="next">下一步</button></form>`);
    const clicked = vi.fn();
    document.getElementById("next")?.addEventListener("click", clicked);
    await submitForm(root, view, wait);
    expect(clicked).toHaveBeenCalledTimes(1);
    const bare = form(`<form><input id="u"></form>`);
    const native = vi.fn((event: Event) => event.preventDefault());
    bare.addEventListener("submit", native);
    expect(await submitForm(bare, view, wait)).toBe(true);
    expect(native).toHaveBeenCalledTimes(1);
    // A box that is not a form and has nothing that signs in: nothing.
    document.body.innerHTML = `<div id="box"><input><button type="button">Help</button></div>`;
    expect(await submitForm(document.getElementById("box") as HTMLElement, view, wait)).toBe(false);
  });
});
