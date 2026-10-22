// The content script's work in one frame. It reports what the page shows (a focused field's kind, a
// submitted form, a first step's user name, a code field) and fills what the background sends; it
// reads no address (the background takes them from the browser) and decides nothing. It listens to
// no `window` message and runs only in its isolated world. Pages are scanned on `focusin`, and once
// as it starts, for the field the page focused before it listened (`autofocus`); a
// MutationObserver, at most every 500 ms, runs only while there is something to watch (a typed
// password, a code field to come, an open menu) and is disconnected otherwise.
import type {
  BackgroundMessage,
  CodeAnswer,
  ContentMessage,
  FillValues,
  FocusKind,
  MenuAnswer,
  MenuSide,
  StartAnswer,
} from "../shared/messages";
import type { Measurer } from "./clickjack";
import { FieldIcon } from "./field-icon";
import {
  type FormScan,
  type View,
  findLoginForms,
  findOtp,
  formOf,
  isFillable,
  scanField,
  takesAskedCode,
} from "./fields";
import { type FillResult, fillChosen, fillForm, fillOtp, submitForm } from "./fill";
import { FrameHost } from "./frame-host";
import { SubmitWatcher } from "./submit";

export interface Runtime {
  send(message: ContentMessage): Promise<unknown>;
  /** The background's messages; the handler's promise is the answer, undefined: not ours. */
  listen(handler: (message: unknown) => Promise<unknown> | undefined): void;
  url(path: string): string;
}

export interface AgentOptions {
  doc: Document;
  win: Window;
  runtime: Runtime;
  view: View;
  now: () => number;
  measurer?: Measurer;
  /** Only what the user does counts (`isTrusted`); the tests' events are not. */
  trusted?: (event: Event) => boolean;
}

const OBSERVE_EVERY_MS = 500;
const OTP_FOR_MS = 5 * 60_000;
/** How long the password of a step that gave the user name alone is waited for. */
const STEP_FOR_MS = 2 * 60_000;

type MenuKey = "ArrowDown" | "ArrowUp" | "Enter" | "Escape";

function isMenuKey(key: string): key is MenuKey {
  return key === "ArrowDown" || key === "ArrowUp" || key === "Enter" || key === "Escape";
}

const BACKGROUND_KINDS = new Set([
  "fill",
  "fill.otp",
  "menu.open",
  "prompt.open",
  "frame.resize",
  "frame.close",
  "frame.verify",
]);

/** A message of the background's: only the extension reaches a content script's runtime.onMessage. */
function isBackgroundMessage(raw: unknown): raw is BackgroundMessage {
  return (
    typeof raw === "object" &&
    raw !== null &&
    "kind" in raw &&
    typeof raw.kind === "string" &&
    BACKGROUND_KINDS.has(raw.kind)
  );
}

function isStart(value: unknown): value is StartAnswer {
  return (
    typeof value === "object" &&
    value !== null &&
    "features" in value &&
    "otp" in value &&
    "pageLoad" in value &&
    "prompt" in value
  );
}

/** The background's word that the menu waits for the field's mark. */
function markOnly(value: unknown): boolean {
  return typeof value === "object" && value !== null && "mark" in value && value.mark === true;
}

/** Where the background says the menu opens: above the field where the browser may list its own
 *  passwords under it. */
function sideOf(value: unknown): MenuSide {
  return typeof value === "object" && value !== null && "side" in value && value.side === "above"
    ? "above"
    : "below";
}

function tokenOf(value: unknown): string | null {
  return typeof value === "object" &&
    value !== null &&
    "token" in value &&
    typeof value.token === "string"
    ? value.token
    : null;
}

/** The element that has the focus, inside open shadow roots too. */
function deepActive(doc: Document): Element | null {
  let active = doc.activeElement;
  while (active?.shadowRoot?.activeElement) active = active.shadowRoot.activeElement;
  return active;
}

export class ContentAgent {
  private started = false;
  private features = false;
  private menu: FrameHost | null = null;
  private anchor: HTMLInputElement | null = null;
  /** The field whose focus the background is being told of: once is enough. */
  private asking: HTMLInputElement | null = null;
  /** The fills the background sent, counted: whether a fill as the page loads put anything in. */
  private fills = 0;
  /** The user moved into the list with an arrow (or the shortcut opened it): Enter is ours. */
  private engaged = false;
  private prompt: FrameHost | null = null;
  private readonly icon: FieldIcon;
  private readonly watcher: SubmitWatcher;
  private observer: MutationObserver | null = null;
  private lastObserved = 0;
  private observeTimer: ReturnType<typeof setTimeout> | undefined;
  private otpUntil = 0;
  private stepUntil = 0;
  /** The page set `open` (a dialog) since the last look: what it opened may cover the menu. */
  private opened = false;
  private readonly trusted: (event: Event) => boolean;
  private readonly bound: [EventTarget, string, (event: Event) => void, boolean][] = [];

  constructor(private readonly options: AgentOptions) {
    this.trusted = options.trusted ?? ((event) => event.isTrusted);
    this.icon = new FieldIcon(
      options.doc,
      options.runtime.url("icons/icon-32.png"),
      (anchor) => {
        if (anchor instanceof HTMLInputElement) void this.focused(anchor, true);
      },
      this.trusted,
    );
    this.watcher = new SubmitWatcher(
      options.doc,
      options.win,
      options.view,
      {
        submitted: (submission) => void this.send({ kind: "content.submitted", ...submission }),
        username: (username) => void this.send({ kind: "content.username", username }),
      },
      options.now,
      this.trusted,
    );
  }

  /** The inline menu's iframe and host, for the tests. */
  get menuFrame(): HTMLIFrameElement | null {
    return this.menu?.frame ?? null;
  }

  get menuHost(): HTMLElement | null {
    return this.menu?.host ?? null;
  }

  async start(): Promise<void> {
    if (this.started) return;
    this.started = true;
    const { doc, win, runtime } = this.options;
    runtime.listen((message) => this.receive(message));
    const answer = await this.send({ kind: "content.start", top: win === win.top });
    if (!isStart(answer)) return;
    this.features = answer.features;
    // Put in the frame for one fill (activeTab, no site access): nothing more.
    if (!this.features) return;
    const on = (
      target: EventTarget,
      type: string,
      handler: (event: Event) => void,
      capture = true,
    ) => {
      target.addEventListener(type, handler, { capture, passive: type === "scroll" });
      this.bound.push([target, type, handler, capture]);
    };
    on(doc, "focusin", (event) => this.onFocusIn(event));
    on(doc, "focusout", (event) => this.onFocusOut(event));
    on(doc, "keydown", (event) => this.onKey(event));
    // The page opening a popover, a dialog or full screen over the menu: the menu goes back on top.
    on(doc, "toggle", (event) => {
      if (
        event.target !== this.menu?.host &&
        event.target !== this.prompt?.host &&
        "newState" in event &&
        event.newState === "open"
      )
        this.raise();
    });
    on(doc, "fullscreenchange", () => this.raise());
    on(win, "scroll", () => this.layout());
    on(win, "resize", () => this.layout());
    on(win, "pagehide", () => this.closeAll(), false);
    this.watcher.start();
    if (answer.prompt) this.openPrompt(answer.prompt);
    // The start may hand the page to a fill of its own: the code of the login just filled, the
    // password of the step whose user name was, a fill as the page loads.
    let handed = false;
    if (answer.otp) {
      this.otpUntil = this.options.now() + OTP_FOR_MS;
      handed = this.lookForOtp();
    }
    if (answer.nextStep) {
      this.stepUntil = this.options.now() + STEP_FOR_MS;
      handed = this.lookForStep() || handed;
    }
    const fills = this.fills;
    if (answer.pageLoad && findLoginForms(doc, this.options.view).length > 0)
      await this.send({ kind: "content.page_load" });
    if (!handed && this.fills === fills) this.focusedBefore();
    this.watch();
  }

  /** The field the page focused before this script listened (`autofocus`, a script as the page
   *  loaded, all before document_idle and the background's answer): no focusin comes for it, so
   *  it counts as focused now. In a frame, only while the focus is in the frame: a frame keeps its
   *  field after the focus went to another. */
  private focusedBefore(): void {
    const { doc, win } = this.options;
    const field = deepActive(doc);
    if (!isFillable(field) || (win !== win.top && !doc.hasFocus())) return;
    void this.focused(field, false);
  }

  stop(): void {
    for (const [target, type, handler, capture] of this.bound.splice(0))
      target.removeEventListener(type, handler, capture);
    this.watcher.stop();
    this.observer?.disconnect();
    this.observer = null;
    clearTimeout(this.observeTimer);
    this.closeAll();
    this.started = false;
  }

  private async send(message: ContentMessage): Promise<unknown> {
    try {
      return await this.options.runtime.send(message);
    } catch {
      // The background restarted or the extension went away: nothing to do in the page.
      return undefined;
    }
  }

  // ---- the menu --------------------------------------------------------------------------------

  private onFocusIn(event: Event): void {
    const target = event.composedPath()[0];
    if (target === this.menu?.host || target === this.icon.host) return;
    if (!isFillable(target)) {
      this.closeMenu();
      this.hideMark();
      return;
    }
    void this.focused(target, false);
  }

  private async focused(field: HTMLInputElement, again: boolean): Promise<void> {
    if (!again && (this.asking === field || (this.anchor === field && this.menu))) return;
    const scan = scanField(field, this.options.view);
    // A sign-up form's generator belongs under its new password, not its user name.
    if (scan.kind === "none" || (scan.kind === "signup" && !scan.newPasswords.includes(field))) {
      this.closeMenu();
      return;
    }
    this.watch();
    const message = this.focusMessage(scan, again);
    this.asking = field;
    const answer = await this.send(message);
    if (this.asking === field) this.asking = null;
    const token = tokenOf(answer);
    if (deepActive(this.options.doc) !== field) return;
    if (token) {
      this.openMenu(token, field, false, sideOf(answer));
      return;
    }
    // The menu opens only from the field's mark (the options page's choice): the mark alone.
    if (markOnly(answer)) {
      this.closeMenu();
      this.icon.show(field);
    }
  }

  private hideMark(): void {
    if (!this.menu) this.icon.hide();
  }

  private focusMessage(scan: FormScan, explicit: boolean): ContentMessage {
    const form: FocusKind = scan.kind === "none" ? "login" : scan.kind;
    const asked = explicit ? { explicit: true as const } : {};
    if (form !== "signup") return { kind: "content.focus", form, ...asked };
    return {
      kind: "content.focus",
      form,
      ...asked,
      signup: {
        rules: scan.rules,
        minlength: scan.minlength,
        maxlength: scan.maxlength,
        username: scan.username?.value ?? "",
      },
    };
  }

  private openMenu(
    token: string,
    anchor: HTMLInputElement,
    engaged: boolean,
    side: MenuSide,
  ): void {
    this.closeMenu();
    const host = new FrameHost({
      doc: this.options.doc,
      url: this.options.runtime.url("inline.html"),
      token,
      kind: "menu",
      now: this.options.now,
      onTampered: () => this.closeMenu(),
      ...(this.options.measurer ? { measurer: this.options.measurer } : {}),
    });
    this.menu = host;
    this.anchor = anchor;
    this.engaged = engaged;
    host.show({ anchor, side });
    this.icon.show(anchor);
  }

  private openPrompt(token: string): void {
    this.prompt?.remove();
    const host = new FrameHost({
      doc: this.options.doc,
      url: this.options.runtime.url("inline.html"),
      token,
      kind: "prompt",
      now: this.options.now,
      onTampered: () => this.closePrompt(),
      ...(this.options.measurer ? { measurer: this.options.measurer } : {}),
    });
    this.prompt = host;
    host.show("corner");
  }

  private closeMenu(): void {
    const menu = this.menu;
    if (!menu) return;
    this.menu = null;
    this.anchor = null;
    this.engaged = false;
    menu.remove();
    this.icon.hide();
    void this.send({ kind: "content.closed", token: menu.token });
  }

  private closePrompt(): void {
    const prompt = this.prompt;
    if (!prompt) return;
    this.prompt = null;
    prompt.remove();
    void this.send({ kind: "content.closed", token: prompt.token });
  }

  private closeAll(): void {
    this.closeMenu();
    this.closePrompt();
  }

  private onFocusOut(event: Event): void {
    const menu = this.menu;
    if (!menu) {
      // The mark alone goes with its field's focus, once the browser has moved it.
      const field = this.icon.field;
      if (field)
        setTimeout(() => {
          if (!this.menu && this.icon.field === field && deepActive(this.options.doc) !== field)
            this.icon.hide();
        }, 0);
      return;
    }
    // Focus that goes into the menu's own frame stays with the menu.
    if (event instanceof FocusEvent && event.relatedTarget === menu.host) return;
    // Where the focus went is known once the browser has moved it: a task later, not a microtask
    // (those run between this event and the move).
    setTimeout(() => {
      const active = deepActive(this.options.doc);
      if (this.menu === menu && active !== this.anchor && active !== menu.host) this.closeMenu();
    }, 0);
  }

  private onKey(event: Event): void {
    if (
      !(event instanceof KeyboardEvent) ||
      !isMenuKey(event.key) ||
      !this.menu ||
      !this.trusted(event)
    )
      return;
    if (event.composedPath()[0] !== this.anchor) return;
    const token = this.menu.token;
    const key = event.key;
    if (key === "Enter" && !this.engaged) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (key === "ArrowDown" || key === "ArrowUp") this.engaged = true;
    // Enter fills: the same checks as a click, at the frame's middle.
    const allowed = key === "Enter" ? this.menu.verify(null, true, null).ok : false;
    void this.send({ kind: "content.key", token, key, allowed });
    if (key === "Escape") this.closeMenu();
  }

  private raise(): void {
    this.menu?.raise();
    this.prompt?.raise();
  }

  private layout(): void {
    this.menu?.layout();
    this.prompt?.layout();
    this.icon.layout();
  }

  // ---- what the background sends ----------------------------------------------------------------

  private receive(message: unknown): Promise<unknown> | undefined {
    if (!isBackgroundMessage(message)) return undefined;
    switch (message.kind) {
      case "fill":
        return Promise.resolve(this.fill(message.values));
      case "fill.otp": {
        const code = this.fillCode(message.code, message.submit === true, message.focused === true);
        return Promise.resolve({ code } satisfies CodeAnswer);
      }
      case "menu.open": {
        const active = deepActive(this.options.doc);
        const opened = active instanceof HTMLInputElement && isFillable(active);
        if (opened) this.openMenu(message.token, active, true, sideOf(message));
        return Promise.resolve({ opened } satisfies MenuAnswer);
      }
      case "prompt.open":
        this.openPrompt(message.token);
        return Promise.resolve(null);
      case "frame.resize":
        this.hostFor(message.token)?.resize(message.height);
        return Promise.resolve(null);
      case "frame.close": {
        if (this.menu?.token === message.token) this.closeMenu();
        if (this.prompt?.token === message.token) this.closePrompt();
        return Promise.resolve(null);
      }
      case "frame.verify": {
        const host = this.hostFor(message.token);
        return Promise.resolve(
          host
            ? host.verify(message.point, message.trusted, message.visible)
            : { ok: false, reason: "tampered" },
        );
      }
      default:
        return undefined;
    }
  }

  private hostFor(token: string): FrameHost | null {
    if (this.menu?.token === token) return this.menu;
    if (this.prompt?.token === token) return this.prompt;
    return null;
  }

  /** Fill the form the menu sits on, else the focused field's, else the page's sign-in form; then
   *  watch this page for what comes next: the password of a step that took the user name alone,
   *  and the code of a login that has one (a sign-in that changes the page without loading
   *  another; the background tells the next page). */
  private fill(values: FillValues): FillResult {
    const view = this.options.view;
    const active = deepActive(this.options.doc);
    const field =
      this.anchor ?? (active instanceof HTMLInputElement && isFillable(active) ? active : null);
    const scan = field ? scanField(field, view) : findLoginForms(this.options.doc, view)[0];
    if (!scan) return { filled: 0, password: false, code: false };
    // A field Lockra cannot tell, chosen by the user (the menu shortcut opened the menu on it, the
    // fill shortcut was pressed in it): the user name goes there.
    const result =
      scan.kind === "none" && field
        ? fillChosen(field, values, view)
        : fillForm(scan, values, view);
    if (result.filled > 0) this.fills += 1;
    const now = this.options.now();
    if (result.password) this.stepUntil = 0;
    // A code's page asks for no password after it.
    else if (result.filled > 0 && !values.newPassword && scan.kind !== "otp")
      this.stepUntil = now + STEP_FOR_MS;
    if (values.otp !== null && !result.code) this.otpUntil = now + OTP_FOR_MS;
    this.watch();
    return result;
  }

  /** The code into the page's code field, the focused one first. Asked for by the user
   *  (`asked`), it goes into the focused field where Lockra can tell no code field, though never
   *  into a sign-in's password field (`takesAskedCode`). Whether it went in. */
  private fillCode(code: string, submit: boolean, asked: boolean): boolean {
    const view = this.options.view;
    const active = deepActive(this.options.doc);
    const field = active instanceof HTMLInputElement && isFillable(active) ? active : null;
    const focused = field ? scanField(field, view) : null;
    let fields = focused?.kind === "otp" ? focused.otp : findOtp(this.options.doc, view);
    if (!fields && asked && field && takesAskedCode(field, view)) fields = [field];
    const first = fields?.[0];
    const filled = fields !== null && first !== undefined && fillOtp(fields, code);
    if (filled && submit) void submitForm(formOf(first), view);
    this.otpUntil = 0;
    return filled;
  }

  // ---- watching the page ------------------------------------------------------------------------

  /** Whether the page shows the code field now (the background is asked for the code). */
  private lookForOtp(): boolean {
    if (this.options.now() >= this.otpUntil) {
      this.otpUntil = 0;
      return false;
    }
    if (!findOtp(this.options.doc, this.options.view)) return false;
    this.otpUntil = 0;
    void this.send({ kind: "content.otp" });
    return true;
  }

  /** The next step of a sign-in: whether a form with a password shows now (the background is
   *  asked to fill it, once). */
  private lookForStep(): boolean {
    if (this.options.now() >= this.stepUntil) {
      this.stepUntil = 0;
      return false;
    }
    if (findLoginForms(this.options.doc, this.options.view).length === 0) return false;
    this.stepUntil = 0;
    void this.send({ kind: "content.next_step" });
    return true;
  }

  /** What the observer still has to watch for. */
  private get watching(): boolean {
    return this.watcher.watching || this.otpUntil > 0 || this.stepUntil > 0 || this.menu !== null;
  }

  /** Observe the page while there is something to watch, at most every 500 ms. */
  private watch(): void {
    if (this.observer || !this.watching) return;
    this.observer = new MutationObserver((records) => this.observed(records));
    this.observer.observe(this.options.doc.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["open"],
    });
  }

  private observed(records: readonly MutationRecord[]): void {
    if (records.some((record) => record.type === "attributes" && record.attributeName === "open"))
      this.opened = true;
    if (this.observeTimer !== undefined) return;
    const wait = Math.max(0, this.lastObserved + OBSERVE_EVERY_MS - this.options.now());
    this.observeTimer = setTimeout(() => {
      this.observeTimer = undefined;
      this.lastObserved = this.options.now();
      this.tick();
    }, wait);
  }

  private tick(): void {
    this.watcher.check();
    if (this.otpUntil > 0) this.lookForOtp();
    if (this.stepUntil > 0) this.lookForStep();
    if (this.menu && this.anchor && !this.anchor.isConnected) this.closeMenu();
    // A dialog the page opened (`open` set) may cover the menu. Nothing else does (popovers and full
    // screen have their own events), and raising it for nothing costs it Chrome's visibility check:
    // after, IntersectionObserver v2 kept the frame unseen and every Enter or click was refused.
    if (this.opened) {
      this.opened = false;
      this.raise();
    }
    if (!this.watching) {
      this.observer?.disconnect();
      this.observer = null;
    }
  }
}
