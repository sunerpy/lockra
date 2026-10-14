// What a form submitted (plan §四 「保存与更新的检测」): the form's submit event, a click on a
// button of a sign-in form, Enter in its password field, or a password the user typed going away
// (the field removed by a page that never submits, or the page itself unloading). The values go to
// the background, which keeps them in memory for five minutes and asks the app whether to offer
// saving or updating; the first step of a sign-in in two reports its user name the same way.
import { type View, formOf, isFillable, isPassword, scanField } from "./fields";

export interface Reports {
  submitted(submission: { username: string; password: string; signup: boolean }): void;
  username(username: string): void;
}

interface Typed {
  root: HTMLElement;
  username: string;
  password: string;
  signup: boolean;
}

/** The same values again within this are the same submission (the click, then the submit). */
const SAME_MS = 3_000;
const BUTTON =
  'button, input[type="submit" i], input[type="button" i], input[type="image" i], [role="button"]';

export class SubmitWatcher {
  /** Password fields the user typed in, with the values they last had. */
  private readonly typed = new Map<HTMLInputElement, Typed>();
  private last: { username: string; password: string; at: number } | null = null;
  private readonly bound: [EventTarget, string, (event: Event) => void][] = [];

  constructor(
    private readonly doc: Document,
    private readonly win: Window,
    private readonly view: View,
    private readonly reports: Reports,
    private readonly now: () => number,
    /** Only what the user does counts (`isTrusted`); the tests' events are not. */
    private readonly trusted: (event: Event) => boolean = (event) => event.isTrusted,
  ) {}

  start(): void {
    const on = (target: EventTarget, type: string, handler: (event: Event) => void) => {
      target.addEventListener(type, handler, true);
      this.bound.push([target, type, handler]);
    };
    on(this.doc, "input", (event) => this.onInput(event));
    on(this.doc, "submit", (event) => {
      if (event.target instanceof HTMLElement) this.collect(event.target);
    });
    on(this.doc, "click", (event) => this.onClick(event));
    on(this.doc, "keydown", (event) => this.onKey(event));
    on(this.win, "pagehide", () => this.leaving());
  }

  stop(): void {
    for (const [target, type, handler] of this.bound.splice(0))
      target.removeEventListener(type, handler, true);
    this.typed.clear();
  }

  /** Whether a typed password is watched (the page's observer keeps running for it). */
  get watching(): boolean {
    return this.typed.size > 0;
  }

  /** The page changed: a typed password field that is gone took its form with it. */
  check(): void {
    for (const [field, typed] of this.typed) {
      if (field.isConnected) continue;
      this.typed.delete(field);
      this.emit(typed.username, typed.password, typed.signup);
    }
  }

  private onInput(event: Event): void {
    const target = event.target;
    if (!isFillable(target) || !this.trusted(event)) return;
    const scan = scanField(target, this.view);
    if (scan.kind !== "login" && scan.kind !== "signup") return;
    const fields = scan.kind === "signup" ? scan.newPasswords : scan.passwords;
    const password = fields.map((p) => p.value).find((value) => value !== "") ?? "";
    if (password === "") return;
    const typed: Typed = {
      root: scan.root,
      username: scan.username?.value ?? scan.hiddenUsername?.value ?? "",
      password,
      signup: scan.kind === "signup",
    };
    for (const field of scan.passwords) this.typed.set(field, typed);
  }

  private onClick(event: Event): void {
    if (!this.trusted(event) || !(event.target instanceof Element)) return;
    const button = event.target.closest(BUTTON);
    if (!(button instanceof HTMLElement)) return;
    const form = button.closest("form");
    if (form) return this.collect(form);
    for (const typed of this.typed.values()) {
      if (typed.root.contains(button)) return this.collect(typed.root);
    }
  }

  private onKey(event: Event): void {
    if (!(event instanceof KeyboardEvent) || event.key !== "Enter" || !this.trusted(event)) return;
    if (isFillable(event.target)) this.collect(formOf(event.target));
  }

  private leaving(): void {
    this.check();
    for (const typed of new Set(this.typed.values())) this.collect(typed.root);
  }

  /** What `root` holds now: a sign-in's user name and password, or a first step's user name. */
  private collect(root: HTMLElement): void {
    const inputs = Array.from(root.querySelectorAll("input")).filter(isFillable);
    const field = inputs.find((input) => isPassword(input) && this.view.visible(input));
    if (!field) {
      const step = inputs.find(
        (input) => input.value !== "" && scanField(input, this.view).kind === "username",
      );
      if (step) this.reports.username(step.value);
      return;
    }
    const scan = scanField(field, this.view);
    const signup = scan.kind === "signup";
    const password =
      (signup ? scan.newPasswords : scan.passwords)
        .map((p) => p.value)
        .find((value) => value !== "") ?? "";
    if (password === "") return;
    this.emit(scan.username?.value ?? scan.hiddenUsername?.value ?? "", password, signup);
  }

  private emit(username: string, password: string, signup: boolean): void {
    const now = this.now();
    const last = this.last;
    if (last && last.username === username && last.password === password && now - last.at < SAME_MS)
      return;
    this.last = { username, password, at: now };
    this.reports.submitted({ username, password, signup });
  }
}
