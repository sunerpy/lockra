// @vitest-environment node
// The background end to end on fakes: a fake Lockra behind the native port, a fake browser with tabs,
// frames and content scripts. The inline menu, a fill through it with the click checks, the save and
// update prompts, the code of a second step, the shortcut and its round of accounts, the popup, and
// site access.
import { FakeApp } from "../test/fake-app";
import { FakeBrowser, type FakePortEnd } from "../test/fake-browser";
import { ManualClock } from "../test/clock";
import { generateKeyPair, type KeyPair } from "../noise/crypto";
import { Connection, type Keys, type PairingRecord } from "./connection";
import { Controller } from "./controller";
import { SelfUpdate } from "./update";
import type { BackgroundMessage, InlineView, PopupEvent, PopupState } from "../shared/messages";
import { AVOID_BROWSER_LIST, MENU_ON_FOCUS } from "../shared/prefs";

class MemoryKeys implements Keys {
  pair: KeyPair | null = null;
  record: PairingRecord | null = null;
  async staticPair(): Promise<KeyPair> {
    this.pair ??= await generateKeyPair();
    return this.pair;
  }
  async pairing(): Promise<PairingRecord | null> {
    return this.record;
  }
  async setPairing(record: PairingRecord | null): Promise<void> {
    this.record = record;
  }
}

const ITEM = "5c0fe1a2-0b3c-4d5e-8f60-000000000001";
const ITEM2 = "5c0fe1a2-0b3c-4d5e-8f60-000000000002";

function summary(id: string, change: Record<string, unknown> = {}) {
  return {
    item_id: id,
    title: "GitHub",
    username: id === ITEM ? "octocat" : "octocat-work",
    email: "",
    has_password: true,
    has_totp: true,
    protection: "fill",
    favorite: false,
    downgrade: false,
    matches: true,
    ...change,
  };
}

const POLICY = {
  fill_otp: true,
  copy_otp: true,
  auto_submit: false,
  confirm_fill: false,
  cross_origin_frames: "never",
  on_page_load: false,
};

function queryAnswer(items: unknown[], change: Record<string, unknown> = {}) {
  return {
    site: "example.com",
    level: "host",
    items,
    never_fill: false,
    never_save: false,
    cross_origin: false,
    policy: POLICY,
    save: null,
    ...change,
  };
}

/** `folder`: the version in Lockra's folder, when the extension was loaded from it. */
async function setup(options: { folder?: string } = {}) {
  const app = await FakeApp.start();
  const browser = new FakeBrowser();
  const clock = new ManualClock();
  const keys = new MemoryKeys();
  const connection = new Connection({
    connectNative: app.connect,
    keys,
    clock,
    client: async () => ({
      browser: "chrome",
      label: "",
      extension: browser.runtime.id,
      version: "0.10.0",
      page_access: browser.granted,
    }),
  });
  const folder = { version: options.folder };
  const update = new SelfUpdate({
    api: browser,
    loaded: "0.10.0",
    fetch: async () => {
      if (folder.version === undefined) throw new TypeError("Failed to fetch");
      return { ok: true, json: async () => ({ version: folder.version }) };
    },
  });
  const controller = new Controller({
    api: browser,
    connection,
    clock,
    extensionVersion: "0.10.0",
    update,
  });
  controller.install();
  connection.start();
  await vi.waitFor(() => expect(connection.view.link).toBe("ready"));
  await connection.pair();
  await vi.waitFor(() => expect(connection.view.pairing.stage).toBe("waiting"));
  const pairing = await app.approve();
  await vi.waitFor(() => expect(connection.view.session).toBe("open"));
  app.handlers.query = (request) =>
    request.purpose === "save"
      ? queryAnswer([summary(ITEM)], {
          save: {
            action: request.username === "octocat" ? "update" : "save",
            item_id: request.username === "octocat" ? ITEM : null,
          },
        })
      : queryAnswer([summary(ITEM), summary(ITEM2)]);
  app.handlers.fill = (request) => ({
    item_id: request.item_id,
    username: request.item_id === ITEM ? "octocat" : "octocat-work",
    password: "s3cret",
    fields: [],
    otp: null,
    submit: false,
  });
  const contentGot: BackgroundMessage[] = [];
  let verify = { ok: true };
  /** What the page's content script answers a fill: what went in. */
  const filled = { value: { filled: 2, password: true, code: false } };
  /** What it answers a code: whether it went in. */
  const codeIn = { value: { code: true } };
  /** What it answers a frame to show at the focused field: whether a field had the focus. */
  const menuIn = { value: { opened: true } };
  browser.content.set("1:0", (message) => {
    contentGot.push(message as BackgroundMessage);
    const m = message as BackgroundMessage;
    if (m.kind === "frame.verify") return verify;
    if (m.kind === "fill") return filled.value;
    if (m.kind === "fill.otp") return codeIn.value;
    if (m.kind === "menu.open") return menuIn.value;
    return undefined;
  });
  return {
    filled,
    codeIn,
    menuIn,
    app,
    browser,
    clock,
    keys,
    connection,
    controller,
    pairing,
    contentGot,
    folder,
    refuseClicks: () => {
      verify = { ok: false, reason: "covered" } as typeof verify;
    },
  };
}

type Context = Awaited<ReturnType<typeof setup>>;

/** The inline frame `token` connects from frame 5 under the page, as the browser reports it
 *  (`listed`: webNavigation lists it, as Firefox does; Chromium leaves the extension's own frames
 *  out, the browser smoke test found). */
function inlineFrame(
  context: Context,
  token: string,
  frameId = 5,
  parent = 0,
  listed = true,
): FakePortEnd {
  const { browser } = context;
  const frames = browser.frames.get(1) ?? [];
  if (listed && !frames.some((f) => f.frameId === frameId)) {
    frames.push({
      frameId,
      parentFrameId: parent,
      url: browser.runtime.getURL(`inline.html#${token}`),
      documentId: `doc-${frameId}`,
    });
  }
  const port = browser.connectPort("lockra-inline", {
    id: browser.runtime.id,
    tab: { id: 1, url: browser.openTabs[0]?.url },
    frameId,
    url: browser.runtime.getURL(`inline.html#${token}`),
  });
  port.send({ kind: "hello", token });
  return port;
}

function lastView(port: FakePortEnd): InlineView | undefined {
  const views = port.received.filter(
    (m): m is { kind: "view"; view: InlineView } => (m as { kind: string }).kind === "view",
  );
  return views.at(-1)?.view;
}

describe("the inline menu", () => {
  it("regression: gets its view where webNavigation does not list the extension's own frame", async () => {
    // The browser smoke test, 2026-10-15: Chrome's webNavigation.getAllFrames has no entry for the
    // inline frame, so the parent check refused every menu and the menu stayed 0 px high.
    const context = await setup();
    const answer = (await context.browser.fromContent({
      kind: "content.focus",
      form: "login",
    })) as {
      token: string;
    };
    const frame = inlineFrame(context, answer.token, 5, 0, false);
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("list"));
    // Every view says the background's build, which the frame compares with its own.
    expect(frame.received.filter((m) => (m as { kind: string }).kind === "view")).toEqual(
      frame.received
        .filter((m) => (m as { kind: string }).kind === "view")
        .map((m) => ({ ...(m as object), build: LOCKRA_BUILD })),
    );
    // A token no content script handed out still gets nothing.
    const stranger = inlineFrame(context, "0".repeat(32), 6, 0, false);
    await vi.waitFor(() => expect(stranger.closed).toBe(true));
    expect(lastView(stranger)).toBeUndefined();
  });

  it("opens for a sign-in field with the app's matches, and fills the chosen login after the click checks", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    const answer = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    expect(answer.token).toMatch(/^[0-9a-f]{32}$/);
    expect(app.requests.at(-1)).toMatchObject({
      op: "query",
      purpose: "fill",
      frame_url: "https://www.example.com/login",
      top_url: "https://www.example.com/login",
    });
    const frame = inlineFrame(context, answer.token);
    await vi.waitFor(() =>
      expect(lastView(frame)).toEqual({
        view: "list",
        site: "example.com",
        items: [
          { title: "GitHub", username: "octocat", downgrade: false },
          { title: "GitHub", username: "octocat-work", downgrade: false },
        ],
        active: 0,
        // Two logins or more: the menu may be searched.
        search: "",
      }),
    );
    frame.send({ kind: "visible", visible: true });
    frame.send({
      kind: "action",
      action: { type: "choose", index: 1 },
      point: { x: 20, y: 60 },
      trusted: true,
    });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "fill")).toBe(true));
    const verify = contentGot.find((m) => m.kind === "frame.verify");
    expect(verify).toEqual({
      kind: "frame.verify",
      token: answer.token,
      point: { x: 20, y: 60 },
      trusted: true,
      visible: true,
    });
    expect(app.requests.at(-1)).toMatchObject({
      op: "fill",
      item_id: ITEM2,
      trigger: "inline_menu",
      allow_http: false,
    });
    expect(contentGot.find((m) => m.kind === "fill")).toEqual({
      kind: "fill",
      values: {
        username: "octocat-work",
        password: "s3cret",
        fields: [],
        otp: null,
        submit: false,
      },
    });
    // The fill went to the frame's document, and the menu closed.
    expect(browser.sent.find((s) => (s.message as { kind: string }).kind === "fill")).toMatchObject(
      { tabId: 1, frameId: 0, documentId: "doc-0" },
    );
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "frame.close")).toBe(true));
  });

  it("offers the field's mark alone when the menu is not to open on focus, and the menu from the mark", async () => {
    // 2026-10-16, the user: the menu and the browser's own list of saved passwords overlapped.
    const { browser } = await setup();
    await browser.storage.local.set({ [MENU_ON_FOCUS]: false });
    expect(await browser.fromContent({ kind: "content.focus", form: "login" })).toEqual({
      token: null,
      mark: true,
    });
    const asked = (await browser.fromContent({
      kind: "content.focus",
      form: "login",
      explicit: true,
    })) as { token: string | null };
    expect(typeof asked.token).toBe("string");
    // On again: the menu opens as the field gets the focus.
    await browser.storage.local.set({ [MENU_ON_FOCUS]: true });
    const focused = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string | null;
    };
    expect(typeof focused.token).toBe("string");
  });

  it("opens the menu above the field, clear of the browser's own list of passwords, unless the options page says otherwise", async () => {
    // 2026-10-16, the user: Chrome lists the passwords it keeps under the field, whatever its
    // offer to save says, right where the menu opened.
    const { browser } = await setup();
    expect(await browser.fromContent({ kind: "content.focus", form: "login" })).toMatchObject({
      side: "above",
    });
    await browser.storage.local.set({ [AVOID_BROWSER_LIST]: false });
    expect(await browser.fromContent({ kind: "content.focus", form: "login" })).toMatchObject({
      side: "below",
    });
  });

  it("shows nothing to a frame without the token, or one that is not under the page's frame", async () => {
    const context = await setup();
    const { browser } = context;
    const answer = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    const stranger = inlineFrame(context, "00000000000000000000000000000000", 6);
    await vi.waitFor(() => expect(stranger.closed).toBe(true));
    expect(lastView(stranger)).toBeUndefined();
    // The right token from a frame the page put elsewhere (under frame 6, not the page's frame).
    browser.frames
      .get(1)
      ?.push({ frameId: 8, parentFrameId: 6, url: "https://ads.example/", documentId: "doc-8" });
    const misplaced = inlineFrame(context, answer.token, 9, 8);
    await vi.waitFor(() => expect(misplaced.closed).toBe(true));
    expect(lastView(misplaced)).toBeUndefined();
  });

  it("closes without filling when a click fails the checks", async () => {
    const context = await setup();
    const { browser, app, contentGot, refuseClicks } = context;
    const answer = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    const frame = inlineFrame(context, answer.token);
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("list"));
    refuseClicks();
    frame.send({
      kind: "action",
      action: { type: "choose", index: 0 },
      point: { x: 1, y: 1 },
      trusted: true,
    });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "frame.close")).toBe(true));
    expect(app.requests.some((r) => r.op === "fill")).toBe(false);
    // A frame occluded by its own IntersectionObserver's account is refused too, whatever the page says.
    const again = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    const second = inlineFrame(context, again.token, 7);
    await vi.waitFor(() => expect(lastView(second)?.view).toBe("list"));
    second.send({ kind: "visible", visible: false });
    second.send({
      kind: "action",
      action: { type: "choose", index: 0 },
      point: { x: 1, y: 1 },
      trusted: true,
    });
    await vi.waitFor(() => expect(second.closed).toBe(true));
    expect(app.requests.some((r) => r.op === "fill")).toBe(false);
  });

  it("asks first when the policy says so, and warns before filling an https login into an http page", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    app.handlers.query = () =>
      queryAnswer([summary(ITEM, { downgrade: true })], {
        policy: { ...POLICY, confirm_fill: true },
      });
    const answer = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    const frame = inlineFrame(context, answer.token);
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("list"));
    frame.send({
      kind: "action",
      action: { type: "choose", index: 0 },
      point: null,
      trusted: true,
    });
    await vi.waitFor(() =>
      expect(lastView(frame)).toEqual({
        view: "confirm",
        item: { title: "GitHub", username: "octocat", downgrade: true },
      }),
    );
    frame.send({ kind: "action", action: { type: "fill" }, point: null, trusted: true });
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("http"));
    expect(app.requests.some((r) => r.op === "fill")).toBe(false);
    frame.send({ kind: "action", action: { type: "allow_http" }, point: null, trusted: true });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "fill")).toBe(true));
    expect(app.requests.at(-1)).toMatchObject({ op: "fill", item_id: ITEM, allow_http: true });
  });

  it("is not offered for a frame of another site unless the policy asks, nor on a never-fill site", async () => {
    const context = await setup();
    const { browser, app } = context;
    app.handlers.query = () => queryAnswer([summary(ITEM)], { cross_origin: true });
    expect(await browser.fromContent({ kind: "content.focus", form: "login" })).toEqual({
      token: null,
    });
    app.handlers.query = () => queryAnswer([], { never_fill: true });
    expect(await browser.fromContent({ kind: "content.focus", form: "login" })).toEqual({
      token: null,
    });
    app.handlers.query = () =>
      queryAnswer([summary(ITEM)], {
        cross_origin: true,
        policy: { ...POLICY, cross_origin_frames: "ask" },
      });
    expect(
      (
        (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
          token: string | null;
        }
      ).token,
    ).not.toBeNull();
  });

  it("offers a generated password on a sign-up form, saves it as a new login and fills both fields", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    let made = 0;
    app.handlers.generate = (request) => ({
      password: `Generated-${++made}`,
      username: "alice",
      item_id: request.save ? ITEM : null,
    });
    app.handlers.save = () => ({ item_id: ITEM2 });
    const answer = (await browser.fromContent({
      kind: "content.focus",
      form: "signup",
      signup: { rules: "minlength: 20;", minlength: 8, maxlength: 64, username: "new-user" },
    })) as { token: string };
    expect(app.requests.at(-1)).toMatchObject({
      op: "generate",
      rules: "minlength: 20;",
      minlength: 8,
      maxlength: 64,
      save: null,
    });
    const frame = inlineFrame(context, answer.token);
    await vi.waitFor(() =>
      expect(lastView(frame)).toEqual({
        view: "signup",
        site: "www.example.com",
        password: "Generated-1",
        username: "new-user",
        rules: true,
      }),
    );
    frame.send({ kind: "action", action: { type: "regenerate" }, point: null, trusted: true });
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ password: "Generated-2" }));
    frame.send({ kind: "action", action: { type: "use" }, point: null, trusted: true });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "fill")).toBe(true));
    expect(app.requests.at(-1)).toMatchObject({
      op: "save",
      username: "new-user",
      password: "Generated-2",
      title: "",
      group: null,
    });
    expect(contentGot.find((m) => m.kind === "fill")).toEqual({
      kind: "fill",
      values: {
        username: "new-user",
        password: "Generated-2",
        fields: [],
        otp: null,
        submit: false,
        newPassword: true,
      },
    });
  });
});

describe("a sign-up form's generated password", () => {
  async function signupMenu(context: Context, username = "new-user"): Promise<FakePortEnd> {
    const answer = (await context.browser.fromContent({
      kind: "content.focus",
      form: "signup",
      signup: { rules: "minlength: 20;", minlength: 8, maxlength: 64, username },
    })) as { token: string };
    const frame = inlineFrame(context, answer.token);
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("signup"));
    return frame;
  }

  it("is the same in every menu of the form, the confirm field's too, and a new one only on request", async () => {
    // 2026-10-16, the user: the confirm field's menu offered another password than the first's.
    const context = await setup();
    const { app } = context;
    let made = 0;
    app.handlers.generate = () => ({
      password: `Generated-${++made}`,
      username: "",
      item_id: null,
    });
    const first = await signupMenu(context);
    expect(lastView(first)).toMatchObject({ password: "Generated-1" });
    first.send({ kind: "action", action: { type: "close" }, point: null, trusted: true });
    const confirm = await signupMenu(context);
    expect(lastView(confirm)).toMatchObject({ password: "Generated-1", username: "new-user" });
    expect(app.requests.filter((r) => r.op === "generate")).toHaveLength(1);
    // The refresh makes a new one, which the form's next menu offers.
    confirm.send({ kind: "action", action: { type: "regenerate" }, point: null, trusted: true });
    await vi.waitFor(() => expect(lastView(confirm)).toMatchObject({ password: "Generated-2" }));
    confirm.send({ kind: "action", action: { type: "close" }, point: null, trusted: true });
    expect(lastView(await signupMenu(context))).toMatchObject({ password: "Generated-2" });
    // Five minutes later the form gets a new one.
    context.clock.advance(5 * 60_000);
    expect(lastView(await signupMenu(context))).toMatchObject({ password: "Generated-3" });
  });

  it("saves one login however often it is used, and fills every new-password field each time", async () => {
    const context = await setup();
    const { app, contentGot } = context;
    let made = 0;
    app.handlers.generate = () => ({
      password: `Generated-${++made}`,
      username: "",
      item_id: null,
    });
    app.handlers.save = () => ({ item_id: ITEM2 });
    app.handlers.update = () => ({});
    const fills = () => contentGot.filter((m) => m.kind === "fill");
    const first = await signupMenu(context);
    first.send({ kind: "action", action: { type: "use" }, point: null, trusted: true });
    await vi.waitFor(() => expect(fills()).toHaveLength(1));
    const again = await signupMenu(context);
    again.send({ kind: "action", action: { type: "use" }, point: null, trusted: true });
    await vi.waitFor(() => expect(fills()).toHaveLength(2));
    expect(app.requests.filter((r) => r.op === "save")).toHaveLength(1);
    expect(fills()[1]).toMatchObject({
      values: { password: "Generated-1", newPassword: true },
    });
    // Another one after that changes the saved login's password: still no second login.
    const third = await signupMenu(context);
    expect(lastView(third)).toMatchObject({ password: "Generated-1" });
    third.send({ kind: "action", action: { type: "regenerate" }, point: null, trusted: true });
    await vi.waitFor(() => expect(lastView(third)).toMatchObject({ password: "Generated-2" }));
    third.send({ kind: "action", action: { type: "use" }, point: null, trusted: true });
    await vi.waitFor(() => expect(fills()).toHaveLength(3));
    expect(app.requests.filter((r) => r.op === "save")).toHaveLength(1);
    expect(app.requests.findLast((r) => r.op === "update")).toMatchObject({
      item_id: ITEM2,
      password: "Generated-2",
    });
    expect(fills()[2]).toMatchObject({ values: { password: "Generated-2", newPassword: true } });
  });

  it("copies the password the menu shows through Lockra, which clears the clipboard later", async () => {
    // 2026-10-16, the user: the menu should copy it as well.
    const context = await setup();
    const { app } = context;
    app.handlers.generate = () => ({ password: "Generated-1", username: "", item_id: null });
    app.handlers.copy_generated = () => ({});
    const frame = await signupMenu(context);
    frame.send({
      kind: "action",
      action: { type: "copy" },
      point: { x: 300, y: 30 },
      trusted: true,
    });
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ copied: true }));
    expect(app.requests.at(-1)).toMatchObject({
      op: "copy_generated",
      frame_url: "https://www.example.com/login",
      password: "Generated-1",
    });
    // The menu stays: the password can still be used.
    expect(lastView(frame)).toMatchObject({ view: "signup", password: "Generated-1" });
  });
});

describe("saving what a form submitted", () => {
  it("asks to save a new login at the top right, with the user name of the step before", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    app.handlers.save = () => ({ item_id: ITEM2 });
    await browser.fromContent({ kind: "content.username", username: "alice" });
    await browser.fromContent({
      kind: "content.submitted",
      username: "",
      password: "hunter2",
      signup: false,
    });
    expect(app.requests.at(-1)).toMatchObject({
      op: "query",
      purpose: "save",
      username: "alice",
      password: "hunter2",
    });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "prompt.open")).toBe(true));
    const open = contentGot.find((m) => m.kind === "prompt.open") as { token: string };
    const frame = inlineFrame(context, open.token);
    await vi.waitFor(() =>
      expect(lastView(frame)).toEqual({
        view: "save",
        site: "example.com",
        title: "example.com",
        username: "alice",
        password: "hunter2",
      }),
    );
    // The password as the user corrected it in the prompt.
    frame.send({
      kind: "action",
      action: {
        type: "save",
        title: "Example",
        username: "alice",
        password: "hunter3",
        group: "Work",
      },
      point: { x: 300, y: 200 },
      trusted: true,
    });
    await vi.waitFor(() => expect(app.requests.at(-1)?.op).toBe("save"));
    expect(app.requests.at(-1)).toMatchObject({
      title: "Example",
      username: "alice",
      password: "hunter3",
      group: "Work",
      frame_url: "https://www.example.com/login",
    });
    await vi.waitFor(() => expect(frame.closed).toBe(true));
  });

  it("asks to update a login whose password changed, shows the prompt again on the next page, and keeps quiet when told", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    app.handlers.update = () => ({});
    app.handlers.site_rule = () => ({});
    await browser.fromContent({
      kind: "content.submitted",
      username: "octocat",
      password: "n3w",
      signup: false,
    });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "prompt.open")).toBe(true));
    // The page navigated: the next document asks at its start, and gets the prompt again.
    const start = (await browser.fromContent({ kind: "content.start", top: true })) as {
      prompt: string;
    };
    expect(start.prompt).toMatch(/^[0-9a-f]{32}$/);
    const frame = inlineFrame(context, start.prompt);
    await vi.waitFor(() =>
      expect(lastView(frame)).toEqual({
        view: "update",
        site: "example.com",
        item: { title: "GitHub", username: "octocat", downgrade: false },
        password: "n3w",
      }),
    );
    frame.send({
      kind: "action",
      action: { type: "update", password: "n3w" },
      point: null,
      trusted: true,
    });
    await vi.waitFor(() =>
      expect(app.requests.at(-1)).toMatchObject({ op: "update", item_id: ITEM, password: "n3w" }),
    );
    expect(
      (
        (await browser.fromContent({ kind: "content.start", top: true })) as {
          prompt: string | null;
        }
      ).prompt,
    ).toBeNull();

    await browser.fromContent({
      kind: "content.submitted",
      username: "bob",
      password: "pw",
      signup: false,
    });
    await vi.waitFor(() =>
      expect(contentGot.filter((m) => m.kind === "prompt.open")).toHaveLength(2),
    );
    const second = contentGot.findLast((m) => m.kind === "prompt.open") as { token: string };
    const prompt = inlineFrame(context, second.token, 6);
    await vi.waitFor(() => expect(lastView(prompt)?.view).toBe("save"));
    prompt.send({ kind: "action", action: { type: "never" }, point: null, trusted: true });
    await vi.waitFor(() =>
      expect(app.requests.at(-1)).toMatchObject({ op: "site_rule", kind: "never_save" }),
    );
  });

  it("offers the user name a sign-in brought to the login saved without one", async () => {
    // 2026-10-22, the user's AWS IAM Identity Center: its password page showed the user name as
    // text only, so the login was saved without one; a later sign-in with the name offers it to
    // that login instead of a second one.
    const context = await setup();
    const { browser, app, contentGot } = context;
    const nameless = { ...summary(ITEM), username: "", email: "" };
    app.handlers.query = (request) =>
      request.purpose === "save"
        ? queryAnswer([nameless], { save: { action: "add_username", item_id: ITEM } })
        : queryAnswer([nameless]);
    app.handlers.update = () => ({});
    await browser.fromContent({
      kind: "content.submitted",
      username: "alice@example.com",
      password: "s3cret",
      signup: false,
    });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "prompt.open")).toBe(true));
    const open = contentGot.find((m) => m.kind === "prompt.open") as { token: string };
    const frame = inlineFrame(context, open.token);
    await vi.waitFor(() =>
      expect(lastView(frame)).toEqual({
        view: "username",
        site: "example.com",
        item: { title: "GitHub", username: "", downgrade: false },
        username: "alice@example.com",
      }),
    );
    // The name as the user left it in the prompt; the password is the login's own.
    frame.send({
      kind: "action",
      action: { type: "username", username: "alice" },
      point: null,
      trusted: true,
    });
    await vi.waitFor(() =>
      expect(app.requests.at(-1)).toMatchObject({
        op: "update",
        item_id: ITEM,
        password: "s3cret",
        username: "alice",
      }),
    );
    await vi.waitFor(() => expect(frame.closed).toBe(true));
    expect(
      ((await browser.fromContent({ kind: "content.start", top: true })) as { prompt: unknown })
        .prompt,
    ).toBeNull();
  });

  it("says when what was sent looks like a one-time code, and saves nothing when told it is one", async () => {
    // 2026-10-20, the user: a code typed into a field the page masks as a password came back as
    // a password to save or to update.
    const context = await setup();
    const { browser, app, contentGot } = context;
    await browser.fromContent({
      kind: "content.submitted",
      username: "octocat",
      password: "123456",
      signup: false,
    });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "prompt.open")).toBe(true));
    const open = contentGot.find((m) => m.kind === "prompt.open") as { token: string };
    const frame = inlineFrame(context, open.token);
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ view: "update", code: true }));
    const before = app.requests.length;
    frame.send({ kind: "action", action: { type: "code" }, point: null, trusted: true });
    await vi.waitFor(() => expect(frame.closed).toBe(true));
    expect(app.requests.slice(before).filter((r) => r.op === "update" || r.op === "save")).toEqual(
      [],
    );
    // Forgotten: the next page shows no prompt.
    expect(
      ((await browser.fromContent({ kind: "content.start", top: true })) as { prompt: unknown })
        .prompt,
    ).toBeNull();
  });

  it("offers to link a code to the login filled moments before, which Lockra does", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    app.handlers.query = () =>
      queryAnswer([summary(ITEM, { has_totp: false })], {
        save: { action: "save", item_id: null },
      });
    app.handlers.link_code = () => ({});
    // The sign-in's first page, filled from the menu.
    await fillFromMenu(context);
    await browser.fromContent({
      kind: "content.submitted",
      username: "",
      password: "123456",
      signup: false,
    });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "prompt.open")).toBe(true));
    const open = contentGot.findLast((m) => m.kind === "prompt.open") as { token: string };
    const frame = inlineFrame(context, open.token);
    await vi.waitFor(() =>
      expect(lastView(frame)).toMatchObject({
        view: "save",
        code: true,
        link: { title: "GitHub" },
      }),
    );
    frame.send({ kind: "action", action: { type: "link_code" }, point: null, trusted: true });
    await vi.waitFor(() =>
      expect(app.requests.at(-1)).toMatchObject({
        op: "link_code",
        item_id: ITEM,
        frame_url: "https://www.example.com/login",
      }),
    );
    await vi.waitFor(() => expect(frame.closed).toBe(true));
    expect(app.requests.some((r) => r.op === "save")).toBe(false);
  });

  it("offers nothing when the app advises nothing", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    app.handlers.query = () => queryAnswer([], { save: { action: "none", item_id: null } });
    await browser.fromContent({
      kind: "content.submitted",
      username: "x",
      password: "y",
      signup: false,
    });
    await browser.fromContent({
      kind: "content.submitted",
      username: "x",
      password: "",
      signup: false,
    });
    expect(contentGot.some((m) => m.kind === "prompt.open")).toBe(false);
  });
});

/** Choose the first login from the inline menu of frame 0, and wait for its fill. */
async function fillFromMenu(context: Context): Promise<void> {
  const { browser, contentGot } = context;
  const fills = contentGot.filter((m) => m.kind === "fill").length;
  const answer = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
    token: string;
  };
  const frame = inlineFrame(context, answer.token);
  await vi.waitFor(() => expect(lastView(frame)?.view).toBe("list"));
  frame.send({ kind: "action", action: { type: "choose", index: 0 }, point: null, trusted: true });
  await vi.waitFor(() =>
    expect(contentGot.filter((m) => m.kind === "fill").length).toBe(fills + 1),
  );
}

describe("the next step of a sign-in", () => {
  it("fills the password of the login whose user name a step took, once, on the next page", async () => {
    const context = await setup();
    const { browser, app, contentGot, filled } = context;
    filled.value = { filled: 1, password: false, code: false };
    await fillFromMenu(context);
    expect(await browser.fromContent({ kind: "content.start", top: true })).toMatchObject({
      nextStep: true,
    });
    filled.value = { filled: 1, password: true, code: false };
    const fills = () => app.requests.filter((r) => r.op === "fill");
    const before = fills().length;
    await browser.fromContent({ kind: "content.next_step" });
    await vi.waitFor(() => expect(fills()).toHaveLength(before + 1));
    expect(fills().at(-1)).toMatchObject({ op: "fill", item_id: ITEM, trigger: "next_step" });
    expect(contentGot.filter((m) => m.kind === "fill")).toHaveLength(2);
    // Once: nothing more is waited for or asked.
    expect(await browser.fromContent({ kind: "content.start", top: true })).toMatchObject({
      nextStep: false,
    });
    await browser.fromContent({ kind: "content.next_step" });
    expect(fills()).toHaveLength(before + 1);
  });

  it("is not waited for after a fill that put the password in, nor past two minutes", async () => {
    const context = await setup();
    const { browser, filled } = context;
    await fillFromMenu(context);
    expect(await browser.fromContent({ kind: "content.start", top: true })).toMatchObject({
      nextStep: false,
    });
    filled.value = { filled: 1, password: false, code: false };
    await fillFromMenu(context);
    context.clock.advance(2 * 60_000);
    expect(await browser.fromContent({ kind: "content.start", top: true })).toMatchObject({
      nextStep: false,
    });
  });
});

describe("the code of a second step", () => {
  it("is filled on the next page of the tab where a login was just filled", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    app.handlers.otp = () => ({ code: "654321", copied: true });
    expect(await browser.fromContent({ kind: "content.start", top: true })).toMatchObject({
      otp: false,
    });
    const answer = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    const frame = inlineFrame(context, answer.token);
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("list"));
    frame.send({
      kind: "action",
      action: { type: "choose", index: 0 },
      point: null,
      trusted: true,
    });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "fill")).toBe(true));
    expect(await browser.fromContent({ kind: "content.start", top: true })).toMatchObject({
      otp: true,
    });
    await browser.fromContent({ kind: "content.otp" });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "fill.otp")).toBe(true));
    expect(app.requests.at(-1)).toMatchObject({ op: "otp", item_id: ITEM });
    // An app that does not say whether to send the code's form: not sent.
    expect(contentGot.find((m) => m.kind === "fill.otp")).toEqual({
      kind: "fill.otp",
      code: "654321",
      submit: false,
    });
    app.handlers.otp = () => ({ code: "123456", copied: false, submit: true });
    await browser.fromContent({ kind: "content.otp" });
    await vi.waitFor(() =>
      expect(contentGot.findLast((m) => m.kind === "fill.otp")).toEqual({
        kind: "fill.otp",
        code: "123456",
        submit: true,
      }),
    );
    // Five minutes on, nothing is asked any more.
    context.clock.advance(5 * 60_000);
    expect(await browser.fromContent({ kind: "content.start", top: true })).toMatchObject({
      otp: false,
    });
  });
});

describe("searching the menu", () => {
  const ITEM3 = "5c0fe1a2-0b3c-4d5e-8f60-000000000003";
  const three = () =>
    queryAnswer([
      summary(ITEM),
      summary(ITEM2),
      summary(ITEM3, { title: "GitLab", username: "", email: "dev@example.com" }),
    ]);

  it("narrows the list to what the box holds, and its arrows and Enter go by what is shown", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    app.handlers.query = three;
    const answer = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    const frame = inlineFrame(context, answer.token);
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ view: "list", search: "" }));
    // Title, user name and email address, without regard to case.
    frame.send({ kind: "search", text: "WORK" });
    await vi.waitFor(() =>
      expect(lastView(frame)).toMatchObject({
        items: [{ username: "octocat-work" }],
        active: 0,
        search: "WORK",
      }),
    );
    frame.send({ kind: "search", text: "dev@" });
    await vi.waitFor(() =>
      expect(lastView(frame)).toMatchObject({
        items: [{ title: "GitLab", username: "dev@example.com" }],
      }),
    );
    frame.send({ kind: "search", text: "git" });
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ items: { length: 3 } }));
    frame.send({ kind: "key", key: "ArrowUp" });
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ active: 2 }));
    frame.send({ kind: "search", text: "octo" });
    await vi.waitFor(() =>
      expect(lastView(frame)).toMatchObject({ items: { length: 2 }, active: 0 }),
    );
    frame.send({ kind: "key", key: "ArrowDown" });
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ active: 1 }));
    // Nothing left: nothing to choose.
    frame.send({ kind: "search", text: "nobody" });
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ items: [], search: "nobody" }));
    frame.send({ kind: "key", key: "ArrowDown" });
    // The chosen row is the one shown at that place.
    frame.send({ kind: "search", text: "lab" });
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ items: [{ title: "GitLab" }] }));
    frame.send({ kind: "visible", visible: true });
    frame.send({
      kind: "action",
      action: { type: "choose", index: 0 },
      point: null,
      trusted: true,
    });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "fill")).toBe(true));
    expect(contentGot.find((m) => m.kind === "frame.verify")).toMatchObject({
      point: null,
      trusted: true,
    });
    expect(app.requests.at(-1)).toMatchObject({ op: "fill", item_id: ITEM3 });
  });

  it("is offered only over two logins or more, and takes no malformed message", async () => {
    const context = await setup();
    const { browser, app } = context;
    app.handlers.query = () => queryAnswer([summary(ITEM)]);
    const one = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    const frame = inlineFrame(context, one.token);
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("list"));
    expect(lastView(frame)).not.toHaveProperty("search");
    // A list that is no search's is not narrowed.
    frame.send({ kind: "search", text: "nobody" });
    frame.send({ kind: "search", text: "x".repeat(300) });
    frame.send({ kind: "key", key: "Tab" });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(lastView(frame)).toMatchObject({ items: [{ username: "octocat" }] });
    expect(lastView(frame)).not.toHaveProperty("search");
  });
});

describe("deleting a login", () => {
  const click = { point: { x: 20, y: 60 }, trusted: true };

  it("asks in the menu first, then deletes it where it is offered and lists the rest", async () => {
    const context = await setup();
    const { browser, app } = context;
    app.handlers.delete = () => ({});
    const answer = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    const frame = inlineFrame(context, answer.token);
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("list"));
    frame.send({ kind: "visible", visible: true });
    frame.send({ kind: "action", action: { type: "remove", index: 1 }, ...click });
    await vi.waitFor(() =>
      expect(lastView(frame)).toEqual({
        view: "delete",
        site: "example.com",
        item: { title: "GitHub", username: "octocat-work", downgrade: false },
      }),
    );
    // Nothing went to Lockra yet; Enter in the page's field deletes nothing either.
    expect(app.requests.some((r) => r.op === "delete")).toBe(false);
    await browser.fromContent({
      kind: "content.key",
      token: answer.token,
      key: "Enter",
      allowed: true,
    });
    expect(app.requests.some((r) => r.op === "delete")).toBe(false);
    frame.send({ kind: "action", action: { type: "delete" }, ...click });
    await vi.waitFor(() =>
      expect(app.requests.at(-1)).toMatchObject({ op: "delete", item_id: ITEM2 }),
    );
    expect(app.requests.at(-1)).toMatchObject({
      frame_url: "https://www.example.com/login",
      top_url: "https://www.example.com/login",
    });
    await vi.waitFor(() =>
      expect(lastView(frame)).toEqual({
        view: "list",
        site: "example.com",
        items: [{ title: "GitHub", username: "octocat", downgrade: false }],
        active: 0,
      }),
    );
    // The last one gone, the menu closes.
    frame.send({ kind: "action", action: { type: "remove", index: 0 }, ...click });
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("delete"));
    frame.send({ kind: "action", action: { type: "delete" }, ...click });
    await vi.waitFor(() => expect(frame.closed).toBe(true));
    expect(app.requests.filter((r) => r.op === "delete").map((r) => r.item_id)).toEqual([
      ITEM2,
      ITEM,
    ]);
  });

  it("words Lockra's refusal, and a click that fails the checks deletes nothing", async () => {
    const context = await setup();
    const { browser, app } = context;
    app.handlers.delete = () => ({ error: "denied" });
    const answer = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    const frame = inlineFrame(context, answer.token);
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("list"));
    frame.send({ kind: "visible", visible: true });
    frame.send({ kind: "action", action: { type: "remove", index: 0 }, ...click });
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("delete"));
    frame.send({ kind: "action", action: { type: "delete" }, ...click });
    await vi.waitFor(() =>
      expect(lastView(frame)).toMatchObject({ view: "notice", refusal: "denied" }),
    );
    // Not the user's click: the frame closes unasked.
    const again = (await browser.fromContent({ kind: "content.focus", form: "login" })) as {
      token: string;
    };
    const next = inlineFrame(context, again.token, 6);
    await vi.waitFor(() => expect(lastView(next)?.view).toBe("list"));
    next.send({
      kind: "action",
      action: { type: "remove", index: 0 },
      point: null,
      trusted: false,
    });
    await vi.waitFor(() => expect(next.closed).toBe(true));
    expect(app.requests.filter((r) => r.op === "delete")).toHaveLength(1);
  });
});

describe("a code field", () => {
  it("nothing was filled for lately offers the site's logins that have a code, and fills the chosen one's", async () => {
    const context = await setup();
    const { browser, app, contentGot, filled } = context;
    app.handlers.query = () =>
      queryAnswer([summary(ITEM, { has_totp: false }), summary(ITEM2, { has_totp: true })]);
    app.handlers.fill = (request) => ({
      item_id: request.item_id,
      username: "octocat-work",
      password: "s3cret",
      fields: [],
      otp: "246810",
      submit: false,
    });
    filled.value = { filled: 1, password: false, code: true };
    const answer = (await browser.fromContent({ kind: "content.focus", form: "otp" })) as {
      token: string | null;
    };
    expect(app.requests.some((r) => r.op === "otp")).toBe(false);
    expect(answer.token).toMatch(/^[0-9a-f]{32}$/);
    const frame = inlineFrame(context, answer.token ?? "");
    await vi.waitFor(() =>
      expect(lastView(frame)).toMatchObject({
        view: "list",
        code: true,
        items: [{ username: "octocat-work" }],
      }),
    );
    frame.send({
      kind: "action",
      action: { type: "choose", index: 0 },
      point: null,
      trusted: true,
    });
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "fill")).toBe(true));
    expect(contentGot.find((m) => m.kind === "fill")).toMatchObject({ values: { otp: "246810" } });
    expect(app.requests.at(-1)).toMatchObject({
      op: "fill",
      item_id: ITEM2,
      trigger: "inline_menu",
    });
    // A code's fill waits for no later step.
    expect(await browser.fromContent({ kind: "content.start", top: true })).toMatchObject({
      nextStep: false,
    });
    // Filled lately: the code goes in by itself, no menu.
    app.handlers.otp = () => ({ code: "111222", copied: false });
    expect(await browser.fromContent({ kind: "content.focus", form: "otp" })).toEqual({
      token: null,
    });
    await vi.waitFor(() =>
      expect(contentGot.findLast((m) => m.kind === "fill.otp")).toMatchObject({ code: "111222" }),
    );
    // A site with no login that has a code: no menu.
    delete app.handlers.otp;
    app.handlers.query = () => queryAnswer([summary(ITEM, { has_totp: false })]);
    expect(await browser.fromContent({ kind: "content.focus", form: "otp" })).toEqual({
      token: null,
    });
  });

  it("says so when Lockra gives the chosen login no code to fill (its 填入验证码 off)", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    app.handlers.query = () => queryAnswer([summary(ITEM2, { has_totp: true })]);
    const answer = (await browser.fromContent({ kind: "content.focus", form: "otp" })) as {
      token: string | null;
    };
    const frame = inlineFrame(context, answer.token ?? "");
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ view: "list", code: true }));
    frame.send({
      kind: "action",
      action: { type: "choose", index: 0 },
      point: null,
      trusted: true,
    });
    await vi.waitFor(() =>
      expect(lastView(frame)).toMatchObject({ view: "notice", refusal: "protected" }),
    );
    expect(app.requests.at(-1)).toMatchObject({ op: "fill", item_id: ITEM2 });
    expect(contentGot.some((m) => m.kind === "fill")).toBe(false);
  });
});

describe("the shortcut", () => {
  it("fills the focused frame's likeliest login, and the next one when pressed again soon", async () => {
    const context = await setup();
    const { browser, app, contentGot, clock } = context;
    browser.frames.get(1)?.push({
      frameId: 3,
      parentFrameId: 0,
      url: "https://www.example.com/frame",
      documentId: "doc-3",
    });
    browser.probes.set(0, { focused: true, editable: false, framed: true });
    browser.probes.set(3, { focused: true, editable: true, framed: false });
    const third: typeof contentGot = [];
    browser.content.set("1:3", (message) => {
      third.push(message as BackgroundMessage);
      return { filled: 2 };
    });
    browser.commands.onCommand.emit("fill-login", browser.openTabs[0]);
    await vi.waitFor(() => expect(third.some((m) => m.kind === "fill")).toBe(true));
    expect(app.requests.findLast((r) => r.op === "fill")).toMatchObject({
      item_id: ITEM,
      trigger: "shortcut",
      frame_url: "https://www.example.com/frame",
      top_url: "https://www.example.com/login",
    });
    // Without site access the content script is put in the frame on demand.
    expect(browser.injected.at(-1)).toEqual({
      target: { tabId: 1, frameIds: [3] },
      files: ["content.js"],
    });
    browser.commands.onCommand.emit("fill-login", browser.openTabs[0]);
    await vi.waitFor(() => expect(app.requests.filter((r) => r.op === "fill")).toHaveLength(2));
    expect(app.requests.findLast((r) => r.op === "fill")).toMatchObject({ item_id: ITEM2 });
    clock.advance(10_000);
    browser.commands.onCommand.emit("fill-login", browser.openTabs[0]);
    await vi.waitFor(() => expect(app.requests.filter((r) => r.op === "fill")).toHaveLength(3));
    expect(app.requests.findLast((r) => r.op === "fill")).toMatchObject({ item_id: ITEM });
    expect(contentGot.some((m) => m.kind === "fill")).toBe(false);
  });

  it("shows the in-page confirmation when the policy asks first", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    browser.probes.set(0, { focused: true, editable: true, framed: false });
    app.handlers.query = () =>
      queryAnswer([summary(ITEM)], { policy: { ...POLICY, confirm_fill: true } });
    browser.commands.onCommand.emit("fill-login", browser.openTabs[0]);
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "menu.open")).toBe(true));
    const open = contentGot.find((m) => m.kind === "menu.open") as { token: string };
    // Where the menu opens on a click, clear of the browser's own list.
    expect(open).toMatchObject({ side: "above" });
    const frame = inlineFrame(context, open.token);
    await vi.waitFor(() => expect(lastView(frame)?.view).toBe("confirm"));
    expect(app.requests.some((r) => r.op === "fill")).toBe(false);
    frame.send({
      kind: "action",
      action: { type: "fill" },
      point: { x: 10, y: 10 },
      trusted: true,
    });
    await vi.waitFor(() =>
      expect(app.requests.at(-1)).toMatchObject({ op: "fill", trigger: "shortcut" }),
    );
  });

  it("opens the popup when Lockra cannot answer (locked)", async () => {
    const context = await setup();
    const { app, browser } = context;
    await app.lock();
    await vi.waitFor(() => expect(context.connection.view.app?.state).toBe("locked"));
    browser.probes.set(0, { focused: true, editable: true, framed: false });
    browser.commands.onCommand.emit("fill-login", browser.openTabs[0]);
    await vi.waitFor(() => expect(browser.popupOpened).toBe(1));
  });
});

describe("the menu shortcut", () => {
  it("opens Lockra's menu on the focused field with the site's logins, and fills the one chosen", async () => {
    // Pages Lockra has no rule for (the user's request, 2026-10-22): the menu on any field.
    const context = await setup();
    const { browser, app, contentGot } = context;
    browser.probes.set(0, { focused: true, editable: true, framed: false });
    browser.commands.onCommand.emit("open-menu", browser.openTabs[0]);
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "menu.open")).toBe(true));
    // Without site access the content script is put in the frame on demand.
    expect(browser.injected.at(-1)).toEqual({
      target: { tabId: 1, frameIds: [0] },
      files: ["content.js"],
    });
    const open = contentGot.find((m) => m.kind === "menu.open") as { token: string };
    expect(open).toMatchObject({ side: "above" });
    const frame = inlineFrame(context, open.token);
    await vi.waitFor(() => expect(lastView(frame)).toMatchObject({ view: "list" }));
    expect(app.requests.some((r) => r.op === "fill")).toBe(false);
    frame.send({
      kind: "action",
      action: { type: "choose", index: 1 },
      point: { x: 10, y: 10 },
      trusted: true,
    });
    await vi.waitFor(() =>
      expect(app.requests.at(-1)).toMatchObject({
        op: "fill",
        item_id: ITEM2,
        trigger: "inline_menu",
      }),
    );
    await vi.waitFor(() => expect(contentGot.some((m) => m.kind === "fill")).toBe(true));
    expect(browser.popupOpened).toBe(0);
  });

  it("opens the popup instead when no field has the focus, the site has no login, or Lockra cannot answer", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    browser.probes.set(0, { focused: true, editable: false, framed: false });
    browser.commands.onCommand.emit("open-menu", browser.openTabs[0]);
    await vi.waitFor(() => expect(browser.popupOpened).toBe(1));
    browser.probes.set(0, { focused: true, editable: true, framed: false });
    app.handlers.query = () => queryAnswer([]);
    browser.commands.onCommand.emit("open-menu", browser.openTabs[0]);
    await vi.waitFor(() => expect(browser.popupOpened).toBe(2));
    await app.lock();
    await vi.waitFor(() => expect(context.connection.view.app?.state).toBe("locked"));
    browser.commands.onCommand.emit("open-menu", browser.openTabs[0]);
    await vi.waitFor(() => expect(browser.popupOpened).toBe(3));
    expect(contentGot.some((m) => m.kind === "menu.open")).toBe(false);
  });

  it("opens the popup when the field lost the focus before the menu came", async () => {
    const context = await setup();
    const { browser, contentGot, menuIn } = context;
    browser.probes.set(0, { focused: true, editable: true, framed: false });
    menuIn.value = { opened: false };
    browser.commands.onCommand.emit("open-menu", browser.openTabs[0]);
    await vi.waitFor(() => expect(browser.popupOpened).toBe(1));
    const open = contentGot.find((m) => m.kind === "menu.open") as { token: string };
    // The menu's frame record is gone: its page finds nothing to show.
    const frame = inlineFrame(context, open.token);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(lastView(frame)).toBeUndefined();
  });
});

describe("the popup", () => {
  function popup(context: Context): {
    port: FakePortEnd;
    states: () => PopupState[];
    events: () => PopupEvent[];
  } {
    const port = context.browser.connectPort("lockra-popup", {
      id: context.browser.runtime.id,
      url: context.browser.runtime.getURL("popup.html"),
    });
    const events = () => port.received as PopupEvent[];
    const states = () =>
      events()
        .filter((e): e is Extract<PopupEvent, { kind: "state" }> => e.kind === "state")
        .map((e) => e.state);
    return { port, states, events };
  }

  it("lists the current site's logins and fills one into the focused frame", async () => {
    const context = await setup();
    const { browser, app, contentGot } = context;
    browser.probes.set(0, { focused: false, editable: true, framed: false });
    const { port, states, events } = popup(context);
    port.send({ kind: "hello" });
    await vi.waitFor(() => expect(states().at(-1)?.site?.items).toHaveLength(2));
    const state = states().at(-1);
    expect(state).toMatchObject({
      screen: "ready",
      appVersion: "0.10.0",
      extensionVersion: "0.10.0",
      shortcut: "Ctrl+Shift+L",
      pageAccess: false,
      // The pages compare it with their own.
      build: LOCKRA_BUILD,
    });
    expect(state?.site).toMatchObject({ site: "example.com", fillable: true });
    // The list stays in storage.session for the next opening, and goes as Lockra locks.
    expect(browser.storage.session.data.get("snapshot")).toMatchObject({
      url: "https://www.example.com/login",
    });
    port.send({ kind: "fill", itemId: ITEM, allowHttp: false });
    await vi.waitFor(() =>
      expect(events().some((e) => e.kind === "result" && e.outcome === "done")).toBe(true),
    );
    expect(app.requests.at(-1)).toMatchObject({ op: "fill", item_id: ITEM, trigger: "popup" });
    expect(contentGot.some((m) => m.kind === "fill")).toBe(true);
    await app.lock();
    await vi.waitFor(() => expect(states().at(-1)?.screen).toBe("locked"));
    expect(browser.storage.session.data.has("snapshot")).toBe(false);
  });

  it("searches every login, copies through Lockra, and words a refusal", async () => {
    const context = await setup();
    const { app } = context;
    app.handlers.search = () => ({ items: [summary(ITEM, { matches: false })] });
    app.handlers.copy = () => ({});
    const { port, events } = popup(context);
    port.send({ kind: "search", text: "git" });
    await vi.waitFor(() => expect(events().some((e) => e.kind === "search")).toBe(true));
    expect(events().find((e) => e.kind === "search")).toMatchObject({ text: "git", error: null });
    expect(app.requests.at(-1)).toMatchObject({
      op: "search",
      text: "git",
      top_url: "https://www.example.com/login",
    });
    port.send({ kind: "copy", itemId: ITEM, part: { type: "password" } });
    await vi.waitFor(() =>
      expect(events().some((e) => e.kind === "result" && e.action === "copy")).toBe(true),
    );
    expect(events().find((e) => e.kind === "result" && e.action === "copy")).toMatchObject({
      outcome: "done",
      part: "password",
    });
    app.handlers.copy = () => ({ error: "protected" });
    port.send({ kind: "copy", itemId: ITEM, part: { type: "password" } });
    await vi.waitFor(() => expect(events().filter((e) => e.kind === "result")).toHaveLength(2));
    expect(events().findLast((e) => e.kind === "result")).toMatchObject({ outcome: "protected" });
  });

  it("asks before filling an https login into an http page", async () => {
    const context = await setup();
    const { app, browser } = context;
    browser.probes.set(0, { focused: false, editable: true, framed: false });
    app.handlers.query = () => queryAnswer([summary(ITEM, { downgrade: true })]);
    const { port, states, events } = popup(context);
    port.send({ kind: "hello" });
    await vi.waitFor(() => expect(states().at(-1)?.site?.items).toHaveLength(1));
    port.send({ kind: "fill", itemId: ITEM, allowHttp: false });
    await vi.waitFor(() =>
      expect(events().some((e) => e.kind === "result" && e.outcome === "http_downgrade")).toBe(
        true,
      ),
    );
    expect(app.requests.some((r) => r.op === "fill")).toBe(false);
    port.send({ kind: "fill", itemId: ITEM, allowHttp: true });
    await vi.waitFor(() =>
      expect(app.requests.at(-1)).toMatchObject({ op: "fill", allow_http: true }),
    );
  });

  it("fills a login's code into the page where the user asks, or says the page has no field for it", async () => {
    const context = await setup();
    const { browser, app, contentGot, codeIn } = context;
    browser.probes.set(0, { focused: false, editable: true, framed: false });
    app.handlers.otp = () => ({ code: "135790", copied: false, submit: false });
    const { port, states, events } = popup(context);
    port.send({ kind: "hello" });
    await vi.waitFor(() => expect(states().at(-1)?.site?.items).toHaveLength(2));
    port.send({ kind: "fill_code", itemId: ITEM });
    await vi.waitFor(() =>
      expect(events().some((e) => e.kind === "result" && e.part === "totp")).toBe(true),
    );
    expect(app.requests.at(-1)).toMatchObject({
      op: "otp",
      item_id: ITEM,
      trigger: "popup",
      frame_url: "https://www.example.com/login",
    });
    expect(contentGot.findLast((m) => m.kind === "fill.otp")).toEqual({
      kind: "fill.otp",
      code: "135790",
      submit: false,
      focused: true,
    });
    expect(events().findLast((e) => e.kind === "result")).toMatchObject({
      action: "fill",
      outcome: "done",
      part: "totp",
    });
    codeIn.value = { code: false };
    port.send({ kind: "fill_code", itemId: ITEM });
    await vi.waitFor(() =>
      expect(events().findLast((e) => e.kind === "result")).toMatchObject({
        outcome: "no_code_field",
      }),
    );
    // A login without a code is no such request.
    app.handlers.query = () => queryAnswer([summary(ITEM, { has_totp: false })]);
    port.send({ kind: "hello" });
    await vi.waitFor(() => expect(states().at(-1)?.site?.items).toHaveLength(1));
    const asked = app.requests.filter((r) => r.op === "otp").length;
    port.send({ kind: "fill_code", itemId: ITEM });
    await vi.waitFor(() =>
      expect(events().findLast((e) => e.kind === "result")).toMatchObject({
        outcome: "not_found",
      }),
    );
    expect(app.requests.filter((r) => r.op === "otp")).toHaveLength(asked);
  });

  it("deletes a login the user agreed to, and shows the page's logins again", async () => {
    const context = await setup();
    const { browser, app } = context;
    browser.probes.set(0, { focused: false, editable: true, framed: false });
    let gone = false;
    app.handlers.delete = () => {
      gone = true;
      return {};
    };
    app.handlers.query = () =>
      queryAnswer(gone ? [summary(ITEM)] : [summary(ITEM), summary(ITEM2)]);
    const { port, states, events } = popup(context);
    port.send({ kind: "hello" });
    await vi.waitFor(() => expect(states().at(-1)?.site?.items).toHaveLength(2));
    port.send({ kind: "delete", itemId: ITEM2 });
    await vi.waitFor(() =>
      expect(events().findLast((e) => e.kind === "result")).toMatchObject({
        action: "delete",
        itemId: ITEM2,
        outcome: "done",
      }),
    );
    expect(app.requests.find((r) => r.op === "delete")).toMatchObject({
      item_id: ITEM2,
      frame_url: "https://www.example.com/login",
    });
    await vi.waitFor(() => expect(states().at(-1)?.site?.items).toHaveLength(1));
    // One the page does not list is not deleted from here.
    port.send({ kind: "delete", itemId: ITEM2 });
    await vi.waitFor(() =>
      expect(events().findLast((e) => e.kind === "result")).toMatchObject({
        outcome: "not_found",
      }),
    );
    expect(app.requests.filter((r) => r.op === "delete")).toHaveLength(1);
  });

  it("switches the page's site to its full host and shows its matches again", async () => {
    const context = await setup();
    const { browser, app } = context;
    browser.probes.set(0, { focused: false, editable: true, framed: false });
    let host = false;
    app.handlers.site_matching = (request) => {
      host = (request as { host: boolean }).host;
      return {};
    };
    app.handlers.query = () =>
      queryAnswer(host ? [summary(ITEM)] : [summary(ITEM), summary(ITEM2)], {
        host_matching: host,
      });
    const { port, states } = popup(context);
    port.send({ kind: "hello" });
    await vi.waitFor(() => expect(states().at(-1)?.site).toMatchObject({ hostMatching: false }));
    expect(states().at(-1)?.site?.host).toBe("www.example.com");
    port.send({ kind: "site_matching", host: true });
    await vi.waitFor(() =>
      expect(states().at(-1)?.site).toMatchObject({
        hostMatching: true,
        items: [{ item_id: ITEM }],
      }),
    );
    expect(app.requests.find((r) => r.op === "site_matching")).toMatchObject({
      host: true,
      frame_url: "https://www.example.com/login",
    });
  });

  it("follows pairing, unlocking and starting Lockra", async () => {
    const context = await setup();
    const { app, connection } = context;
    const { port, states } = popup(context);
    port.send({ kind: "hello" });
    await vi.waitFor(() => expect(states().at(-1)?.screen).toBe("ready"));
    await connection.forget();
    await vi.waitFor(() => expect(states().at(-1)?.screen).toBe("unpaired"));
    port.send({ kind: "pair" });
    await vi.waitFor(() => expect(states().at(-1)?.screen).toBe("pairing"));
    await vi.waitFor(() => expect(states().at(-1)?.pairing?.code).toBe(app.code));
    port.send({ kind: "pair.cancel" });
    await vi.waitFor(() => expect(states().at(-1)?.screen).toBe("unpaired"));
    await app.lock();
    await vi.waitFor(() => expect(states().at(-1)?.screen).toBe("locked"));
    port.send({ kind: "unlock" });
    await vi.waitFor(() => expect(states().at(-1)?.unlockAsked).toBe(true));
    await app.quit();
    await vi.waitFor(() => expect(states().at(-1)?.screen).toBe("not_running"));
    port.send({ kind: "start", show: false });
    await vi.waitFor(() => expect(states().at(-1)?.starting).toBe(true));
    expect(app.posted.some((m) => m.plain?.op === "start_app")).toBe(true);
  });

  it("opens the extensions page to update the extension", async () => {
    const context = await setup();
    const { port } = popup(context);
    port.send({ kind: "extensions" });
    await vi.waitFor(() =>
      expect(context.browser.created).toEqual([
        "chrome://extensions/?id=testextensionidtestextensionidte",
      ]),
    );
  });
});

describe("site access", () => {
  it("registers the content script when granted, unregisters it when revoked, and says so in the hello", async () => {
    const context = await setup();
    const { browser, app } = context;
    const hellos = () => app.posted.filter((m) => m.plain?.op === "hello");
    const before = hellos().length;
    browser.granted = true;
    browser.permissions.onAdded.emit({ origins: ["<all_urls>"] });
    await vi.waitFor(() => expect(browser.registered.map((s) => s.id)).toEqual(["lockra-content"]));
    expect(browser.registered[0]).toMatchObject({
      allFrames: true,
      persistAcrossSessions: true,
      js: ["content.js"],
    });
    await vi.waitFor(() => expect(hellos()).toHaveLength(before + 1));
    expect(hellos().at(-1)?.plain?.page_access).toBe(true);
    browser.granted = false;
    browser.permissions.onRemoved.emit({ origins: ["<all_urls>"] });
    await vi.waitFor(() => expect(browser.registered).toEqual([]));
    await vi.waitFor(() => expect(hellos().at(-1)?.plain?.page_access).toBe(false));
  });

  it("checks the registration at startup and after an update", async () => {
    const context = await setup();
    const { browser } = context;
    browser.granted = true;
    browser.runtime.onStartup.emit();
    await vi.waitFor(() => expect(browser.registered).toHaveLength(1));
    browser.registered = [];
    browser.runtime.onInstalled.emit({ reason: "update" });
    await vi.waitFor(() => expect(browser.registered).toHaveLength(1));
  });
});

describe("messages the background does not take", () => {
  it("ignores other senders and unknown kinds", async () => {
    const context = await setup();
    const { browser } = context;
    expect(
      await browser.fromContent(
        { kind: "content.focus", form: "login" },
        { id: "another-extension" },
      ),
    ).toBeUndefined();
    expect(await browser.fromContent({ kind: "nonsense" })).toBeUndefined();
    expect(await browser.fromContent("text")).toBeUndefined();
    // A tab closing forgets what it had.
    browser.tabs.onRemoved.emit(1);
  });
});

describe("the extension's own pages", () => {
  it("are known by the extension's id and their path, whatever id the address carries", async () => {
    const context = await setup();
    const { browser } = context;
    const dynamic = "chrome-extension://0f7d3e1c-dynamic-session-id/popup.html";
    const page = browser.connectPort("lockra-popup", { id: browser.runtime.id, url: dynamic });
    page.send({ kind: "hello" });
    await vi.waitFor(() =>
      expect(page.received.some((m) => (m as { kind: string }).kind === "state")).toBe(true),
    );
    // A content script naming the popup's port, a page of another extension, another path: no.
    const content = browser.connectPort("lockra-popup", {
      id: browser.runtime.id,
      url: "https://evil.example/popup.html",
      tab: { id: 1 },
    });
    const stranger = browser.connectPort("lockra-popup", {
      id: "other",
      url: browser.runtime.getURL("popup.html"),
    });
    const elsewhere = browser.connectPort("lockra-options", {
      id: browser.runtime.id,
      url: browser.runtime.getURL("popup.html"),
    });
    const nameless = browser.connectPort("lockra-popup", {
      id: browser.runtime.id,
      url: "not a url",
    });
    await vi.waitFor(() =>
      expect([content, stranger, elsewhere, nameless].every((p) => p.closed)).toBe(true),
    );
  });
});

describe("its own update", () => {
  function popup(context: Context) {
    const port = context.browser.connectPort("lockra-popup", {
      id: context.browser.runtime.id,
      url: context.browser.runtime.getURL("popup.html"),
    });
    port.send({ kind: "hello" });
    const states = () =>
      (port.received as PopupEvent[])
        .filter((e): e is Extract<PopupEvent, { kind: "state" }> => e.kind === "state")
        .map((e) => e.state);
    return { port, states };
  }

  /** Lockra updated to `version` and put its copy of the extension in the folder, then said hello
   *  again. */
  function lockraUpdated(context: Context, version: string, folder = true) {
    context.app.version = version;
    if (folder) context.folder.version = version;
    context.connection.refreshHello();
  }

  /** Until the extension reloaded itself once. A request still under way as the last page closes
   *  (the reopened session's first ones) puts the reload off for a retry (UPDATE_RETRY_MS, the
   *  test after next), which only the manual clock brings: CI on 2026-10-20 closed the popup then. */
  async function reloadedOnce(context: Context) {
    await vi.waitFor(() => {
      context.clock.advance(5_000);
      expect(context.browser.reloads).toBe(1);
    });
  }

  it("reloads itself as it starts when Lockra's folder holds another version", async () => {
    const { browser } = await setup({ folder: "0.10.1" });
    await vi.waitFor(() => expect(browser.reloads).toBe(1));
  });

  it("a copy Lockra puts there while the popup is open waits for it to close, and says so", async () => {
    const context = await setup({ folder: "0.10.0" });
    const { port, states } = popup(context);
    await vi.waitFor(() => expect(states().at(-1)?.screen).toBe("ready"));
    expect(states().at(-1)?.update).toBeNull();
    lockraUpdated(context, "0.10.1");
    await vi.waitFor(() => expect(states().at(-1)?.update).toBe("waiting"));
    expect(context.browser.reloads).toBe(0);
    port.disconnect();
    await reloadedOnce(context);
  });

  it("finds a copy Lockra put there as a page opens, and reloads once it closes", async () => {
    const context = await setup({ folder: "0.10.0" });
    context.folder.version = "0.10.1";
    const { port, states } = popup(context);
    await vi.waitFor(() => expect(states().at(-1)?.update).toBe("waiting"));
    expect(context.browser.reloads).toBe(0);
    port.disconnect();
    await reloadedOnce(context);
  });

  it("reads the folder again as Lockra comes back, of the same version or not", async () => {
    // An install of the same version starts Lockra again with another copy in the folder (it
    // writes the copy before it listens): nothing else would look until a page opens.
    const context = await setup({ folder: "0.10.0" });
    await context.app.quit();
    await vi.waitFor(() => expect(context.connection.view.link).toBe("not_running"));
    context.folder.version = "0.10.1";
    context.app.running = true;
    context.connection.ensure();
    await vi.waitFor(() => expect(context.connection.view.app?.version).toBe("0.10.0"));
    await reloadedOnce(context);
  });

  it("waits for the menu by a field to close, not for one whose page went", async () => {
    const context = await setup({ folder: "0.10.0" });
    const open = async () => {
      const answer = (await context.browser.fromContent({
        kind: "content.focus",
        form: "login",
      })) as {
        token: string;
      };
      const frame = inlineFrame(context, answer.token);
      await vi.waitFor(() => expect(lastView(frame)?.view).toBe("list"));
      return frame;
    };
    // A menu whose page went away (its port gone, its frame still listed) holds nothing up.
    const gone = await open();
    gone.disconnect();
    const menu = await open();
    context.folder.version = "0.10.1";
    const { port, states } = popup(context);
    await vi.waitFor(() => expect(states().at(-1)?.update).toBe("waiting"));
    port.disconnect();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(context.browser.reloads).toBe(0);
    menu.disconnect();
    await reloadedOnce(context);
  });

  it("waits for a request under way, then tries again", async () => {
    const context = await setup({ folder: "0.10.0" });
    const { port, states } = popup(context);
    await vi.waitFor(() => expect(states().at(-1)?.screen).toBe("ready"));
    lockraUpdated(context, "0.10.1");
    await vi.waitFor(() => expect(states().at(-1)?.update).toBe("waiting"));
    await vi.waitFor(() => expect(context.connection.view.session).toBe("open"));
    let answer = () => undefined as void;
    const status = context.app.handlers.status;
    if (!status) throw new Error("the fake app answers status");
    context.app.handlers.status = (raw) =>
      new Promise((resolve) => (answer = () => resolve(status(raw))));
    const request = context.connection.request({ op: "status" });
    await vi.waitFor(() => expect(context.connection.busy).toBe(true));
    port.disconnect();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(context.browser.reloads).toBe(0);
    answer();
    expect(await request).toMatchObject({ ok: true });
    expect(context.connection.busy).toBe(false);
    context.clock.advance(5_000);
    await vi.waitFor(() => expect(context.browser.reloads).toBe(1));
  });

  it("not loaded from Lockra's folder, it tells it is older than Lockra and stays as it is", async () => {
    const context = await setup();
    const { states } = popup(context);
    await vi.waitFor(() => expect(states().at(-1)?.screen).toBe("ready"));
    expect(states().at(-1)?.update).toBeNull();
    lockraUpdated(context, "0.10.1", false);
    await vi.waitFor(() => expect(states().at(-1)?.update).toBe("manual"));
    expect(context.browser.reloads).toBe(0);
  });

  it("a folder Lockra has not brought up to date says so", async () => {
    const context = await setup({ folder: "0.10.0" });
    const { states } = popup(context);
    lockraUpdated(context, "0.10.1", false);
    await vi.waitFor(() => expect(states().at(-1)?.update).toBe("folder"));
    expect(context.browser.reloads).toBe(0);
  });
});
