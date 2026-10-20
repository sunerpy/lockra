// The inline frame's page (inline.html): the menu under a field and the save or update prompt, drawn
// in plain DOM with the tokens (DESIGN.md; boards 33, 34). It shows nothing until the background
// accepts the token from its URL's fragment (the content script put it there) and sends the view;
// every click goes back with where it landed and whether the user made it, and counts only once
// the content script's checks pass. Its own IntersectionObserver v2 (Chromium) reports whether the
// frame is really visible.
import type { ExtensionKey, ExtensionT, Params } from "@lockra/shared/i18n/extension";
import type {
  BackgroundToInline,
  InlineAction,
  InlineToBackground,
  InlineView,
  MenuItem,
} from "../shared/messages";
import { AVATAR_RADIUS, AVATAR_TEXT, autoColor, initial } from "../shared/avatar-look";
import { type IconName, icon, logo } from "../shared/icons";
import { refusalText } from "../shared/refusal";

export interface InlinePort {
  post(message: InlineToBackground): void;
  onMessage(listener: (message: BackgroundToInline) => void): void;
  onDisconnect(listener: () => void): void;
}

export interface InlineOptions {
  doc: Document;
  token: string;
  connect(): InlinePort;
  t: ExtensionT;
  /** Reload the extension (`runtime.reload`): the background is of another build. */
  reload?: () => void;
  /** The browser's observers (the tests give theirs; jsdom has none). */
  IntersectionObserver?: typeof IntersectionObserver | undefined;
  ResizeObserver?: typeof ResizeObserver | undefined;
}

type Child = Node | string | null | undefined | false;

/** IntersectionObserver v2's options (Chromium): the frame's visibility, occlusion included. */
/** A list's row, and how many show before it scrolls. */
const ROW_HEIGHT = 52;
const MAX_ROWS = 7;

const VISIBILITY: IntersectionObserverInit & { trackVisibility: boolean; delay: number } = {
  threshold: 1,
  trackVisibility: true,
  delay: 100,
};

export class InlinePage {
  private port: InlinePort | null = null;
  private readonly root: HTMLElement;
  private view: InlineView | null = null;
  private revealed = false;
  private lastHeight = -1;
  /** A searchable list's parts, kept while its view changes: the box keeps its text and focus. */
  private searching: { hint: HTMLElement; rows: HTMLElement } | null = null;

  constructor(private readonly options: InlineOptions) {
    this.root = options.doc.getElementById("root") ?? options.doc.body;
  }

  start(): void {
    // No token, nothing to ask for: an empty frame.
    if (!/^[0-9a-f]{32}$/.test(this.options.token)) return;
    const port = this.options.connect();
    this.port = port;
    port.onMessage((message) => {
      if (message.kind !== "view") this.clear();
      else if (message.build === LOCKRA_BUILD) this.render(message.view);
      else this.stale();
    });
    port.onDisconnect(() => {
      this.port = null;
      this.clear();
    });
    port.post({ kind: "hello", token: this.options.token });
    this.watchVisibility();
    this.watchSize();
  }

  private send(message: InlineToBackground): void {
    try {
      this.port?.post(message);
    } catch {
      this.port = null;
    }
  }

  private t(key: ExtensionKey, params?: Params): string {
    return this.options.t(key, params);
  }

  // ---- what the frame tells -------------------------------------------------------------------

  private watchVisibility(): void {
    const Observer = this.options.IntersectionObserver;
    if (typeof Observer !== "function") return;
    try {
      const observer = new Observer((entries) => {
        const entry = entries.at(-1);
        // Firefox has no v2: it says nothing, which the checks read as unknown.
        if (entry && "isVisible" in entry && typeof entry.isVisible === "boolean")
          this.send({ kind: "visible", visible: entry.isVisible });
      }, VISIBILITY);
      observer.observe(this.root);
    } catch {
      // No v2 options here.
    }
  }

  private watchSize(): void {
    const Observer = this.options.ResizeObserver;
    const report = () => {
      const height = Math.ceil(this.root.getBoundingClientRect().height);
      if (height === this.lastHeight) return;
      this.lastHeight = height;
      this.send({ kind: "size", height });
    };
    if (typeof Observer === "function") new Observer(report).observe(this.root);
    this.afterRender = report;
  }

  private afterRender: () => void = () => undefined;

  /** A click goes with where it landed; a key, or a button the keyboard pressed (a click of no
   *  pointer, `detail` 0), with no point: the checks run at the frame's middle. */
  private act(action: InlineAction, event?: MouseEvent | KeyboardEvent): void {
    const pointer = event && "clientX" in event && event.detail !== 0;
    this.send({
      kind: "action",
      action,
      point: pointer ? { x: event.clientX, y: event.clientY } : null,
      trusted: event?.isTrusted ?? false,
    });
  }

  // ---- drawing ----------------------------------------------------------------------------------

  private el<K extends keyof HTMLElementTagNameMap>(
    tag: K,
    className: string,
    ...children: Child[]
  ): HTMLElementTagNameMap[K] {
    const node = this.options.doc.createElement(tag);
    if (className) node.className = className;
    for (const child of children) {
      if (child === null || child === undefined || child === false) continue;
      node.append(child);
    }
    return node;
  }

  private button(
    label: string,
    variant: "primary" | "danger" | "ghost" | "outline" | "text-muted",
    onClick: (event: MouseEvent) => void,
    iconName?: IconName,
  ): HTMLButtonElement {
    const styles = {
      primary: "bg-primary text-primary-fg shadow-primary hover:opacity-90",
      danger: "bg-danger text-surface-solid hover:opacity-90",
      ghost: "bg-transparent text-fg hover:bg-inset",
      outline: "bg-surface-solid text-fg hairline hover:bg-inset",
      "text-muted": "bg-transparent px-0 text-fg-muted hover:text-fg",
    };
    const node = this.el(
      "button",
      `inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-6 px-2.5 text-[12px] font-medium whitespace-nowrap transition-colors ${styles[variant]}`,
      iconName ? icon(this.options.doc, iconName, 13) : null,
      label,
    );
    node.type = "button";
    node.addEventListener("click", onClick);
    return node;
  }

  private iconButton(
    name: IconName,
    label: string,
    onClick: (event: MouseEvent) => void,
  ): HTMLButtonElement {
    const node = this.el(
      "button",
      "inline-flex size-6 shrink-0 items-center justify-center rounded-6 text-fg-muted transition-colors hover:bg-inset hover:text-fg",
      icon(this.options.doc, name, 14),
    );
    node.type = "button";
    node.setAttribute("aria-label", label);
    node.title = label;
    node.addEventListener("click", onClick);
    return node;
  }

  private avatar(item: MenuItem): HTMLSpanElement {
    const node = this.el(
      "span",
      `inline-flex size-8 shrink-0 items-center justify-center bg-tag-bg font-semibold text-tag-fg select-none ${AVATAR_TEXT[32]}`,
      initial(item.title, item.username),
    );
    node.dataset.tag = autoColor(item.title, item.username);
    node.style.borderRadius = `${AVATAR_RADIUS[32]}px`;
    node.setAttribute("aria-hidden", "true");
    return node;
  }

  private keycap(text: string): HTMLElement {
    return this.el(
      "kbd",
      "mono inline-flex h-5 min-w-5 items-center justify-center rounded-6 border border-keycap-border bg-keycap-bg px-1.5 text-[11px] leading-none text-fg-muted",
      text,
    );
  }

  private clear(): void {
    this.view = null;
    this.searching = null;
    this.root.replaceChildren();
  }

  private render(view: InlineView): void {
    if (this.view?.view !== view.view) this.revealed = false;
    const before = this.view;
    this.view = view;
    // A searchable list that stays one changes its hint and rows only: redrawn whole, the box
    // would lose the focus and the caret at every key.
    if (
      this.searching &&
      view.view === "list" &&
      view.search !== undefined &&
      before?.view === "list" &&
      before.search !== undefined
    ) {
      this.searching.hint.textContent = this.hint(view);
      this.searching.rows.replaceChildren(...this.rows(view.items, view.active));
      this.showActive(this.searching.rows);
      this.afterRender();
      return;
    }
    this.searching = null;
    const body =
      view.view === "save" || view.view === "update" ? this.prompt(view) : this.menu(view);
    this.root.replaceChildren(body);
    this.afterRender();
  }

  /** The background is of another build (an older one the browser still runs, its files having
   *  changed under it): what it would show may not be what this page draws, and what this page's
   *  buttons ask it may be nothing it knows. A reload of the extension instead. */
  private stale(): void {
    const t = (key: ExtensionKey) => this.t(key);
    this.view = null;
    this.searching = null;
    const reload = this.button(
      t("inline.staleReload"),
      "primary",
      () => this.options.reload?.(),
      "refresh",
    );
    this.root.replaceChildren(
      this.el(
        "div",
        "overflow-hidden rounded-10 border border-border bg-surface-solid",
        this.el(
          "div",
          "flex h-[34px] items-center gap-2 px-3",
          logo(this.options.doc, 18),
          this.el("span", "min-w-0 flex-1 text-[12px] font-semibold text-fg", "Lockra"),
          this.iconButton("x", t("inline.close"), () => this.act({ type: "close" })),
        ),
        this.el("div", "h-px bg-border"),
        this.el(
          "div",
          "p-1",
          this.message("refresh", t("inline.staleTitle"), t("inline.staleBody")),
          this.el("div", "flex px-2.5 pb-2.5 pl-9", reload),
        ),
      ),
    );
    this.afterRender();
  }

  private hint(view: Exclude<InlineView, { view: "save" } | { view: "update" }>): string {
    const site = "site" in view ? view.site : "";
    return view.view === "list"
      ? view.code
        ? this.t("inline.codeHint", { site })
        : this.t("inline.matches", { site, n: view.items.length })
      : view.view === "signup"
        ? this.t("inline.signupHint")
        : site;
  }

  /** The menu's frame: a header, the content, a footer of keys. */
  private menu(view: Exclude<InlineView, { view: "save" } | { view: "update" }>): HTMLElement {
    const t = (key: ExtensionKey, params?: Params) => this.t(key, params);
    const hint = this.el(
      "span",
      "min-w-0 flex-1 truncate text-[11px] text-fg-subtle",
      this.hint(view),
    );
    const header = this.el(
      "div",
      "flex h-[34px] items-center gap-2 px-3",
      logo(this.options.doc, 18),
      this.el("span", "text-[12px] font-semibold text-fg", "Lockra"),
      hint,
      this.iconButton("x", t("inline.close"), () => this.act({ type: "close" })),
    );
    let content: HTMLElement;
    let footer: string;
    switch (view.view) {
      case "list":
        content = this.list(view, hint);
        footer = t(view.code ? "inline.codeFooter" : "inline.listFooter");
        break;
      case "signup":
        content = this.signup(view);
        footer = t("inline.signupFooter");
        break;
      case "confirm":
        content = this.confirm(view.item);
        footer = t("inline.confirmFooter");
        break;
      case "delete":
        content = this.deleteView(view.item);
        footer = t("inline.escFooter");
        break;
      case "http":
        content = this.http();
        footer = t("inline.escFooter");
        break;
      case "waiting":
        content = this.message("lock", t("inline.waitingTitle"), t("inline.waitingBody"));
        footer = t("inline.escFooter");
        break;
      case "notice":
        content = this.message(
          "info",
          t("inline.noticeTitle"),
          refusalText(this.options.t, view.refusal),
        );
        footer = t("inline.escFooter");
        break;
    }
    return this.el(
      "div",
      "overflow-hidden rounded-10 border border-border bg-surface-solid",
      header,
      this.el("div", "h-px bg-border"),
      this.el("div", "p-1", content),
      this.el("div", "h-px bg-border"),
      this.el("div", "mono flex h-[30px] items-center px-3 text-[10px] text-fg-subtle", footer),
    );
  }

  /** The logins, seven rows high at most (the rest scroll), under the search box when the list
   *  has one. A list being searched keeps the height it opened with: a frame that changes its
   *  size starts its wait before a click or Enter counts again, so an Enter right after typing
   *  would close it unfilled. */
  private list(view: Extract<InlineView, { view: "list" }>, hint: HTMLElement): HTMLElement {
    const rows = this.el(
      "div",
      "flex flex-col overflow-y-auto",
      ...this.rows(view.items, view.active),
    );
    rows.style.maxHeight = `${MAX_ROWS * ROW_HEIGHT}px`;
    queueMicrotask(() => this.showActive(rows));
    if (view.search === undefined) return rows;
    rows.style.minHeight = `${Math.min(view.items.length, MAX_ROWS) * ROW_HEIGHT}px`;
    this.searching = { hint, rows };
    return this.el("div", "flex flex-col gap-1", this.searchBox(view.search), rows);
  }

  private rows(items: MenuItem[], active: number): HTMLElement[] {
    if (items.length === 0)
      return [this.el("p", "px-2 py-3 text-[12px] text-fg-subtle", this.t("inline.noMatch"))];
    return items.map((item, index) => {
      const row = this.el(
        "button",
        "flex h-full min-w-0 flex-1 items-center gap-3 px-2 text-left",
        this.avatar(item),
        this.el(
          "span",
          "min-w-0 flex-1",
          this.el("span", "block truncate text-[13px] font-semibold text-fg", item.title),
          this.el("span", "block truncate text-[12px] text-fg-muted", item.username),
        ),
        index === active ? this.keycap("↵") : null,
      );
      row.type = "button";
      row.title = `${item.title} · ${item.username}`;
      row.addEventListener("click", (event) => this.act({ type: "choose", index }, event));
      // Deleting asks first, in the menu (the background shows the question).
      const remove = this.iconButton("trash", this.t("inline.delete"), (event) =>
        this.act({ type: "remove", index }, event),
      );
      if (index !== active)
        remove.classList.add("opacity-0", "group-hover:opacity-100", "focus-visible:opacity-100");
      const line = this.el(
        "div",
        `group flex h-[52px] w-full shrink-0 items-center gap-1 rounded-6 pr-1.5 transition-colors ${index === active ? "bg-inset" : "hover:bg-inset"}`,
        row,
        remove,
      );
      if (index === active) line.dataset.active = "true";
      return line;
    });
  }

  /** Delete this login? Nothing goes until 「删除」: the focus starts on 「取消」. */
  private deleteView(item: MenuItem): HTMLElement {
    const t = (key: ExtensionKey, params?: Params) => this.t(key, params);
    const name = item.username ? `${item.title} · ${item.username}` : item.title;
    const cancel = this.button(t("inline.cancel"), "ghost", () => this.act({ type: "close" }));
    queueMicrotask(() => cancel.focus());
    return this.el(
      "div",
      "flex gap-2.5 p-2.5",
      this.el("span", "mt-0.5 text-danger", icon(this.options.doc, "trash", 16)),
      this.el(
        "div",
        "min-w-0 flex-1",
        this.el(
          "p",
          "truncate text-[13px] font-semibold text-fg",
          t("inline.deleteTitle", { name }),
        ),
        this.el("p", "text-[12px] leading-snug text-fg-muted", t("inline.deleteBody")),
        this.el(
          "div",
          "mt-2 flex gap-2",
          this.button(
            t("inline.deleteConfirm"),
            "danger",
            (event) => this.act({ type: "delete" }, event),
            "trash",
          ),
          cancel,
        ),
      ),
    );
  }

  /** The chosen row in sight, where the list scrolls. */
  private showActive(rows: HTMLElement): void {
    rows.querySelector<HTMLElement>("[data-active]")?.scrollIntoView?.({ block: "nearest" });
  }

  /** The box that narrows the list: what it holds goes to the background, which sends the list
   *  again; its arrows move the choice, Enter fills it, Esc empties the box, then closes. */
  private searchBox(text: string): HTMLElement {
    const label = this.t("inline.search");
    const input = this.input(text, label);
    input.type = "text";
    input.setAttribute("aria-label", label);
    input.className = "min-w-0 flex-1 bg-transparent outline-none placeholder:text-fg-subtle";
    input.addEventListener("input", () => this.send({ kind: "search", text: input.value }));
    input.addEventListener("keydown", (event) => {
      const view = this.view;
      if (view?.view !== "list") return;
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        this.send({ kind: "key", key: event.key });
      } else if (event.key === "Enter") {
        event.preventDefault();
        if (view.items.length > 0) this.act({ type: "choose", index: view.active }, event);
      } else if (event.key === "Escape") {
        event.preventDefault();
        if (input.value === "") {
          this.act({ type: "close" });
          return;
        }
        input.value = "";
        this.send({ kind: "search", text: "" });
      }
    });
    return this.el(
      "div",
      "flex h-8 shrink-0 items-center gap-2 rounded-6 bg-surface-solid px-2.5 text-[13px] hairline focus-within:border-fg",
      icon(this.options.doc, "search", 14, "shrink-0 text-fg-subtle"),
      input,
    );
  }

  private signup(view: Extract<InlineView, { view: "signup" }>): HTMLElement {
    const t = (key: ExtensionKey, params?: Params) => this.t(key, params);
    const use = this.el(
      "button",
      "flex w-full items-start gap-2.5 rounded-6 bg-inset px-3 py-2 text-left",
      this.el("span", "mt-0.5 text-accent-text", icon(this.options.doc, "dice", 16)),
      this.el(
        "span",
        "min-w-0 flex-1",
        this.el("span", "block text-[13px] font-semibold text-fg", t("inline.signupTitle")),
        this.el("span", "mono block truncate text-[13px] font-medium text-fg", view.password),
        this.el(
          "span",
          "block text-[11px] text-fg-subtle",
          t(view.rules ? "inline.signupRules" : "inline.signupRandom", {
            n: Array.from(view.password).length,
          }),
        ),
      ),
    );
    use.type = "button";
    use.addEventListener("click", (event) => this.act({ type: "use" }, event));
    const refresh = this.iconButton("refresh", t("inline.regenerate"), (event) =>
      this.act({ type: "regenerate" }, event),
    );
    // Lockra copies it and clears the clipboard later, as it does a login's password.
    const copy = this.iconButton(
      view.copied ? "check" : "copy",
      t(view.copied ? "inline.copied" : "inline.copyPassword"),
      (event) => this.act({ type: "copy" }, event),
    );
    const note = view.username
      ? t("inline.signupNote", { site: view.site, username: view.username })
      : t("inline.signupNoteNoUser", { site: view.site });
    return this.el(
      "div",
      "flex flex-col gap-1",
      this.el("div", "flex items-center gap-1", use, copy, refresh),
      this.el(
        "p",
        "flex items-start gap-2 px-3 py-1.5 text-[11.5px] leading-snug text-fg-muted",
        this.el("span", "mt-0.5 text-fg-subtle", icon(this.options.doc, "info", 14)),
        note,
      ),
    );
  }

  private confirm(item: MenuItem): HTMLElement {
    const t = (key: ExtensionKey, params?: Params) => this.t(key, params);
    return this.el(
      "div",
      "flex gap-3 rounded-6 bg-inset p-2.5",
      this.avatar(item),
      this.el(
        "div",
        "min-w-0 flex-1",
        this.el(
          "p",
          "truncate text-[13px] font-semibold text-fg",
          t("inline.confirmTitle", { title: item.title, username: item.username }),
        ),
        this.el("p", "text-[11.5px] text-fg-subtle", t("inline.confirmNote")),
        this.el(
          "div",
          "mt-2 flex gap-2",
          this.button(t("inline.fill"), "primary", (event) => this.act({ type: "fill" }, event)),
          this.button(t("inline.cancel"), "ghost", () => this.act({ type: "close" })),
        ),
      ),
    );
  }

  private http(): HTMLElement {
    const t = (key: ExtensionKey) => this.t(key);
    return this.el(
      "div",
      "flex gap-2.5 p-2.5",
      this.el("span", "mt-0.5 text-warning", icon(this.options.doc, "alert", 16)),
      this.el(
        "div",
        "min-w-0 flex-1",
        this.el("p", "text-[13px] font-semibold text-fg", t("http.title")),
        this.el("p", "text-[12px] leading-snug text-fg-muted", t("http.body")),
        this.el(
          "div",
          "mt-2 flex gap-2",
          this.button(t("http.fill"), "outline", (event) =>
            this.act({ type: "allow_http" }, event),
          ),
          this.button(t("http.cancel"), "ghost", () => this.act({ type: "close" })),
        ),
      ),
    );
  }

  private message(name: IconName, title: string, body: string): HTMLElement {
    return this.el(
      "div",
      "flex gap-2.5 p-2.5",
      this.el("span", "mt-0.5 text-fg-muted", icon(this.options.doc, name, 16)),
      this.el(
        "div",
        "min-w-0 flex-1",
        this.el("p", "text-[13px] font-semibold text-fg", title),
        this.el("p", "text-[12px] leading-snug text-fg-muted", body),
      ),
    );
  }

  private field(label: string, input: HTMLInputElement, ...after: Child[]): HTMLElement {
    const id = `f-${label.length}-${Math.random().toString(36).slice(2, 8)}`;
    input.id = id;
    input.className = `min-w-0 flex-1 bg-transparent outline-none placeholder:text-fg-subtle ${input.className}`;
    const caption = this.el("label", "text-[12px] text-fg-muted", label);
    caption.htmlFor = id;
    return this.el(
      "div",
      "flex flex-col gap-1",
      caption,
      this.el(
        "div",
        "flex h-8 items-center gap-2 rounded-6 bg-surface-solid px-2.5 text-[13px] hairline focus-within:border-fg",
        input,
        ...after,
      ),
    );
  }

  private input(value: string, placeholder = ""): HTMLInputElement {
    const input = this.options.doc.createElement("input");
    input.value = value;
    input.placeholder = placeholder;
    input.autocomplete = "off";
    input.spellcheck = false;
    return input;
  }

  /** The save or update prompt at the top right. */
  private prompt(view: Extract<InlineView, { view: "save" } | { view: "update" }>): HTMLElement {
    const t = (key: ExtensionKey) => this.t(key);
    const title = this.el(
      "div",
      "flex items-center gap-2.5",
      logo(this.options.doc, 20),
      this.el(
        "span",
        "min-w-0 flex-1 truncate text-[14px] font-semibold text-fg",
        t(view.view === "save" ? "save.title" : "update.title"),
      ),
      this.iconButton("x", t("inline.close"), () => this.act({ type: "close" })),
    );
    // The password as the form sent it, masked, and the user's to correct: what a page sends as a
    // password is not always one.
    const passwordInput = this.input(view.password);
    passwordInput.type = this.revealed ? "text" : "password";
    passwordInput.className = "mono";
    const reveal = this.iconButton(
      this.revealed ? "eyeOff" : "eye",
      t(this.revealed ? "save.hide" : "save.show"),
      () => {
        // In place: drawn again, the fields would lose what the user changed.
        this.revealed = !this.revealed;
        passwordInput.type = this.revealed ? "text" : "password";
        reveal.replaceChildren(icon(this.options.doc, this.revealed ? "eyeOff" : "eye", 14));
        const label = t(this.revealed ? "save.hide" : "save.show");
        reveal.setAttribute("aria-label", label);
        reveal.title = label;
      },
    );
    const password = this.field(t("save.password"), passwordInput, reveal);
    let body: HTMLElement;
    let primary: HTMLButtonElement;
    let dismiss: string;
    if (view.view === "save") {
      const titleInput = this.input(view.title);
      const usernameInput = this.input(view.username);
      const groupInput = this.input("", t("save.groupPlaceholder"));
      body = this.el(
        "div",
        "flex flex-col gap-2.5",
        this.field(t("save.fieldTitle"), titleInput),
        this.field(t("save.username"), usernameInput),
        password,
        this.field(t("save.group"), groupInput),
      );
      primary = this.button(t("save.save"), "primary", (event) =>
        this.act(
          {
            type: "save",
            title: titleInput.value,
            username: usernameInput.value,
            password: passwordInput.value,
            group: groupInput.value,
          },
          event,
        ),
      );
      dismiss = t("save.dismiss");
    } else {
      body = this.el(
        "div",
        "flex flex-col gap-2.5",
        this.el(
          "div",
          "flex h-[52px] items-center gap-3 rounded-6 bg-inset px-3 hairline",
          this.avatar(view.item),
          this.el(
            "span",
            "min-w-0 flex-1",
            this.el("span", "block truncate text-[13px] font-semibold text-fg", view.item.title),
            this.el("span", "block truncate text-[12px] text-fg-muted", view.item.username),
          ),
        ),
        this.el("p", "text-[12.5px] leading-normal text-fg-muted", t("update.body")),
        password,
      );
      primary = this.button(t("update.update"), "primary", (event) =>
        this.act({ type: "update", password: passwordInput.value }, event),
      );
      dismiss = t("update.dismiss");
    }
    // Nothing to save without a password.
    const sync = () => {
      primary.disabled = passwordInput.value === "";
    };
    passwordInput.addEventListener("input", sync);
    sync();
    // A one-time code sent as a password: nothing to save; Lockra can link a code to the site's
    // login instead, picked in Lockra.
    const code = view.code
      ? this.el(
          "div",
          "flex flex-col gap-2 rounded-6 bg-warning-soft px-3 py-2 text-[12px] leading-snug text-fg",
          this.el(
            "div",
            "flex items-start gap-2",
            this.el("span", "mt-0.5 text-warning", icon(this.options.doc, "info", 14)),
            this.el(
              "span",
              "min-w-0 flex-1",
              t("save.codeHint"),
              view.link ? ` ${this.t("save.linkHint", { title: view.link.title })}` : null,
            ),
          ),
          this.el(
            "div",
            "flex justify-end gap-2",
            this.button(t("save.isCode"), "ghost", (event) => this.act({ type: "code" }, event)),
            view.link
              ? this.button(
                  t("save.linkCode"),
                  "outline",
                  (event) => this.act({ type: "link_code" }, event),
                  "key",
                )
              : null,
          ),
        )
      : null;
    const actions = this.el(
      "div",
      "flex items-center gap-2",
      this.button(t("save.never"), "text-muted", (event) => this.act({ type: "never" }, event)),
      this.el("span", "flex-1"),
      this.button(dismiss, "ghost", (event) => this.act({ type: "dismiss" }, event)),
      primary,
    );
    return this.el(
      "div",
      "flex flex-col gap-3.5 overflow-hidden rounded-14 border border-border bg-surface-solid p-4",
      title,
      body,
      code,
      actions,
    );
  }
}
