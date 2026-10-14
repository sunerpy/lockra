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
  const page = new InlinePage({
    doc: document,
    token,
    connect,
    t: extensionTranslator("zh-CN"),
    ...observers,
  });
  page.start();
  return {
    posted,
    connect,
    show: (view: InlineView) => receive({ kind: "view", view }),
    close: () => receive({ kind: "close" }),
    disconnect: () => disconnect(),
    root: () => document.getElementById("root") as HTMLElement,
  };
}

function click(el: Element | null | undefined): void {
  el?.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: 12, clientY: 34 }));
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
      action: { type: "save", title: "GitHub", username: "octo", group: "Work" },
    });
    click(button("不保存"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "dismiss" } });
    click(button("此网站不再提示"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "never" } });
  });

  it("prompts to update a changed password", () => {
    const { posted, show, root } = setup();
    show({ view: "update", site: "github.com", item: GITHUB });
    expect(root().textContent).toContain("更新 Lockra 中的密码？");
    expect(root().textContent).toContain("octocat");
    click(button("更新"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "update" } });
    click(button("不更新"));
    expect(posted.at(-1)).toMatchObject({ action: { type: "dismiss" } });
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
