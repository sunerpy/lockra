// The user name a page shows as text where its form has no field for it: AWS IAM Identity Center's
// password step when it remembers the user, and what does not count (a label's word, a sentence, a
// masked name, a control's text, what is out of sight, two names that differ). jsdom draws nothing,
// so what is visible comes from a test view, as in fields.test.ts.
import { type View, scanField } from "./fields";
import { formUsername, shownUsername } from "./shown-username";

const view: View = {
  visible: (el) =>
    el instanceof HTMLElement &&
    !el.closest("[hidden], [data-hidden]") &&
    el.getAttribute("type") !== "hidden" &&
    el.style.display !== "none",
};

/** `html` as the page; its first form, or the body. */
function page(html: string): HTMLElement {
  document.body.innerHTML = html;
  return document.querySelector("form") ?? document.body;
}

function input(id: string): HTMLInputElement {
  const el = document.getElementById(id);
  if (!(el instanceof HTMLInputElement)) throw new Error(`no input #${id}`);
  return el;
}

/** A form that shows `inside` above its password field. */
function shows(inside: string): HTMLElement {
  return page(`<form>${inside}<input type="password" id="pass"></form>`);
}

/** AWS IAM Identity Center's password step when it remembers the user, as measured in the user's
 *  Chrome (2026-10-22): no shadow DOM, no frame, an empty hidden user name outside the form. */
const AWS = `
  <form>
    <div class="awsui-util-container-content">
      <p class="username awsui-util-mb-l" data-testid="test-not-you">user@example.com
        <awsui-button id="not-you-link" class="not-you"><button type="button" class="awsui-button">不是您?</button></awsui-button>
      </p>
      <label for="awsui-input-2">密码</label>
      <awsui-input><input id="awsui-input-2" type="password" autocomplete="off" spellcheck="false" class="awsui-input awsui-input-type-password"></awsui-input>
      <input type="checkbox" id="awsui-checkbox-2"> <label for="awsui-checkbox-2">显示密码</label>
      <awsui-button id="forgot-password-link"><button type="button" class="awsui-button">忘记密码</button></awsui-button>
      <awsui-button id="password-submit-button"><button type="submit" class="awsui-button">登录</button></awsui-button>
    </div>
  </form>
  <div id="wdc_backward_compatibility"><input id="wdc_username" type="hidden"></div>`;

describe("the user name a page shows", () => {
  it("is the text above AWS IAM Identity Center's password when it remembers the user", () => {
    const root = page(AWS);
    expect(shownUsername(root, view)).toBe("user@example.com");
    // The form has no user name field, and the hidden one outside it is empty: the text is all.
    const scan = scanField(input("awsui-input-2"), view);
    expect([scan.kind, scan.root === root, scan.username, scan.hiddenUsername]).toEqual([
      "login",
      true,
      null,
      null,
    ]);
    expect(formUsername(scan, view)).toBe("user@example.com");
  });

  it("is in an element an id, a class, a name or a test id names a user name, as pages spell it", () => {
    for (const named of [
      'class="username"',
      'class="user-name"',
      'id="user_name"',
      'class="userName"',
      'id="USERNAME"',
      'data-testid="login-name"',
      'id="loginName"',
      'class="userid"',
      'id="userID"',
      'class="login-id"',
      'class="email"',
      'id="e-mail"',
      'class="identifier no-translate"',
      'name="username"',
      'class="c-login__user-name--active"',
    ]) {
      const root = shows(`<span ${named}>alice</span>`);
      expect([named, shownUsername(root, view)]).toEqual([named, "alice"]);
    }
    // An account, a name, a user or a nickname alone names no user name; nor words that only
    // hold its letters.
    for (const named of [
      'class="account"',
      'class="name"',
      'id="display-name"',
      'class="user"',
      'class="nickname"',
      'class="mailbox"',
      'class="logins"',
      'title="username"',
    ]) {
      const root = shows(`<span ${named}>alice</span>`);
      expect([named, shownUsername(root, view)]).toEqual([named, ""]);
    }
  });

  it("is a name alone: not a label's word, a sentence, a text with a colon, nor a masked name", () => {
    for (const text of [
      "用户名",
      "账号",
      "帐号",
      "邮箱",
      "手机号",
      "Username",
      "USERNAME",
      "User name",
      "Email",
      "E-mail",
      "Login",
      "Username:",
      "用户名：",
      "alice:",
      "邮箱：",
      "Signed in as alice",
      "Alice Smith",
      "alice smith",
      "a***@example.com",
      "138****1234",
      "•••",
      "—",
      "x".repeat(101),
    ]) {
      const root = shows(`<p class="username">${text}</p>`);
      expect([text, shownUsername(root, view)]).toEqual([text, ""]);
    }
    for (const text of ["a", "x".repeat(100), "13800138000", "李雷", "alice.smith@example.com"]) {
      const root = shows(`<p class="username">  ${text}\n  </p>`);
      expect([text, shownUsername(root, view)]).toEqual([text, text]);
    }
  });

  it("is the element's own text: not a button's in it, nor a name another element splits", () => {
    expect(
      shownUsername(shows(`<p class="username"><button type="button">Not you?</button></p>`), view),
    ).toBe("");
    expect(shownUsername(shows(`<p class="username">al<b>i</b>ce</p>`), view)).toBe("");
    // Nor the part a split leaves whole: the name begins in the bold.
    expect(shownUsername(shows(`<p class="username"><b>alice</b>@example.com</p>`), view)).toBe("");
    // The parts of one text a framework writes apart (a comment between them) are one name.
    expect(shownUsername(shows(`<span class="username">@<!-- -->alice</span>`), view)).toBe(
      "@alice",
    );
    const root = shows(`<span class="username"></span>`);
    root.querySelector("span")?.append("alice", "@example.com");
    expect(shownUsername(root, view)).toBe("alice@example.com");
  });

  it("is not the text of a control, a link or a label, nor of what the page does not show", () => {
    for (const markup of [
      `<a class="username" href="#">alice</a>`,
      `<a href="/switch"><span class="username">alice</span></a>`,
      `<button type="button" class="username">alice</button>`,
      `<button type="button"><span class="username">alice</span></button>`,
      `<label class="username">alice</label>`,
      `<span role="button" class="username">alice</span>`,
      `<span role="link" class="username">alice</span>`,
      `<div role="textbox" class="username">alice</div>`,
      `<select><option class="username">alice</option></select>`,
      `<textarea class="username">alice</textarea>`,
      `<script type="text/plain" class="username">alice</script>`,
      `<style class="username">alice</style>`,
      `<span class="username" hidden>alice</span>`,
      `<span class="username" style="display: none">alice</span>`,
      `<div data-hidden><span class="username">alice</span></div>`,
    ]) {
      expect([markup, shownUsername(shows(markup), view)]).toEqual([markup, ""]);
    }
  });

  it("says nothing when the form shows two names that differ; the same name twice is one", () => {
    expect(
      shownUsername(
        shows(`<p class="username">alice</p><span class="email">bob@example.com</span>`),
        view,
      ),
    ).toBe("");
    expect(
      shownUsername(
        shows(`<p class="username">alice</p><span class="user-name">alice</span>`),
        view,
      ),
    ).toBe("alice");
    // A name out of sight makes no second one; a box around the name holds none of its own.
    expect(
      shownUsername(
        shows(`<p class="username">alice</p><span class="email" hidden>bob@example.com</span>`),
        view,
      ),
    ).toBe("alice");
    expect(
      shownUsername(
        shows(`<div class="username-box"> <span class="username">alice</span> </div>`),
        view,
      ),
    ).toBe("alice");
  });

  it("is looked for in the form only", () => {
    const root = page(
      `<p class="username">alice</p><form><input type="password" id="pass"></form>`,
    );
    expect(shownUsername(root, view)).toBe("");
    expect(formUsername(scanField(input("pass"), view), view)).toBe("");
  });
});

describe("the user name a form gives", () => {
  it("is its user name field's, even empty; else a hidden one's; else the one the page shows", () => {
    const shown = `<p class="username">shown@example.com</p>`;
    // A field the page wants typed gives what is typed in it, nothing before.
    page(`<form>${shown}<input id="user" name="username"><input type="password" id="pass"></form>`);
    expect(formUsername(scanField(input("pass"), view), view)).toBe("");
    input("user").value = "typed@example.com";
    expect(formUsername(scanField(input("pass"), view), view)).toBe("typed@example.com");
    // A user name the page kept in a hidden field comes before the one it shows.
    page(
      `<form>${shown}<input id="kept" autocomplete="username" value="kept@example.com" style="display: none"><input type="password" id="pass"></form>`,
    );
    expect(formUsername(scanField(input("pass"), view), view)).toBe("kept@example.com");
    // Neither: the one it shows, on a sign-up form as on a sign-in.
    page(`<form>${shown}<input type="password" id="pass"></form>`);
    expect(formUsername(scanField(input("pass"), view), view)).toBe("shown@example.com");
    page(
      `<form>${shown}<input type="password" id="pass" autocomplete="new-password"><input type="password" id="again" autocomplete="new-password"></form>`,
    );
    const scan = scanField(input("pass"), view);
    expect([scan.kind, formUsername(scan, view)]).toEqual(["signup", "shown@example.com"]);
  });
});
