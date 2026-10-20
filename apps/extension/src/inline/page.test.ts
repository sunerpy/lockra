// The inline frame's page: nothing without a token, then the view the background sends; each click
// goes back with its point and whether the user made it; IntersectionObserver v2 and the frame's
// height are reported.
import { extensionTranslator } from "@lockra/shared/i18n/extension";
import type { BackgroundToInline, InlineToBackground, InlineView } from "../shared/messages";
import { InlinePage, type InlinePort } from "./page";

function setup(
  token = "a".repeat(32),
  observers: {
    IntersectionObserver?: typeof IntersectionObserver;
    ResizeObserver?: typeof ResizeObserver;
  } = {},
) {
  document.body.innerHTML = `<div id="root"></div>`;
  const posted: InlineToBackground[] = [];
  let receive: (message: BackgroundToInline) => void = () => undefined;
  let disconnect: () => void = () => undefined;
  const connect = vi.fn((): InlinePort => ({
    post: (message) => posted.push(message),
    onMessage: (listener) => {
      receive = listener;
    },
    onDisconnect: (listener) => {
      disconnect = listener;
    },
  }));
  const reload = vi.fn();
  const page = new InlinePage({
    doc: document,
    token,
    connect,
    t: extensionTranslator("zh-CN"),
    reload,
    ...observers,
  });
  page.start();
  return {
    posted,
    connect,
    reload,
    show: (view: InlineView) => receive({ kind: "view", view, build: LOCKRA_BUILD }),
    /** A view from a background of another build, or one too old to tell its build. */
    showFrom: (build: string | undefined, view: InlineView) =>
      receive(build === undefined ? { kind: "view", view } : { kind: "view", view, build }),
    close: () => receive({ kind: "close" }),
    disconnect: () => disconnect(),
    root: () => document.getElementById("root") as HTMLElement,
  };
}

/** A pointer's click (`detail` 1); a button the keyboard presses clicks with `detail` 0. */
function click(el: Element | null | undefined, detail = 1): void {
  el?.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: 12, clientY: 34, detail }));
}

function button(text: string): HTMLButtonElement | undefined {
  const buttons = Array.from(document.querySelectorAll("button"));
  return (
    buttons.find((b) => b.textContent?.trim() === text || b.getAttribute("aria-label") === text) ??
    buttons.find((b) => b.textContent?.includes(text))
  );
}

const GITHUB = { title: "GitHub", username: "octocat", downgrade: false };

describe("the inline frame", () => {
  it("shows nothing without a token of the right shape", () => {
    const { connect, root } = setup("not-a-token");
    expect(connect).not.toHaveBeenCalled();
    expect(root().childElementCount).toBe(0);
  });

  it("says hello with its token and shows nothing until the background answers", () => {
    const { posted, root } = setup();
    expect(posted[0]).toEqual({ kind: "hello", token: "a".repeat(32) });
    expect(root().textContent).toBe("");
  });

  it("lists the matches and sends the chosen row with where the click landed", () => {
    const { posted, show, root } = setup();
    show({
      view: "list",
      site: "github.com",
      items: [GITHUB, { ...GITHUB, username: "octocat-work" }],
      active: 0,
    });
    expect(root().textContent).toContain("github.com · 2 个匹配");
    expect(root().textContent).toContain("↑↓ 选择 · ↵ 填充 · Esc 关闭");
    const rows = Array.from(root().querySelectorAll("button")).filter((b) =>
      b.title.startsWith("GitHub"),
    );
    expect(rows).toHaveLength(2);
    expect(rows[0]?.querySelector("kbd")?.textContent).toBe("↵");
    expect(rows[0]?.querySelector("[data-tag]")?.textContent).toBe("G");
    click(rows[1]);
    // jsdom's synthetic events are not the user's: the background refuses such a click.
    expect(posted.at(-1)).toEqual({
      kind: "action",
      action: { type: "choose", index: 1 },
      point: { x: 12, y: 34 },
      trusted: false,
    });
    click(button("关闭"));
    expect(posted.at(-1)).toEqual({
      kind: "action",
      action: { type: "close" },
      point: null,
      trusted: false,
    });
  });

  it("offers the generated password on a sign-up form", () => {
    const { posted, show, root } = setup();
    show({
      view: "signup",
      site: "github.com",
      password: "Vq7#mL2!xR9wTz4@pK8s",
      username: "new-user",
      rules: true,
    });
    expect(root().textContent).toContain("使用生成的密码");
    expect(root().textContent).toContain("Vq7#mL2!xR9wTz4@pK8s");
    expect(root().textContent).toContain("按本站的密码规则生成 · 20 位");
    expect(root().textContent).toContain(
      "填好密码框，并在 Lockra 里新建「github.com · new-user」。",
    );
    click(button("使用生成的密码"));
    expect(posted.at(-1)).toMatchObject({ kind: "action", action: { type: "use" } });
    click(button("换一个"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "regenerate" } });
    // The copy, through Lockra; then it says so.
    click(button("复制密码"));
    expect(posted.at(-1)).toMatchObject({ kind: "action", action: { type: "copy" } });
    show({
      view: "signup",
      site: "github.com",
      password: "Vq7#mL2!xR9wTz4@pK8s",
      username: "new-user",
      rules: true,
      copied: true,
    });
    expect(button("已复制")).toBeTruthy();
    show({ view: "signup", site: "github.com", password: "abc", username: "", rules: false });
    expect(root().textContent).toContain("随机生成 · 3 位");
    expect(root().textContent).toContain("填好密码框，并在 Lockra 里新建「github.com」。");
  });

  it("asks before filling, warns before filling into an http page, waits, and words a refusal", () => {
    const { posted, show, root } = setup();
    show({ view: "confirm", item: GITHUB });
    expect(root().textContent).toContain("填充「GitHub · octocat」？");
    click(button("填充"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "fill" } });
    show({ view: "http", site: "example.com", item: GITHUB });
    expect(root().textContent).toContain("这个页面没有加密（HTTP）");
    click(button("仍然填充"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "allow_http" } });
    click(button("取消"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "close" } });
    show({ view: "waiting", item: GITHUB });
    expect(root().textContent).toContain("在 Lockra 中确认");
    show({ view: "notice", site: "example.com", refusal: "denied" });
    expect(root().textContent).toContain("在 Lockra 中被拒绝。");
    show({ view: "notice", site: "example.com", refusal: "later_code" });
    expect(root().textContent).toContain("没有完成（later_code）。");
  });

  it("prompts to save what was submitted, with the fields as edited", () => {
    const { posted, show, root } = setup();
    show({
      view: "save",
      site: "github.com",
      title: "github.com",
      username: "octocat",
      password: "hunter2",
    });
    expect(root().textContent).toContain("保存到 Lockra？");
    const inputs = Array.from(root().querySelectorAll("input"));
    expect(inputs.map((i) => i.value)).toEqual(["github.com", "octocat", "hunter2", ""]);
    expect(inputs[2]?.type).toBe("password");
    click(button("显示密码"));
    expect(Array.from(root().querySelectorAll("input"))[2]?.type).toBe("text");
    const [title, user, , group] = Array.from(root().querySelectorAll("input"));
    if (title) title.value = "GitHub";
    if (group) group.value = "Work";
    if (user) user.value = "octo";
    click(button("保存"));
    expect(posted.at(-1)).toMatchObject({
      action: {
        type: "save",
        title: "GitHub",
        username: "octo",
        password: "hunter2",
        group: "Work",
      },
    });
    click(button("不保存"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "dismiss" } });
    click(button("此网站不再提示"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "never" } });
  });

  it("lets the password be corrected before it is saved, and saves none empty", () => {
    // 2026-10-20, the user: what a form sent is not always the password; the prompt is editable.
    const { posted, show, root } = setup();
    show({
      view: "save",
      site: "github.com",
      title: "github.com",
      username: "octocat",
      password: "hunter2",
    });
    const password = Array.from(root().querySelectorAll("input"))[2];
    expect(password?.readOnly).toBe(false);
    if (password) {
      password.value = "correct horse";
      password.dispatchEvent(new Event("input", { bubbles: true }));
    }
    click(button("保存"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "save", password: "correct horse" } });
    if (password) {
      password.value = "";
      password.dispatchEvent(new Event("input", { bubbles: true }));
    }
    expect(button("保存")?.disabled).toBe(true);
  });

  it("prompts to update a changed password, the new one shown masked and editable", () => {
    const { posted, show, root } = setup();
    show({ view: "update", site: "github.com", item: GITHUB, password: "new-secret" });
    expect(root().textContent).toContain("更新 Lockra 中的密码？");
    expect(root().textContent).toContain("octocat");
    const password = root().querySelector("input");
    expect([password?.type, password?.value]).toEqual(["password", "new-secret"]);
    click(button("显示密码"));
    expect(root().querySelector("input")?.type).toBe("text");
    click(button("更新"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "update", password: "new-secret" } });
    const shown = root().querySelector("input");
    if (shown) {
      shown.value = "newer-secret";
      shown.dispatchEvent(new Event("input", { bubbles: true }));
    }
    click(button("更新"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "update", password: "newer-secret" } });
    click(button("不更新"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "dismiss" } });
  });

  it("says when what was sent looks like a one-time code, and takes it for one", () => {
    // 2026-10-20, the user: a code typed into a field the page masks as a password came back as
    // a password to save.
    for (const view of [
      {
        view: "save",
        site: "example.com",
        title: "example.com",
        username: "me",
        password: "123456",
        code: true,
      },
      { view: "update", site: "example.com", item: GITHUB, password: "654321", code: true },
    ] as InlineView[]) {
      const { posted, show, root } = setup();
      show(view);
      expect(root().textContent).toContain("这看起来是验证码，不是密码");
      click(button("这是验证码"));
      expect(posted.at(-1)).toMatchObject({ action: { type: "code" } });
    }
    const { show, root } = setup();
    show({
      view: "save",
      site: "example.com",
      title: "example.com",
      username: "me",
      password: "hunter2",
    });
    expect(root().textContent).not.toContain("验证码");
  });

  it("offers to link a code to the site's login that has none, picked in Lockra", () => {
    const { posted, show, root } = setup();
    show({
      view: "save",
      site: "example.com",
      title: "example.com",
      username: "me",
      password: "123456",
      code: true,
      link: { title: "ADFS", username: "me", downgrade: false },
    });
    expect(root().textContent).toContain(
      "在 Lockra 里给「ADFS」关联验证码后，这一步不再提示保存，扩展弹窗中的「填入验证码」可以把验证码填进这个输入框。",
    );
    click(button("关联验证码…"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "link_code" } });
    // Without a login to link: the code alone.
    const other = setup();
    other.show({
      view: "update",
      site: "example.com",
      item: GITHUB,
      password: "654321",
      code: true,
    });
    expect(button("关联验证码…")).toBeUndefined();
  });

  it("asks for a reload instead of a view when the background is of another build", () => {
    // 2026-10-20, the user's Chrome: the pages read the new files of Lockra's folder while the
    // browser still ran a background two versions older, which let the delete button and the code
    // button do nothing.
    const list: InlineView = { view: "list", site: "github.com", items: [GITHUB], active: 0 };
    for (const build of [undefined, "an-older-build"]) {
      const { showFrom, posted, reload, root } = setup();
      showFrom(build, list);
      expect(root().textContent).toContain("Lockra 扩展需要重新加载");
      expect(root().textContent).not.toContain("octocat");
      click(button("重新加载扩展"));
      expect(reload).toHaveBeenCalledTimes(1);
      // Its close button still closes the frame.
      click(button("关闭"));
      expect(posted.at(-1)).toMatchObject({ kind: "action", action: { type: "close" } });
    }
  });

  it("empties itself when closed or disconnected", () => {
    const { show, close, disconnect, root } = setup();
    show({ view: "confirm", item: GITHUB });
    close();
    expect(root().textContent).toBe("");
    show({ view: "confirm", item: GITHUB });
    disconnect();
    expect(root().textContent).toBe("");
  });

  it("reports what IntersectionObserver v2 sees, and its height", () => {
    let seen: IntersectionObserverCallback = () => undefined;
    let sized: ResizeObserverCallback = () => undefined;
    class FakeIntersection {
      constructor(callback: IntersectionObserverCallback) {
        seen = callback;
      }
      observe(): void {}
    }
    class FakeResize {
      constructor(callback: ResizeObserverCallback) {
        sized = callback;
      }
      observe(): void {}
    }
    const { posted } = setup("a".repeat(32), {
      IntersectionObserver: FakeIntersection as unknown as typeof IntersectionObserver,
      ResizeObserver: FakeResize as unknown as typeof ResizeObserver,
    });
    seen(
      [{ isVisible: false } as unknown as IntersectionObserverEntry],
      {} as IntersectionObserver,
    );
    expect(posted.at(-1)).toEqual({ kind: "visible", visible: false });
    // A browser without v2 says nothing.
    const count = posted.length;
    seen([{} as IntersectionObserverEntry], {} as IntersectionObserver);
    expect(posted).toHaveLength(count);
    sized([], {} as ResizeObserver);
    expect(posted.at(-1)).toEqual({ kind: "size", height: 0 });
  });
});

describe("the search box", () => {
  const WORK = { ...GITHUB, username: "octocat-work" };
  const list = (search: string | undefined, items = [GITHUB, WORK], active = 0): InlineView => ({
    view: "list",
    site: "github.com",
    items,
    active,
    ...(search === undefined ? {} : { search }),
  });
  const box = () => document.querySelector<HTMLInputElement>('input[aria-label="搜索登录项"]');
  const key = (target: Element, name: string) =>
    target.dispatchEvent(
      new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }),
    );

  it("shows over a list that may be searched, and sends what is typed", () => {
    const { posted, show } = setup();
    show(list(undefined));
    expect(box()).toBeNull();
    show(list(""));
    const input = box() as HTMLInputElement;
    expect(input.placeholder).toBe("搜索登录项");
    input.value = "work";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    expect(posted.at(-1)).toEqual({ kind: "search", text: "work" });
  });

  it("keeps the box, its text, its focus and its height as the list narrows, and says when nothing is left", () => {
    const { show, root } = setup();
    show(list(""));
    const input = box() as HTMLInputElement;
    const rows = root().querySelector<HTMLElement>(".overflow-y-auto") as HTMLElement;
    expect(rows.style.minHeight).toBe("104px");
    input.focus();
    input.value = "work";
    show(list("work", [WORK]));
    expect(box()).toBe(input);
    expect(root().querySelector(".overflow-y-auto")).toBe(rows);
    expect(rows.style.minHeight).toBe("104px");
    expect(document.activeElement).toBe(input);
    expect(input.value).toBe("work");
    expect(root().textContent).toContain("github.com · 1 个匹配");
    expect(
      Array.from(root().querySelectorAll("button")).filter((b) => b.title.startsWith("GitHub")),
    ).toHaveLength(1);
    input.value = "nobody";
    show(list("nobody", []));
    expect(box()).toBe(input);
    expect(root().textContent).toContain("没有匹配的登录项");
  });

  it("moves with the arrows, fills with Enter, and clears before it closes with Esc", () => {
    const { posted, show } = setup();
    show(list("", [GITHUB, WORK], 1));
    const input = box() as HTMLInputElement;
    key(input, "ArrowDown");
    expect(posted.at(-1)).toEqual({ kind: "key", key: "ArrowDown" });
    key(input, "ArrowUp");
    expect(posted.at(-1)).toEqual({ kind: "key", key: "ArrowUp" });
    key(input, "Enter");
    // A key has no point: the checks run at the frame's middle, as for Enter in the page's field.
    expect(posted.at(-1)).toEqual({
      kind: "action",
      action: { type: "choose", index: 1 },
      point: null,
      trusted: false,
    });
    input.value = "work";
    key(input, "Escape");
    expect(input.value).toBe("");
    expect(posted.at(-1)).toEqual({ kind: "search", text: "" });
    key(input, "Escape");
    expect(posted.at(-1)).toMatchObject({ kind: "action", action: { type: "close" } });
  });

  it("sends no Enter while nothing is listed", () => {
    const { posted, show } = setup();
    show(list("nobody", []));
    const before = posted.length;
    key(box() as HTMLInputElement, "Enter");
    expect(posted).toHaveLength(before);
  });
});

describe("deleting from the menu", () => {
  const WORK = { ...GITHUB, username: "octocat-work" };
  const rows = () =>
    Array.from(document.querySelectorAll("button")).filter((b) => b.title.startsWith("GitHub"));

  it("offers each row's deletion, which asks first", () => {
    const { posted, show } = setup();
    show({ view: "list", site: "github.com", items: [GITHUB, WORK], active: 0 });
    const remove = document.querySelectorAll<HTMLButtonElement>('button[aria-label="删除…"]');
    expect(remove).toHaveLength(2);
    // The chosen row shows its button; the others as the pointer or the focus comes.
    expect(remove[0]?.classList.contains("opacity-0")).toBe(false);
    expect(remove[1]?.classList.contains("opacity-0")).toBe(true);
    click(remove[1]);
    expect(posted.at(-1)).toEqual({
      kind: "action",
      action: { type: "remove", index: 1 },
      point: { x: 12, y: 34 },
      trusted: false,
    });
    // The row itself still fills.
    click(rows()[0]);
    expect(posted.at(-1)).toMatchObject({ action: { type: "choose", index: 0 } });
  });

  it("asks with the login's name, starts on 取消, and deletes only on 删除", async () => {
    const { posted, show, root } = setup();
    show({ view: "delete", site: "github.com", item: WORK });
    expect(root().textContent).toContain("删除「GitHub · octocat-work」？");
    expect(root().textContent).toContain("删除后无法恢复，同步的设备也会删除它。");
    expect(root().textContent).toContain("Esc 取消");
    await Promise.resolve();
    expect(document.activeElement).toBe(button("取消"));
    click(button("取消"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "close" } });
    click(button("删除"));
    expect(posted.at(-1)).toEqual({
      kind: "action",
      action: { type: "delete" },
      point: { x: 12, y: 34 },
      trusted: false,
    });
    // Pressed from the keyboard, it has no point: the checks run at the frame's middle.
    click(button("删除"), 0);
    expect(posted.at(-1)).toMatchObject({ action: { type: "delete" }, point: null });
  });
});
