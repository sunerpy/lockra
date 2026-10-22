// What a form submitted, for the save and update prompts (plan §四 「保存与更新的检测」): the submit
// event, a click on what looks like the submit button, Enter in a password field, a typed password
// field going away with the page; and the user name of a sign-in's first step.
import type { View } from "./fields";
import { SubmitWatcher } from "./submit";

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

/** Typing, as the user does: a trusted input event. */
function type(el: HTMLInputElement, value: string): void {
  el.value = value;
  el.dispatchEvent(new InputEvent("input", { bubbles: true }));
}

let now = 0;

function watch() {
  const submitted = vi.fn();
  const username = vi.fn();
  const watcher = new SubmitWatcher(
    document,
    window,
    view,
    { submitted, username },
    () => now,
    () => true,
  );
  watcher.start();
  return { watcher, submitted, username };
}

beforeEach(() => {
  now = 1_000;
});

describe("a submitted sign-in", () => {
  it("is not a code typed into a field the page masks as a password", () => {
    // NDKEY's ADFS adapter (2026-10-20): its code field is a password field named OTP; what it
    // sends is no password to save.
    document.body.innerHTML = `<form id="f"><input id="code" name="OTP" type="password" placeholder="Please input password"><input type="submit" id="go" value="Submit"></form>`;
    const { watcher, submitted } = watch();
    type(input("code"), "123456");
    input("go").click();
    document
      .getElementById("f")
      ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    window.dispatchEvent(new Event("pagehide"));
    expect(submitted).not.toHaveBeenCalled();
    expect(watcher.watching).toBe(false);
    watcher.stop();
  });

  it("is reported on the form's submit event, with its user name and password", () => {
    document.body.innerHTML = `<form id="f"><input id="user" autocomplete="username"><input type="password" id="pass"></form>`;
    const { watcher, submitted } = watch();
    type(input("user"), "alice");
    type(input("pass"), "hunter2");
    document
      .getElementById("f")
      ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    expect(submitted).toHaveBeenCalledWith({
      username: "alice",
      password: "hunter2",
      signup: false,
    });
    // The same values again a moment later (the click, then the submit): once.
    document
      .getElementById("f")
      ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    expect(submitted).toHaveBeenCalledTimes(1);
    watcher.stop();
  });

  it("is reported on a click of the submit button of a formless sign-in, or Enter in the password", () => {
    document.body.innerHTML = `<div><input id="user" name="email"><input type="password" id="pass"><button id="go">Log in</button><a id="link" href="#">Help</a></div>`;
    const { watcher, submitted } = watch();
    type(input("user"), "bob");
    type(input("pass"), "pw1");
    document.getElementById("link")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(submitted).not.toHaveBeenCalled();
    document.getElementById("go")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(submitted).toHaveBeenLastCalledWith({ username: "bob", password: "pw1", signup: false });
    now += 5_000;
    type(input("pass"), "pw2");
    input("pass").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(submitted).toHaveBeenLastCalledWith({ username: "bob", password: "pw2", signup: false });
    watcher.stop();
  });

  it("is reported when a typed password field goes away, or the page does", () => {
    document.body.innerHTML = `<div id="box"><input id="user" autocomplete="username"><input type="password" id="pass"></div>`;
    const { watcher, submitted } = watch();
    type(input("user"), "carol");
    type(input("pass"), "spa-pass");
    document.getElementById("box")?.remove();
    watcher.check();
    expect(submitted).toHaveBeenLastCalledWith({
      username: "carol",
      password: "spa-pass",
      signup: false,
    });
    document.body.innerHTML = `<form><input id="user" autocomplete="username"><input type="password" id="pass"></form>`;
    now += 5_000;
    type(input("user"), "dave");
    type(input("pass"), "leaving");
    window.dispatchEvent(new Event("pagehide"));
    expect(submitted).toHaveBeenLastCalledWith({
      username: "dave",
      password: "leaving",
      signup: false,
    });
    watcher.stop();
  });

  it("says when it was a sign-up form", () => {
    document.body.innerHTML = `<form id="f"><input id="user" autocomplete="username"><input type="password" id="p1" autocomplete="new-password"><input type="password" id="p2" autocomplete="new-password"></form>`;
    const { watcher, submitted } = watch();
    type(input("user"), "erin");
    type(input("p1"), "Brand-new-1");
    type(input("p2"), "Brand-new-1");
    document.getElementById("f")?.dispatchEvent(new Event("submit", { bubbles: true }));
    expect(submitted).toHaveBeenCalledWith({
      username: "erin",
      password: "Brand-new-1",
      signup: true,
    });
    watcher.stop();
  });

  it("reports nothing without a typed password, nor after it stopped", () => {
    document.body.innerHTML = `<form id="f"><input id="user" autocomplete="username"><input type="password" id="pass"></form>`;
    const { watcher, submitted } = watch();
    document.getElementById("f")?.dispatchEvent(new Event("submit", { bubbles: true }));
    expect(submitted).not.toHaveBeenCalled();
    watcher.stop();
    type(input("pass"), "late");
    document.getElementById("f")?.dispatchEvent(new Event("submit", { bubbles: true }));
    expect(submitted).not.toHaveBeenCalled();
  });
});

describe("a sign-in that shows its user name", () => {
  // AWS IAM Identity Center's password step when it remembers the user (the user's Chrome,
  // 2026-10-22): the user name as text above the password, no field for it, an empty hidden one
  // outside the form.
  const AWS = `
    <form id="f">
      <div class="awsui-util-container-content">
        <p class="username awsui-util-mb-l" data-testid="test-not-you">user@example.com
          <awsui-button id="not-you-link" class="not-you"><button type="button" class="awsui-button">不是您?</button></awsui-button>
        </p>
        <label for="awsui-input-2">密码</label>
        <awsui-input><input id="awsui-input-2" type="password" autocomplete="off" class="awsui-input"></awsui-input>
        <input type="checkbox" id="awsui-checkbox-2"> <label for="awsui-checkbox-2">显示密码</label>
        <awsui-button id="password-submit-button"><button type="submit" id="go" class="awsui-button">登录</button></awsui-button>
      </div>
    </form>
    <div id="wdc_backward_compatibility"><input id="wdc_username" type="hidden"></div>`;

  it("reports the user name the page shows, the form having no field for it", () => {
    document.body.innerHTML = AWS;
    document.getElementById("f")?.addEventListener("submit", (event) => event.preventDefault());
    const { watcher, submitted } = watch();
    type(input("awsui-input-2"), "hunter2");
    document.getElementById("go")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(submitted).toHaveBeenCalledWith({
      username: "user@example.com",
      password: "hunter2",
      signup: false,
    });
    expect(submitted).toHaveBeenCalledTimes(1);
    watcher.stop();
  });

  it("reports it too when the typed password goes away with the form", () => {
    document.body.innerHTML = AWS;
    const { watcher, submitted } = watch();
    type(input("awsui-input-2"), "spa-pass");
    document.getElementById("f")?.remove();
    watcher.check();
    expect(submitted).toHaveBeenCalledWith({
      username: "user@example.com",
      password: "spa-pass",
      signup: false,
    });
    watcher.stop();
  });

  it("leaves the user name empty where the form has a field for it, empty", () => {
    // The page wants it typed: the text beside it is not the user name.
    document.body.innerHTML = `<form id="f"><p class="username">shown@example.com</p><input id="user" name="username"><input type="password" id="pass"></form>`;
    const { watcher, submitted } = watch();
    type(input("pass"), "pw-typed");
    document.getElementById("f")?.dispatchEvent(new Event("submit", { bubbles: true }));
    expect(submitted).toHaveBeenCalledWith({ username: "", password: "pw-typed", signup: false });
    watcher.stop();
  });
});

describe("the first step of a sign-in in two", () => {
  it("reports the user name it sent on", () => {
    document.body.innerHTML = `<form id="f"><input type="email" id="email" name="identifier"><button>Next</button></form>`;
    const { watcher, username, submitted } = watch();
    type(input("email"), "frank@example.com");
    document.getElementById("f")?.dispatchEvent(new Event("submit", { bubbles: true }));
    expect(username).toHaveBeenCalledWith("frank@example.com");
    expect(submitted).not.toHaveBeenCalled();
    watcher.stop();
  });
});
