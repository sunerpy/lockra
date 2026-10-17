// The messages between the extension's own parts: the content script and the background
// (runtime.sendMessage, tabs.sendMessage), and the ports of the popup, the options page and the
// inline frame. None goes to the page: the content script listens to no `window` message, and the
// inline frame gets its token from its URL's fragment.
import type { CopyPart, ItemSummary, Policy } from "../protocol/schemas";

export const PORT_POPUP = "lockra-popup";
export const PORT_OPTIONS = "lockra-options";
export const PORT_INLINE = "lockra-inline";
/** The command of the fill shortcut (manifest `commands`). */
export const FILL_COMMAND = "fill-login";

// ---- content script → background ------------------------------------------------------------

export type FocusKind = "login" | "signup" | "username" | "otp";

export type ContentMessage =
  /** The content script started in a frame: is anything waiting for this tab? */
  | { kind: "content.start"; top: boolean }
  /** A field the user focused: offer the inline menu. `explicit`: the user clicked the field's
   *  Lockra mark, so the menu opens whatever opens it on focus. */
  | {
      kind: "content.focus";
      form: FocusKind;
      explicit?: true;
      /** Sign-up forms: the page's rules for the generator, and the user name typed. */
      signup?: {
        rules: string | null;
        minlength: number | null;
        maxlength: number | null;
        username: string;
      };
    }
  /** A one-time-code field showed up (or got the focus). */
  | { kind: "content.otp" }
  /** A sign-in form with a password showed up after a fill that put in the user name alone: the
   *  next step of that sign-in. */
  | { kind: "content.next_step" }
  /** A sign-in form on a page that just loaded (only asked when the policy fills as pages load). */
  | { kind: "content.page_load" }
  /** A form was submitted (or its typed password went away with the page). */
  | { kind: "content.submitted"; username: string; password: string; signup: boolean }
  /** The first step of a sign-in in two: the user name, kept for the next step in this tab. */
  | { kind: "content.username"; username: string }
  /** A key on the field while the inline menu is open; `allowed`: the frame passed the checks. */
  | {
      kind: "content.key";
      token: string;
      key: "ArrowDown" | "ArrowUp" | "Enter" | "Escape";
      allowed: boolean;
    }
  /** The inline frame was taken down (focus went elsewhere, the page tampered with it). */
  | { kind: "content.closed"; token: string };

export interface StartAnswer {
  /** The extension may run in pages (site access): the menu, the prompts, the second step's code.
   *  Without it the content script was put in the frame for one fill and does nothing else. */
  features: boolean;
  /** Look for a one-time-code field: a login was filled in this tab a moment ago. */
  otp: boolean;
  /** Fill sign-in forms as the page loads (the policy's choice, top frame only). */
  pageLoad: boolean;
  /** A save or update prompt waits to be shown in this page. */
  prompt: string | null;
  /** Look for a sign-in form with a password: a fill gave the user name alone a moment ago, and
   *  its password comes next. */
  nextStep?: boolean;
}

/** Where the inline menu opens against its field: above it where the browser may list its own
 *  saved passwords under the field (Chromium and Firefox put that list below, above only when
 *  there is no room), below it otherwise. Either way the other side when only it has room. */
export type MenuSide = "above" | "below";

export interface FocusAnswer {
  /** The inline frame's token, when there is a menu to show. */
  token: string | null;
  /** There is one, but it opens only from the field's mark (the options page's choice): show the
   *  mark alone. */
  mark?: true;
  /** Where it opens; below when absent. */
  side?: MenuSide;
}

// ---- background → content script ------------------------------------------------------------

export interface FillValues {
  username: string;
  password: string;
  /** Custom fields by the page field they name. */
  fields: { page_field: string; value: string }[];
  otp: string | null;
  submit: boolean;
  /** A generated password for a sign-up form: every new-password field takes it. */
  newPassword?: boolean;
}

export type BackgroundMessage =
  | { kind: "fill"; values: FillValues }
  /** The code of a second step; `submit`: send its form once the code is in (the app's word). */
  | { kind: "fill.otp"; code: string; submit?: boolean }
  /** Show the inline frame `token` at the focused field (the shortcut's confirmations). */
  | { kind: "menu.open"; token: string; side?: MenuSide }
  /** Show the save or update prompt `token` at the top right. */
  | { kind: "prompt.open"; token: string }
  | { kind: "frame.resize"; token: string; height: number }
  | { kind: "frame.close"; token: string }
  /** May a click at `point` (the frame's coordinates) count? `visible`: the frame's own
   *  IntersectionObserver v2 (null where there is none). */
  | {
      kind: "frame.verify";
      token: string;
      point: { x: number; y: number } | null;
      trusted: boolean;
      visible: boolean | null;
    };

export interface VerifyAnswer {
  ok: boolean;
  reason?: string;
}

export interface FillAnswer {
  filled: number;
}

// ---- the inline frame ------------------------------------------------------------------------

export interface MenuItem {
  title: string;
  username: string;
  downgrade: boolean;
}

export type InlineView =
  | { view: "list"; site: string; items: MenuItem[]; active: number }
  | {
      view: "signup";
      site: string;
      password: string;
      username: string;
      rules: boolean;
      /** Lockra put the password on the clipboard. */
      copied?: boolean;
    }
  | { view: "confirm"; item: MenuItem }
  | { view: "http"; site: string; item: MenuItem }
  | { view: "waiting"; item: MenuItem }
  | { view: "save"; site: string; title: string; username: string; password: string }
  | { view: "update"; site: string; item: MenuItem }
  /** A request the app refused (`refusal`, worded by the frame), or one that failed on the way. */
  | { view: "notice"; site: string; refusal: string };

export type InlineAction =
  | { type: "choose"; index: number }
  | { type: "fill" }
  | { type: "allow_http" }
  | { type: "use" }
  | { type: "regenerate" }
  /** The sign-up menu's password on the clipboard, through Lockra. */
  | { type: "copy" }
  | { type: "save"; title: string; username: string; group: string }
  | { type: "update" }
  | { type: "dismiss" }
  | { type: "never" }
  | { type: "close" };

export type InlineToBackground =
  | { kind: "hello"; token: string }
  | { kind: "size"; height: number }
  | { kind: "visible"; visible: boolean | null }
  | {
      kind: "action";
      action: InlineAction;
      point: { x: number; y: number } | null;
      trusted: boolean;
    };

export type BackgroundToInline = { kind: "view"; view: InlineView } | { kind: "close" };

// ---- the popup and the options page ----------------------------------------------------------

export type Screen =
  | "loading"
  | "no_host"
  | "not_running"
  | "update_app"
  | "update_extension"
  | "no_vault"
  | "locked"
  | "unpaired"
  | "pairing"
  | "ready";

export interface SiteView {
  /** The registrable domain the app named, or the page's host before an answer. */
  site: string;
  items: ItemSummary[];
  /** The page is not a web page (a new tab, the browser's own pages): nothing to fill there. */
  fillable: boolean;
  neverFill: boolean;
  crossOrigin: boolean;
}

/** The extension's own update, as the popup and the options page tell it
 *  (background/update.ts): a new copy in Lockra's folder waits for them to close (`waiting`),
 *  the folder's copy is behind Lockra (`folder`), or the extension came another way and is older
 *  than Lockra (`manual`). */
export type UpdateNote = "waiting" | "folder" | "manual";

export interface PopupState {
  screen: Screen;
  /** Lockra is being started. */
  starting: boolean;
  appVersion: string | null;
  extensionVersion: string;
  /** The pairing in progress: the code both sides show and when the request lapses. */
  pairing: { code: string; expiresAt: number | null } | null;
  /** How the last pairing attempt ended without one. */
  pairingFailed: string | null;
  pageAccess: boolean;
  /** The current tab's matches; null until known. */
  site: SiteView | null;
  policy: Policy | null;
  paused: boolean;
  /** The fill shortcut as the browser shows it ("" when the user removed it). */
  shortcut: string;
  /** Lockra answered `unlock_request` (the window came up). */
  unlockAsked: boolean;
  /** When the extension was paired (the options page). */
  pairedAt: number | null;
  /** The extension's own update; null when there is nothing to tell. */
  update: UpdateNote | null;
}

export type PopupRequest =
  | { kind: "hello" }
  | { kind: "pair" }
  | { kind: "pair.cancel" }
  | { kind: "unlock" }
  | { kind: "start"; show: boolean }
  | { kind: "retry" }
  | { kind: "search"; text: string }
  | { kind: "fill"; itemId: string; allowHttp: boolean }
  | { kind: "copy"; itemId: string; part: CopyPart }
  | { kind: "access.changed" }
  | { kind: "unpair" }
  | { kind: "extensions" };

export type PopupEvent =
  | { kind: "state"; state: PopupState }
  | { kind: "search"; text: string; items: ItemSummary[]; error: string | null }
  /** How a fill or a copy ended: `done`, `waiting` while Lockra asks the user, or the refusal. */
  | {
      kind: "result";
      action: "fill" | "copy";
      itemId: string;
      outcome: string;
      part?: CopyPart["type"];
    };
