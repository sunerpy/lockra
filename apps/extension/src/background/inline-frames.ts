// The inline frames the background has handed out: each menu or prompt gets a random token, which
// the content script puts in the frame's URL fragment; a frame that does not bring a known token,
// from the right tab and under the right frame, is shown nothing (plan §四 「防点击劫持」).
import type { Port } from "../browser/api";
import type { ItemSummary, Policy } from "../protocol/schemas";
import type { InlineView } from "../shared/messages";
import type { TrustedFrame } from "./frames";

export interface InlineFrame {
  token: string;
  kind: "menu" | "prompt";
  /** The frame of the page the inline frame sits in, as it was when it opened. */
  frame: TrustedFrame;
  /** Its URL then, to check (with its document in Chromium) that it is still the same page. */
  frameUrl: string;
  view: InlineView;
  /** The logins behind a list, in its order (those its search leaves). */
  items: ItemSummary[];
  /** A list's logins before its search narrowed them. */
  all?: ItemSummary[];
  policy: Policy | null;
  /** The login a confirmation or a warning is about. */
  item: ItemSummary | null;
  /** A sign-up form's generated password and the user name it goes with. */
  generated: {
    password: string;
    username: string;
    rules: string | null;
    minlength: number | null;
    maxlength: number | null;
  } | null;
  /** The inline frame's port, once it connected with this token. */
  port: Port | null;
  /** What the frame's IntersectionObserver v2 last said. */
  visible: boolean | null;
  /** The trigger a fill from it reports. */
  trigger: "inline_menu" | "shortcut";
  /** Opened by a code field: its fill puts the chosen login's code in, and no later step of the
   *  sign-in is waited for. */
  code?: boolean;
}

function randomToken(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export class InlineFrames {
  private readonly frames = new Map<string, InlineFrame>();

  open(frame: Omit<InlineFrame, "token" | "port" | "visible">): InlineFrame {
    // One frame of each kind per tab: a new menu replaces the old one.
    for (const old of this.frames.values()) {
      if (old.frame.tabId === frame.frame.tabId && old.kind === frame.kind) this.close(old.token);
    }
    const opened: InlineFrame = { ...frame, token: randomToken(), port: null, visible: null };
    this.frames.set(opened.token, opened);
    return opened;
  }

  get(token: string): InlineFrame | undefined {
    return this.frames.get(token);
  }

  byPort(port: Port): InlineFrame | undefined {
    for (const frame of this.frames.values()) if (frame.port === port) return frame;
    return undefined;
  }

  /** Close `token`: its frame is told, and forgets the port. */
  close(token: string): InlineFrame | undefined {
    const frame = this.frames.get(token);
    if (!frame) return undefined;
    this.frames.delete(token);
    try {
      frame.port?.postMessage({ kind: "close" });
      frame.port?.disconnect();
    } catch {
      // Gone already.
    }
    return frame;
  }

  inTab(tabId: number): InlineFrame[] {
    return [...this.frames.values()].filter((frame) => frame.frame.tabId === tabId);
  }

  all(): InlineFrame[] {
    return [...this.frames.values()];
  }
}
