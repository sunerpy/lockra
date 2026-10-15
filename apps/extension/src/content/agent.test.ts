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

function setup(start: Partial<StartAnswer> = {}, focusToken: string | null = "a".repeat(32)) {
  const sent: ContentMessage[] = [];
  let listener: ((message: unknown) => Promise<unknown> | undefined) | null = null;
  const runtime: Runtime = {
    send: async (message) => {
      sent.push(message);
      if (message.kind === "content.start")
        return { features: true, otp: false, pageLoad: false, prompt: null, ...start };
      if (message.kind === "content.focus")
        return { token: message.form === "otp" ? null : focusToken };
      return null;
    },
    listen: (handler) => {
      listener = handler;
    },
    url: (path) => `chrome-extension://ext/${path}`,
  };
  const agent = new ContentAgent({
    doc: document,
    win: window,
    runtime,
    view,
    now: () => now,
    measurer,
    trusted: () => true,
  });
  agents.push(agent);
  return {
    agent,
    sent,
    deliver: (message: unknown) => {
      if (!listener) throw new Error("no listener");
      return listener(message);
    },
  };
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
    expect(await deliver({ kind: "fill", values })).toEqual({ filled: 2 });
    expect([input("user").value, input("pass").value]).toEqual(["alice", "s3cret"]);
    input("user").value = "";
    input("pass").focus();
    expect(await deliver({ kind: "fill", values })).toEqual({ filled: 2 });
    expect(input("user").value).toBe("alice");
    agent.stop();
  });

  it("fills a code into the page's code field", async () => {
    document.body.innerHTML = `<div>${[1, 2, 3, 4, 5, 6].map((n) => `<input id="d${n}" maxlength="1">`).join("")}</div>`;
    const { agent, deliver } = setup();
    await agent.start();
    await deliver({ kind: "fill.otp", code: "123456" });
    expect([1, 2, 3, 4, 5, 6].map((n) => input(`d${n}`).value).join("")).toBe("123456");
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
