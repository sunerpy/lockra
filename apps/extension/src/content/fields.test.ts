// Field detection on DOM fixtures: a sign-in form, the two steps of a split sign-in, sign-up forms,
// one-time-code fields and codes split into one box per digit. jsdom draws nothing, so what is
// visible comes from a test view (hidden, display: none, or data-hidden); the real one measures.
import { type View, findLoginForms, findOtp, scanField, otpGroup, isFillable } from "./fields";

const view: View = {
  visible: (el) =>
    el instanceof HTMLElement &&
    !el.closest("[hidden], [data-hidden]") &&
    el.getAttribute("type") !== "hidden" &&
    el.style.display !== "none",
};

function page(html: string): void {
  document.body.innerHTML = html;
}

function input(id: string): HTMLInputElement {
  const el = document.getElementById(id);
  if (!(el instanceof HTMLInputElement)) throw new Error(`no input #${id}`);
  return el;
}

describe("a sign-in form", () => {
  it("finds the user name before the password, whichever field has the focus", () => {
    page(`
      <form id="login">
        <input type="search" id="q" name="q">
        <label>Email <input type="email" id="user" name="login" autocomplete="username"></label>
        <input type="password" id="pass" name="password" autocomplete="current-password">
        <input type="checkbox" id="remember">
        <button type="submit">Sign in</button>
      </form>`);
    const scans = ["user", "pass"].map((id) => {
      const scan = scanField(input(id), view);
      return {
        id,
        kind: scan.kind,
        username: scan.username?.id,
        passwords: scan.passwords.map((p) => p.id),
        root: scan.root.id,
      };
    });
    expect(scans).toEqual(
      ["user", "pass"].map((id) => ({
        id,
        kind: "login",
        username: "user",
        passwords: ["pass"],
        root: "login",
      })),
    );
  });

  it("takes the nearest text field before the password when nothing names the user name", () => {
    page(`
      <form>
        <input type="text" id="other" name="nickname">
        <input type="text" id="name" name="x1">
        <input type="password" id="pass">
      </form>`);
    expect(scanField(input("pass"), view).username?.id).toBe("name");
  });

  it("works without a <form>, and leaves hidden fields alone", () => {
    page(`
      <div class="page">
        <div class="box">
          <input type="text" id="honeypot" name="username" style="display: none">
          <input type="text" id="user" name="account">
          <input type="password" id="decoy" hidden>
          <input type="password" id="pass">
        </div>
      </div>`);
    const scan = scanField(input("user"), view);
    expect(scan.kind).toBe("login");
    expect(scan.username?.id).toBe("user");
    expect(scan.passwords.map((p) => p.id)).toEqual(["pass"]);
    expect(scan.root.className).toBe("box");
  });

  it("is found on the page for a fill as it loads", () => {
    page(`
      <form id="a"><input id="u" autocomplete="username"><input type="password" id="p"></form>
      <form id="b"><input id="search" type="search"></form>
      <form id="c" data-hidden><input id="u2"><input type="password" id="p2"></form>`);
    const forms = findLoginForms(document, view);
    expect(forms.map((f) => f.root.id)).toEqual(["a"]);
    expect(forms[0]?.kind).toBe("login");
  });
});

describe("a sign-in in two steps", () => {
  it("knows the first step by its lone user name field", () => {
    page(
      `<form id="step1"><input type="email" id="email" name="identifier"><button>Next</button></form>`,
    );
    const scan = scanField(input("email"), view);
    expect(scan.kind).toBe("username");
    expect(scan.username?.id).toBe("email");
    expect(scan.passwords).toEqual([]);
  });

  it("knows the second step by its lone password field, reading a hidden user name if the page kept one", () => {
    page(`
      <form id="step2">
        <input type="text" id="kept" name="username" autocomplete="username" value="alice" style="display: none">
        <input type="password" id="pass">
      </form>`);
    const scan = scanField(input("pass"), view);
    expect(scan.kind).toBe("login");
    expect(scan.username).toBeNull();
    expect(scan.hiddenUsername?.value).toBe("alice");
  });

  it("does not take any text field for a user name", () => {
    page(`<form><input type="text" id="city" name="city"><button>Go</button></form>`);
    expect(scanField(input("city"), view).kind).toBe("none");
  });
});

describe("a sign-up form", () => {
  it("is told by new-password, a confirmation field or passwordrules", () => {
    page(
      `<form id="f"><input id="u" autocomplete="username"><input type="password" id="p" autocomplete="new-password" minlength="12" maxlength="64"></form>`,
    );
    let scan = scanField(input("p"), view);
    expect(scan.kind).toBe("signup");
    expect(scan.newPasswords.map((p) => p.id)).toEqual(["p"]);
    expect([scan.minlength, scan.maxlength]).toEqual([12, 64]);

    page(
      `<form><input id="u" name="email"><input type="password" id="p1"><input type="password" id="p2"></form>`,
    );
    scan = scanField(input("p1"), view);
    expect(scan.kind).toBe("signup");
    expect(scan.newPasswords.map((p) => p.id)).toEqual(["p1", "p2"]);
    expect(scan.username?.id).toBe("u");

    page(
      `<form><input id="u"><input type="password" id="p" passwordrules="minlength: 20; required: lower; required: digit;"></form>`,
    );
    scan = scanField(input("p"), view);
    expect(scan.kind).toBe("signup");
    expect(scan.rules).toBe("minlength: 20; required: lower; required: digit;");
    expect([scan.minlength, scan.maxlength]).toEqual([null, null]);
  });

  it("puts a new password in the new fields of a change-password form, not in the current one", () => {
    page(`
      <form>
        <input type="password" id="old" autocomplete="current-password">
        <input type="password" id="new" autocomplete="new-password">
        <input type="password" id="again" autocomplete="new-password">
      </form>`);
    expect(scanField(input("new"), view).newPasswords.map((p) => p.id)).toEqual(["new", "again"]);
    page(
      `<form><input type="password" id="old"><input type="password" id="new"><input type="password" id="again"></form>`,
    );
    expect(scanField(input("old"), view).newPasswords.map((p) => p.id)).toEqual(["new", "again"]);
  });
});

describe("one-time codes", () => {
  it("are found by autocomplete first", () => {
    page(`<form><input id="code" autocomplete="one-time-code" name="x"></form>`);
    const scan = scanField(input("code"), view);
    expect(scan.kind).toBe("otp");
    expect(scan.otp.map((i) => i.id)).toEqual(["code"]);
  });

  it("then by their names with a code's length or a numeric keyboard", () => {
    page(`
      <input id="totp" name="totp" maxlength="6">
      <input id="cn" placeholder="请输入验证码" inputmode="numeric">
      <input id="mfa" aria-label="MFA code" maxlength="8">
      <input id="long" name="verification" maxlength="20">
      <input id="promo" name="promo_code">
      <input id="zip" name="zip" maxlength="6">`);
    expect(scanField(input("totp"), view).kind).toBe("otp");
    expect(scanField(input("cn"), view).kind).toBe("otp");
    expect(scanField(input("mfa"), view).kind).toBe("otp");
    expect(scanField(input("long"), view).kind).not.toBe("otp");
    expect(scanField(input("promo"), view).kind).not.toBe("otp");
    expect(scanField(input("zip"), view).kind).not.toBe("otp");
  });

  it("are put together from one box per digit", () => {
    page(`
      <form>
        <div class="digits">
          ${[1, 2, 3, 4, 5, 6].map((n) => `<span><input id="d${n}" maxlength="1" inputmode="numeric"></span>`).join("")}
        </div>
        <input id="other" maxlength="1">
      </form>`);
    const scan = scanField(input("d3"), view);
    expect(scan.kind).toBe("otp");
    expect(scan.otp.map((i) => i.id)).toEqual(["d1", "d2", "d3", "d4", "d5", "d6"]);
    expect(otpGroup(input("other"), view)).toBeNull();
    // Three boxes are not a code.
    page(`<div>${[1, 2, 3].map((n) => `<input id="b${n}" maxlength="1">`).join("")}</div>`);
    expect(scanField(input("b1"), view).kind).toBe("none");
  });

  it("are found on a page that shows one after a fill", () => {
    page(
      `<form><input id="q" type="search"></form><div><input id="otp" name="otp" maxlength="6"></div>`,
    );
    expect(findOtp(document, view)?.map((i) => i.id)).toEqual(["otp"]);
    page(
      `<div class="otp">${[1, 2, 3, 4].map((n) => `<input id="o${n}" maxlength="1">`).join("")}</div>`,
    );
    expect(findOtp(document, view)?.map((i) => i.id)).toEqual(["o1", "o2", "o3", "o4"]);
    page(`<form><input id="user"></form>`);
    expect(findOtp(document, view)).toBeNull();
  });
});

describe("what can be filled", () => {
  it("is a text-like input that is enabled and writable", () => {
    page(`
      <input id="a"><input id="b" type="email"><input id="c" type="tel"><input id="d" type="password">
      <input id="e" type="checkbox"><input id="f" disabled><input id="g" readonly><input id="h" type="hidden">
      <textarea id="t"></textarea>`);
    expect(["a", "b", "c", "d"].every((id) => isFillable(document.getElementById(id)))).toBe(true);
    expect(["e", "f", "g", "h", "t"].some((id) => isFillable(document.getElementById(id)))).toBe(
      false,
    );
    expect(isFillable(null)).toBe(false);
  });
});
