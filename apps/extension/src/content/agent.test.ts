// The content script on a jsdom page with a fake runtime: the menu on focus, keys on the field,
// fills and codes from the background, the click checks, tampering, the page's own top layer,
// submissions, the code of a second step, and a fill as the page loads.
import type { Measurer, StyleFacts } from "./clickjack";
import type { View } from "./fields";
import { ContentAgent, type Runtime } from "./agent";
import type { ContentMessage, StartAnswer } from "../shared/messages";

const view: View = {
  visible: (el) =>
    el instanceof HTMLElement &&
    !el.closest("[hidden], [data-hidden]") &&
    el.style.display !== "none",
};

const CLEAN: StyleFacts = {
  opacity: "1",
  filter: "none",
  clipPath: "none",
  mixBlendMode: "normal",
  visibility: "visible",
  display: "block",
};

let now = 0;
let topmost: Element | null = null;
const agents: ContentAgent[] = [];

afterEach(() => {
  for (const agent of agents.splice(0)) agent.stop();
});

const measurer: Measurer = {
  elementsFromPoint: () => (topmost ? [topmost] : []),
  style: () => CLEAN,
};

function setup(
  start: Partial<StartAnswer> = {},
  focusToken: string | null = "a".repeat(32),
  focus?: (message: Extract<ContentMessage, { kind: "content.focus" }>) => unknown,
  more: {
    /** The frame's window (the page's own by default). */
    win?: Window;
    /** What the background does for a fill as the page loads, before it answers. */
    pageLoad?: (deliver: (message: unknown) => Promise<unknown> | undefined) => Promise<unknown>;
  } = {},
) {
  const sent: ContentMessage[] = [];
  let listener: ((message: unknown) => Promise<unknown> | undefined) | null = null;
  const deliver = (message: unknown) => {
    if (!listener) throw new Error("no listener");
    return listener(message);
  };
  const runtime: Runtime = {
    send: async (message) => {
      sent.push(message);
      if (message.kind === "content.start")
        return { features: true, otp: false, pageLoad: false, prompt: null, ...start };
      if (message.kind === "content.focus")
        return focus ? focus(message) : { token: message.form === "otp" ? null : focusToken };
      if (message.kind === "content.page_load") await more.pageLoad?.(deliver);
      return null;
    },
    listen: (handler) => {
      listener = handler;
    },
    url: (path) => `chrome-extension://ext/${path}`,
  };
  const agent = new ContentAgent({
    doc: document,
    win: more.win ?? window,
    runtime,
    view,
    now: () => now,
    measurer,
    trusted: () => true,
  });
  agents.push(agent);
  return { agent, sent, deliver };
}

function input(id: string): HTMLInputElement {
  const el = document.getElementById(id);
  if (!(el instanceof HTMLInputElement)) throw new Error(`no input #${id}`);
  return el;
}

function hosts(): HTMLElement[] {
  return Array.from(document.documentElement.children).filter(
    (el): el is HTMLElement => el instanceof HTMLDivElement && el.hasAttribute("popover"),
  );
}

const LOGIN = `<form id="f"><input id="user" autocomplete="username"><input type="password" id="pass"><button>Sign in</button></form><input id="search" type="search">`;

beforeEach(() => {
  now = 10_000;
  topmost = null;
  document.body.innerHTML = "";
  for (const el of Array.from(document.documentElement.children))
    if (el !== document.head && el !== document.body) el.remove();
});

describe("starting", () => {
  it("asks the background what waits, and without site access only takes fills", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, sent } = setup({ features: false });
    await agent.start();
    expect(sent[0]).toEqual({ kind: "content.start", top: true });
    input("user").focus();
    await Promise.resolve();
    expect(sent).toHaveLength(1);
    agent.stop();
  });

  it("starts once, however often it is injected", async () => {
    const { agent, sent } = setup();
    await agent.start();
    await agent.start();
    expect(sent.filter((m) => m.kind === "content.start")).toHaveLength(1);
    agent.stop();
  });
});

describe("the inline menu", () => {
  it("opens under a sign-in field the app has logins for, in a closed shadow root in the top layer", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, sent } = setup();
    await agent.start();
    input("user").focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(1));
    expect(sent.at(-1)).toEqual({ kind: "content.focus", form: "login" });
    const host = hosts()[0] as HTMLElement;
    expect(host.getAttribute("popover")).toBe("manual");
    expect(host.shadowRoot).toBeNull();
    expect(host.getAttribute("style")).toContain("opacity: 1 !important");
    expect(agent.menuFrame?.src).toBe(`chrome-extension://ext/inline.html#${"a".repeat(32)}`);
    // The field's mark, to open it again.
    expect(document.documentElement.querySelectorAll(":scope > div:not([popover])")).toHaveLength(
      1,
    );
    // Nothing for a field that is not a sign-in's.
    input("search").focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(0));
    expect(sent.at(-1)).toEqual({ kind: "content.closed", token: "a".repeat(32) });
    agent.stop();
  });

  it("regression: opens on a field the page focused before the script listened (autofocus)", async () => {
    // qBittorrent's sign-in (2026-10-18): `autofocus` on the user name, focused before the
    // script runs (document_idle) and asks the background; no focusin comes after.
    document.body.innerHTML = LOGIN;
    input("user").focus();
    const { agent, sent } = setup();
    await agent.start();
    await vi.waitFor(() => expect(hosts()).toHaveLength(1));
    expect(sent.filter((m) => m.kind === "content.focus")).toEqual([
      { kind: "content.focus", form: "login" },
    ]);
    agent.stop();
  });

  it("leaves a field focused before it started to the fill the start hands it to", async () => {
    // The code of the login just filled goes in by itself: a menu would fill it a second time.
    document.body.innerHTML = `<input id="code" autocomplete="one-time-code">`;
    input("code").focus();
    const otp = setup({ otp: true });
    await otp.agent.start();
    expect(otp.sent.some((m) => m.kind === "content.otp")).toBe(true);
    expect(otp.sent.some((m) => m.kind === "content.focus")).toBe(false);
    otp.agent.stop();
    // The password of the step whose user name was filled goes in by itself too.
    document.body.innerHTML = LOGIN;
    input("pass").focus();
    const step = setup({ nextStep: true });
    await step.agent.start();
    expect(step.sent.some((m) => m.kind === "content.next_step")).toBe(true);
    expect(step.sent.some((m) => m.kind === "content.focus")).toBe(false);
    step.agent.stop();
  });

  it("opens on a field focused before it started once a fill as the page loads put nothing in", async () => {
    const values = { username: "alice", password: "s3cret", fields: [], otp: null, submit: false };
    for (const fills of [true, false]) {
      document.body.innerHTML = LOGIN;
      input("user").focus();
      const { agent, sent } = setup({ pageLoad: true }, "a".repeat(32), undefined, {
        pageLoad: async (deliver) => (fills ? deliver({ kind: "fill", values }) : null),
      });
      await agent.start();
      expect(sent.some((m) => m.kind === "content.page_load")).toBe(true);
      expect(input("user").value).toBe(fills ? "alice" : "");
      expect(sent.some((m) => m.kind === "content.focus")).toBe(!fills);
      agent.stop();
    }
  });

  it("asks once for a field whose focus the background is still answering", async () => {
    // The field focused during a fill as the page loads, then found focused after it.
    document.body.innerHTML = LOGIN;
    let answer: (value: unknown) => void = () => undefined;
    const { agent, sent } = setup(
      { pageLoad: true },
      null,
      () => new Promise((r) => (answer = r)),
      {
        pageLoad: async () => input("user").focus(),
      },
    );
    await agent.start();
    expect(sent.filter((m) => m.kind === "content.focus")).toHaveLength(1);
    answer({ token: "d".repeat(32) });
    await vi.waitFor(() => expect(hosts()).toHaveLength(1));
    agent.stop();
  });

  it("in a frame, opens on the field focused before it started only while the focus is in the frame", async () => {
    // A frame keeps its focused field after the focus has gone to another frame.
    const frame = new Proxy(window, {
      get: (target, key) => {
        if (key === "top") return {};
        const value: unknown = Reflect.get(target, key);
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
    for (const focus of [false, true]) {
      document.body.innerHTML = LOGIN;
      input("user").focus();
      const hasFocus = vi.spyOn(document, "hasFocus").mockReturnValue(focus);
      const { agent, sent } = setup({}, "c".repeat(32), undefined, { win: frame });
      await agent.start();
      expect(sent[0]).toEqual({ kind: "content.start", top: false });
      expect(sent.some((m) => m.kind === "content.focus")).toBe(focus);
      agent.stop();
      hasFocus.mockRestore();
    }
  });

  it("shows the field's mark alone when the background says so, and opens the menu from it", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, sent } = setup({}, null, (message) =>
      message.explicit ? { token: "b".repeat(32) } : { token: null, mark: true },
    );
    await agent.start();
    const marks = () => document.documentElement.querySelectorAll(":scope > div:not([popover])");
    input("user").focus();
    await vi.waitFor(() => expect(marks()).toHaveLength(1));
    expect(hosts()).toHaveLength(0);
    expect(sent.at(-1)).toEqual({ kind: "content.focus", form: "login" });
    // A click on the mark asks for the menu, which opens under the field.
    (marks()[0] as HTMLElement).click();
    await vi.waitFor(() => expect(hosts()).toHaveLength(1));
    expect(sent.at(-1)).toEqual({ kind: "content.focus", form: "login", explicit: true });
    expect(agent.menuFrame?.src).toBe(`chrome-extension://ext/inline.html#${"b".repeat(32)}`);
    // The focus leaves: the menu and the mark go.
    input("search").focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(0));
    expect(marks()).toHaveLength(0);
    // The mark alone goes with the focus too.
    input("user").focus();
    await vi.waitFor(() => expect(marks()).toHaveLength(1));
    document.querySelector("button")?.focus();
    await vi.waitFor(() => expect(marks()).toHaveLength(0));
    agent.stop();
  });

  it("opens above the field where the background says the browser lists its own passwords under it, the shortcut's confirmation too", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, deliver } = setup({}, null, () => ({ token: "c".repeat(32), side: "above" }));
    await agent.start();
    const user = input("user");
    user.getBoundingClientRect = () => {
      const rect = { x: 100, y: 300, left: 100, top: 300, width: 360, height: 36 };
      return { ...rect, right: 460, bottom: 336, toJSON: () => rect };
    };
    const top = () =>
      /(?:^|; )top: ([^;]+) !important/.exec(hosts()[0]?.getAttribute("style") ?? "")?.[1];
    user.focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(1));
    await deliver({ kind: "frame.resize", token: "c".repeat(32), height: 100 });
    expect(top()).toBe("194px");
    await deliver({ kind: "frame.close", token: "c".repeat(32) });
    expect(hosts()).toHaveLength(0);
    await deliver({ kind: "menu.open", token: "d".repeat(32), side: "above" });
    expect(hosts()).toHaveLength(1);
    expect(top()).toBe("294px");
    agent.stop();
  });

  it("stays while the focus is in its frame, and goes when the focus goes elsewhere", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, sent } = setup();
    await agent.start();
    input("user").focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(1));
    agent.menuFrame?.focus();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(hosts()).toHaveLength(1);
    input("user").focus();
    document.querySelector("button")?.focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(0));
    expect(sent.at(-1)).toEqual({ kind: "content.closed", token: "a".repeat(32) });
    agent.stop();
  });

  it("offers the generator on a sign-up form with the page's rules", async () => {
    document.body.innerHTML = `<form><input id="user" autocomplete="username" value="new-user"><input type="password" id="pw" autocomplete="new-password" passwordrules="minlength: 20;" minlength="8" maxlength="64"></form>`;
    const { agent, sent } = setup();
    await agent.start();
    input("pw").focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(1));
    expect(sent.at(-1)).toEqual({
      kind: "content.focus",
      form: "signup",
      signup: { rules: "minlength: 20;", minlength: 8, maxlength: 64, username: "new-user" },
    });
    // Its user name field offers nothing.
    const asked = sent.length;
    input("user").focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(0));
    expect(sent.slice(asked).some((m) => m.kind === "content.focus")).toBe(false);
    agent.stop();
  });

  it("gives the generator the user name a set-password form keeps hidden or shows, the form having no field for it", async () => {
    // The generated login is saved with it at once.
    const passwords = `<input type="password" id="pw" autocomplete="new-password"><input type="password" id="again" autocomplete="new-password">`;
    for (const [inside, username] of [
      [`<p class="username">user@example.com</p>`, "user@example.com"],
      [`<input autocomplete="username" value="kept@example.com" hidden>`, "kept@example.com"],
    ] as const) {
      document.body.innerHTML = `<form>${inside}${passwords}</form>`;
      const { agent, sent } = setup();
      await agent.start();
      input("pw").focus();
      await vi.waitFor(() => expect(hosts()).toHaveLength(1));
      expect(sent.at(-1)).toEqual({
        kind: "content.focus",
        form: "signup",
        signup: { rules: null, minlength: null, maxlength: null, username },
      });
      agent.stop();
    }
  });

  it("moves with the arrows, fills with Enter once in the list, closes with Escape", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, sent } = setup();
    await agent.start();
    input("user").focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(1));
    topmost = hosts()[0] ?? null;
    const key = (k: string) => {
      const event = new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true });
      input("user").dispatchEvent(event);
      return event;
    };
    // Enter before the arrows is the page's (it submits the form).
    expect(key("Enter").defaultPrevented).toBe(false);
    expect(key("ArrowDown").defaultPrevented).toBe(true);
    now += 300;
    expect(key("Enter").defaultPrevented).toBe(true);
    await vi.waitFor(() => expect(sent.filter((m) => m.kind === "content.key")).toHaveLength(2));
    expect(sent.filter((m) => m.kind === "content.key")).toEqual([
      { kind: "content.key", token: "a".repeat(32), key: "ArrowDown", allowed: false },
      { kind: "content.key", token: "a".repeat(32), key: "Enter", allowed: true },
    ]);
    key("Escape");
    await vi.waitFor(() => expect(hosts()).toHaveLength(0));
    agent.stop();
  });

  it("regression: raises the menu again only for what may cover it, not for any change of the page", async () => {
    // The browser smoke test, 2026-10-15: every batch of the page's changes raised the menu (out of
    // the top layer and back), after which Chrome's IntersectionObserver v2 kept reporting the frame
    // unseen, and the next Enter or click was refused: on any page that changes, no fill went in.
    vi.useFakeTimers();
    try {
      document.body.innerHTML = LOGIN;
      const { agent, deliver } = setup();
      await agent.start();
      input("user").focus();
      await vi.waitFor(() => expect(hosts()).toHaveLength(1));
      // On to the next field while the menu is up: from now on the page is watched.
      input("pass").focus();
      await vi.waitFor(() => expect(hosts()).toHaveLength(1));
      topmost = hosts()[0] ?? null;
      const verify = () =>
        deliver({
          kind: "frame.verify",
          token: "a".repeat(32),
          point: null,
          trusted: true,
          visible: true,
        });
      now += 300;
      expect(await verify()).toEqual({ ok: true });
      // The page adds content; the watcher's next look comes and goes: the menu stays as it was.
      document.body.append(document.createElement("p"));
      await vi.advanceTimersByTimeAsync(1_000);
      expect(await verify()).toEqual({ ok: true });
      // A dialog the page opens may cover it: then it goes back on top and waits 300 ms again.
      const dialog = document.createElement("dialog");
      document.body.append(dialog);
      dialog.setAttribute("open", "");
      await vi.advanceTimersByTimeAsync(1_000);
      expect(await verify()).toEqual({ ok: false, reason: "too_soon" });
    } finally {
      vi.useRealTimers();
    }
  });

  it("refuses a click too soon, covered, or after the page touched the host; and goes when tampered with", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, sent, deliver } = setup();
    await agent.start();
    input("user").focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(1));
    const token = "a".repeat(32);
    const host = hosts()[0] as HTMLElement;
    topmost = host;
    expect(
      await deliver({
        kind: "frame.verify",
        token,
        point: { x: 5, y: 5 },
        trusted: true,
        visible: true,
      }),
    ).toEqual({ ok: false, reason: "too_soon" });
    now += 300;
    expect(
      await deliver({
        kind: "frame.verify",
        token,
        point: { x: 5, y: 5 },
        trusted: true,
        visible: true,
      }),
    ).toEqual({ ok: true });
    topmost = document.body;
    expect(
      await deliver({
        kind: "frame.verify",
        token,
        point: { x: 5, y: 5 },
        trusted: true,
        visible: null,
      }),
    ).toEqual({ ok: false, reason: "covered" });
    // The page opens its own popover: the menu goes back on top, and waits 300 ms again.
    topmost = host;
    const theirs = document.createElement("div");
    document.body.append(theirs);
    // A toggle event does not bubble: the capture phase on the document still sees it.
    theirs.dispatchEvent(Object.assign(new Event("toggle"), { newState: "open" }));
    expect(
      await deliver({ kind: "frame.verify", token, point: null, trusted: true, visible: null }),
    ).toEqual({ ok: false, reason: "too_soon" });
    // A token it does not know.
    expect(
      await deliver({
        kind: "frame.verify",
        token: "b".repeat(32),
        point: null,
        trusted: true,
        visible: null,
      }),
    ).toEqual({ ok: false, reason: "tampered" });
    host.style.opacity = "0";
    await vi.waitFor(() => expect(hosts()).toHaveLength(0));
    expect(sent.at(-1)).toEqual({ kind: "content.closed", token });
    agent.stop();
  });

  it("resizes and closes as the background says", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, deliver } = setup();
    await agent.start();
    input("pass").focus();
    await vi.waitFor(() => expect(hosts()).toHaveLength(1));
    await deliver({ kind: "frame.resize", token: "a".repeat(32), height: 180 });
    expect(hosts()[0]?.getAttribute("style")).toContain("height: 180px !important");
    await deliver({ kind: "frame.close", token: "a".repeat(32) });
    expect(hosts()).toHaveLength(0);
    agent.stop();
  });
});

describe("what the background sends", () => {
  it("fills the focused form, or the page's sign-in form when no field has the focus", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, deliver } = setup({}, null);
    await agent.start();
    const values = { username: "alice", password: "s3cret", fields: [], otp: null, submit: false };
    const both = { filled: 2, password: true, code: false };
    expect(await deliver({ kind: "fill", values })).toEqual(both);
    expect([input("user").value, input("pass").value]).toEqual(["alice", "s3cret"]);
    input("user").value = "";
    input("pass").focus();
    expect(await deliver({ kind: "fill", values })).toEqual(both);
    expect(input("user").value).toBe("alice");
    agent.stop();
  });

  it("opens the menu the background answers for a code field", async () => {
    document.body.innerHTML = `<form><input type="tel" id="totpPin" name="totpPin" pattern="[0-9 ]*"></form>`;
    const token = "c".repeat(32);
    const { agent, sent } = setup({}, null, (message) =>
      message.form === "otp" ? { token } : { token: null },
    );
    await agent.start();
    input("totpPin").focus();
    await vi.waitFor(() =>
      expect(sent.find((m) => m.kind === "content.focus")).toMatchObject({ form: "otp" }),
    );
    await vi.waitFor(() => expect(agent.menuFrame?.src).toContain(`#${token}`));
  });

  it("fills a code into the page's code field", async () => {
    document.body.innerHTML = `<div>${[1, 2, 3, 4, 5, 6].map((n) => `<input id="d${n}" maxlength="1">`).join("")}</div>`;
    const { agent, deliver } = setup();
    await agent.start();
    await deliver({ kind: "fill.otp", code: "123456" });
    expect([1, 2, 3, 4, 5, 6].map((n) => input(`d${n}`).value).join("")).toBe("123456");
    agent.stop();
  });

  it("puts a code the user asked for into the focused field where it can tell no code field, never a password", async () => {
    document.body.innerHTML = `<input id="mystery" name="answer"><input id="secret" type="password">`;
    const { agent, deliver } = setup();
    await agent.start();
    input("mystery").focus();
    // The next step's code goes only into a code field.
    expect(await deliver({ kind: "fill.otp", code: "246810" })).toEqual({ code: false });
    expect(input("mystery").value).toBe("");
    expect(await deliver({ kind: "fill.otp", code: "246810", focused: true })).toEqual({
      code: true,
    });
    expect(input("mystery").value).toBe("246810");
    input("secret").focus();
    expect(await deliver({ kind: "fill.otp", code: "135790", focused: true })).toEqual({
      code: false,
    });
    expect(input("secret").value).toBe("");
    agent.stop();
  });

  it("puts a code the user asked for into a password field alone in its form, never one the page names a password", async () => {
    // A code step Lockra cannot tell from a password step: the user knows, and asks.
    document.body.innerHTML = `
      <form><input id="pin" type="password" name="pin2"><button>OK</button></form>
      <form><input id="named" type="password" autocomplete="current-password"></form>`;
    const { agent, deliver } = setup();
    await agent.start();
    input("pin").focus();
    expect(await deliver({ kind: "fill.otp", code: "246810" })).toEqual({ code: false });
    expect(input("pin").value).toBe("");
    expect(await deliver({ kind: "fill.otp", code: "246810", focused: true })).toEqual({
      code: true,
    });
    expect(input("pin").value).toBe("246810");
    input("named").focus();
    expect(await deliver({ kind: "fill.otp", code: "135790", focused: true })).toEqual({
      code: false,
    });
    expect(input("named").value).toBe("");
    agent.stop();
  });

  it("shows a prompt at the top right, and a confirmation under the focused field", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, deliver } = setup({}, null);
    await agent.start();
    await deliver({ kind: "prompt.open", token: "c".repeat(32) });
    expect(hosts()).toHaveLength(1);
    expect(hosts()[0]?.getAttribute("style")).toContain("width: 360px !important");
    input("user").focus();
    await deliver({ kind: "menu.open", token: "d".repeat(32) });
    expect(hosts()).toHaveLength(2);
    expect(agent.menuFrame?.src).toContain("#" + "d".repeat(32));
    // Opened by the shortcut: Enter confirms at once.
    topmost = agent.menuHost;
    now += 300;
    const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
    input("user").dispatchEvent(enter);
    expect(enter.defaultPrevented).toBe(true);
    // Not a message of ours.
    expect(await deliver({ kind: "nonsense" })).toBeUndefined();
    agent.stop();
  });
});

describe("the page's own work", () => {
  it("reports a submission and a first step's user name", async () => {
    document.body.innerHTML = LOGIN;
    const { agent, sent } = setup();
    await agent.start();
    const user = input("user");
    user.value = "alice";
    user.dispatchEvent(new InputEvent("input", { bubbles: true }));
    const pass = input("pass");
    pass.value = "hunter2";
    pass.dispatchEvent(new InputEvent("input", { bubbles: true }));
    document.getElementById("f")?.dispatchEvent(new Event("submit", { bubbles: true }));
    expect(sent.at(-1)).toEqual({
      kind: "content.submitted",
      username: "alice",
      password: "hunter2",
      signup: false,
    });
    document.body.innerHTML = `<form id="g"><input type="email" id="email" name="identifier"></form>`;
    input("email").value = "bob@example.com";
    document.getElementById("g")?.dispatchEvent(new Event("submit", { bubbles: true }));
    expect(sent.at(-1)).toEqual({ kind: "content.username", username: "bob@example.com" });
    agent.stop();
  });

  it("reports the user name a password page shows, the form having no field for it", async () => {
    // A sign-in that remembers the user (AWS IAM Identity Center, 2026-10-22).
    document.body.innerHTML = `<form id="f"><p class="username">user@example.com <button type="button">Not you?</button></p><label for="pass">Password</label><input type="password" id="pass"><button type="submit">Sign in</button></form>`;
    const { agent, sent } = setup();
    await agent.start();
    const pass = input("pass");
    pass.value = "hunter2";
    pass.dispatchEvent(new InputEvent("input", { bubbles: true }));
    document.getElementById("f")?.dispatchEvent(new Event("submit", { bubbles: true }));
    expect(sent.at(-1)).toEqual({
      kind: "content.submitted",
      username: "user@example.com",
      password: "hunter2",
      signup: false,
    });
    agent.stop();
  });

  it("looks for the code field of a second step, and watches for one that comes later", async () => {
    document.body.innerHTML = `<p>loading</p>`;
    const { agent, sent } = setup({ otp: true });
    await agent.start();
    expect(sent.some((m) => m.kind === "content.otp")).toBe(false);
    const field = document.createElement("input");
    field.autocomplete = "one-time-code";
    document.body.append(field);
    await vi.waitFor(() => expect(sent.some((m) => m.kind === "content.otp")).toBe(true), {
      timeout: 2_000,
    });
    agent.stop();
  });

  it("watches the same page for the next step: the password after the user name, the code after the password", async () => {
    // A sign-in that changes the page without loading another (2026-10-16).
    document.body.innerHTML = `<form id="step"><input id="email" type="email" autocomplete="username"><button>Next</button></form>`;
    const { agent, sent, deliver } = setup({}, null);
    await agent.start();
    const values = {
      username: "me@example.com",
      password: "s3cret",
      fields: [],
      otp: "123456",
      submit: false,
    };
    input("email").focus();
    expect(await deliver({ kind: "fill", values })).toEqual({
      filled: 1,
      password: false,
      code: false,
    });
    // The page goes on: a password field takes the user name's place.
    document.body.innerHTML = `<form id="step"><input id="pass" type="password"><button>Sign in</button></form>`;
    await vi.waitFor(() => expect(sent.some((m) => m.kind === "content.next_step")).toBe(true), {
      timeout: 2_000,
    });
    expect(sent.filter((m) => m.kind === "content.next_step")).toHaveLength(1);
    input("pass").focus();
    expect(await deliver({ kind: "fill", values })).toEqual({
      filled: 1,
      password: true,
      code: false,
    });
    // Then the code's field comes: asked for once it shows.
    document.body.innerHTML = `<form id="step"><input id="code" autocomplete="one-time-code"><button>Verify</button></form>`;
    await vi.waitFor(() => expect(sent.some((m) => m.kind === "content.otp")).toBe(true), {
      timeout: 2_000,
    });
    agent.stop();
  });

  it("puts the user name into a field Lockra cannot tell when the user chose it, then looks for the password's step", async () => {
    // Pages Lockra has no rule for (the user's request, 2026-10-22): a field that names nothing,
    // with Lockra's menu opened on it by the menu shortcut.
    document.body.innerHTML = `<form id="step"><input id="who" type="text"><button>Next</button></form>`;
    const { agent, sent, deliver } = setup({}, null);
    await agent.start();
    input("who").focus();
    expect(await deliver({ kind: "menu.open", token: "d".repeat(32) })).toEqual({ opened: true });
    expect(hosts()).toHaveLength(1);
    const values = {
      username: "me@example.com",
      password: "s3cret",
      fields: [],
      otp: null,
      submit: true,
    };
    expect(await deliver({ kind: "fill", values })).toEqual({
      filled: 1,
      password: false,
      code: false,
    });
    expect(input("who").value).toBe("me@example.com");
    // The page goes on to its password: asked for once it shows.
    document.body.innerHTML = `<form id="step"><input id="pass" type="password"><button>Sign in</button></form>`;
    await vi.waitFor(() => expect(sent.some((m) => m.kind === "content.next_step")).toBe(true), {
      timeout: 2_000,
    });
    agent.stop();
  });

  it("the fill shortcut in a field Lockra cannot tell puts the user name there; nothing without a field", async () => {
    document.body.innerHTML = `<div><input id="who"></div>`;
    const { agent, deliver } = setup({}, null);
    await agent.start();
    const values = { username: "alice", password: "s3cret", fields: [], otp: null, submit: false };
    // No field has the focus: nothing to fill.
    expect(await deliver({ kind: "fill", values })).toEqual({
      filled: 0,
      password: false,
      code: false,
    });
    // The menu needs a field to open on.
    expect(await deliver({ kind: "menu.open", token: "d".repeat(32) })).toEqual({ opened: false });
    expect(hosts()).toHaveLength(0);
    input("who").focus();
    expect(await deliver({ kind: "fill", values })).toEqual({
      filled: 1,
      password: false,
      code: false,
    });
    expect(input("who").value).toBe("alice");
    agent.stop();
  });

  it("looks for the password of a step the background says is next, and sends the code's step when told", async () => {
    document.body.innerHTML = `<form id="f"><input id="pass" type="password"><button id="go">Sign in</button></form>`;
    const next = setup({ nextStep: true });
    await next.agent.start();
    await vi.waitFor(
      () => expect(next.sent.some((m) => m.kind === "content.next_step")).toBe(true),
      { timeout: 2_000 },
    );
    next.agent.stop();
    // The code's step, sent once its code is in when the app says so.
    document.body.innerHTML = `<form id="f"><input id="code" autocomplete="one-time-code"><button id="verify">Verify</button></form>`;
    const { agent, deliver } = setup();
    await agent.start();
    const verified = vi.fn((event: Event) => event.preventDefault());
    document.getElementById("verify")?.addEventListener("click", verified);
    await deliver({ kind: "fill.otp", code: "654321", submit: true });
    expect(input("code").value).toBe("654321");
    await vi.waitFor(() => expect(verified).toHaveBeenCalledTimes(1));
    // Not told: filled, not sent.
    input("code").value = "";
    await deliver({ kind: "fill.otp", code: "111111", submit: false });
    await new Promise((resolve) => setTimeout(resolve, 120));
    expect(verified).toHaveBeenCalledTimes(1);
    agent.stop();
  });

  it("asks for a fill as the page loads only where there is a sign-in form", async () => {
    document.body.innerHTML = LOGIN;
    const first = setup({ pageLoad: true });
    await first.agent.start();
    expect(first.sent.at(-1)).toEqual({ kind: "content.page_load" });
    first.agent.stop();
    document.body.innerHTML = `<input id="search" type="search">`;
    const second = setup({ pageLoad: true });
    await second.agent.start();
    expect(second.sent.some((m) => m.kind === "content.page_load")).toBe(false);
    second.agent.stop();
  });
});
