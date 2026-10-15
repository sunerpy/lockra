// The background's work between the browser and the connection to Lockra: content scripts asking
// for the inline menu or reporting a submitted form, the inline frames and their clicks, the
// shortcut, the popup and the options page, site access. Every address it sends comes from the
// browser (frames.ts); every decision is the app's, reported in its answers; what this side keeps is
// in memory, for minutes, and goes as Lockra locks.
import {
  type BrowserApi,
  type MessageListener,
  type MessageSender,
  type Port,
  type Tab,
  TARGET,
} from "../browser/api";
import type { FillAnswer, ItemSummary, Policy, QueryAnswer } from "../protocol/schemas";
import {
  type BackgroundMessage,
  type ContentMessage,
  FILL_COMMAND,
  type FocusAnswer,
  type InlineAction,
  type InlineView,
  type MenuItem,
  PORT_INLINE,
  PORT_OPTIONS,
  PORT_POPUP,
  type PopupEvent,
  type PopupRequest,
  type PopupState,
  type Screen,
  type SiteView,
  type StartAnswer,
} from "../shared/messages";
import { syncContentScript } from "./access";
import type { Clock } from "./clock";
import type { Connection, ConnectionView } from "./connection";
import { type FramesOf, type TrustedFrame, factsForFrame, factsFromSender, webUrl } from "./frames";
import { type InlineFrame, InlineFrames } from "./inline-frames";
import { chooseFrame, probe } from "./shortcut";
import { type Submission, TabMemory } from "./tabs";
import { contentMessage, inlineMessage, popupMessage } from "./validate";
import { menuOnFocus } from "../shared/prefs";

export interface ControllerDeps {
  api: BrowserApi;
  connection: Connection;
  clock: Clock;
  extensionVersion: string;
}

interface PopupContext {
  tab: Tab | null;
  /** The frame a fill from the popup goes to, picked as it opened. */
  frame: TrustedFrame | null;
  site: SiteView | null;
}

const SNAPSHOT = "snapshot";
const MAX_FRAME_HEIGHT = 560;

function menuItem(item: ItemSummary): MenuItem {
  return { title: item.title, username: item.username || item.email, downgrade: item.downgrade };
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "";
  }
}

export class Controller {
  private readonly api: BrowserApi;
  private readonly connection: Connection;
  private readonly memory: TabMemory;
  private readonly frames = new InlineFrames();
  private readonly popups = new Map<Port, PopupContext>();
  private readonly options = new Set<Port>();
  private readonly framesOf: FramesOf;
  private policy: Policy | null = null;
  private paused = false;
  private access = false;
  private unlockAsked = false;
  private shortcut = "";
  private lastSession: ConnectionView["session"] = "none";

  constructor(private readonly deps: ControllerDeps) {
    this.api = deps.api;
    this.connection = deps.connection;
    this.memory = new TabMemory(deps.clock);
    this.framesOf = (tabId) => this.api.webNavigation.getAllFrames({ tabId });
  }

  install(): void {
    const api = this.api;
    api.runtime.onMessage.addListener(this.onMessage);
    api.runtime.onConnect.addListener((port) => this.onConnect(port));
    api.commands.onCommand.addListener((command, tab) => {
      if (command === FILL_COMMAND) void this.fillByShortcut(tab);
    });
    api.permissions.onAdded.addListener(() => void this.syncAccess(true));
    api.permissions.onRemoved.addListener(() => void this.syncAccess(true));
    api.runtime.onStartup.addListener(() => void this.syncAccess(true));
    // Registered content scripts do not survive an update (Firefox): register them again.
    api.runtime.onInstalled.addListener(() => void this.syncAccess(true));
    api.tabs.onRemoved.addListener((tabId) => {
      this.memory.forget(tabId);
      for (const frame of this.frames.inTab(tabId)) this.frames.close(frame.token);
    });
    this.connection.subscribe((view) => this.onView(view));
    this.connection.onLocked(() => void this.locked());
    void this.syncAccess(false);
    void this.readShortcut();
  }

  // ---- content scripts ---------------------------------------------------------------------

  private readonly onMessage: MessageListener = (raw, sender, sendResponse) => {
    if (sender.id !== this.api.runtime.id || !sender.tab) return undefined;
    const message = contentMessage(raw);
    if (!message) return undefined;
    this.content(message, sender).then(
      (answer) => sendResponse(answer),
      () => sendResponse(undefined),
    );
    return true;
  };

  private async content(message: ContentMessage, sender: MessageSender): Promise<unknown> {
    const tabId = sender.tab?.id ?? -1;
    switch (message.kind) {
      case "content.start":
        return this.contentStart(sender, message.top);
      case "content.focus":
        return this.focus(sender, message);
      case "content.otp": {
        const frame = await this.frameOf(sender);
        if (frame) await this.fillOtp(frame);
        return null;
      }
      case "content.page_load":
        return this.pageLoad(sender);
      case "content.submitted":
        return this.submitted(sender, message.username, message.password);
      case "content.username":
        this.memory.rememberUsername(tabId, message.username);
        return null;
      case "content.key":
        return this.key(sender, message.token, message.key, message.allowed);
      case "content.closed": {
        const frame = this.frames.get(message.token);
        if (frame && frame.frame.tabId === tabId && frame.frame.frameId === sender.frameId)
          this.frames.close(frame.token);
        return null;
      }
    }
  }

  private async frameOf(sender: MessageSender): Promise<TrustedFrame | null> {
    const facts = await factsFromSender(sender, this.framesOf);
    return "error" in facts ? null : facts;
  }

  private async contentStart(sender: MessageSender, top: boolean): Promise<StartAnswer> {
    this.connection.ensure(true);
    const tabId = sender.tab?.id ?? -1;
    const isTop = top && sender.frameId === 0;
    const answer: StartAnswer = {
      features: this.access,
      otp: this.memory.filledItem(tabId) !== null && (this.policy?.fill_otp ?? true),
      pageLoad:
        isTop && this.policy?.on_page_load === true && this.connection.view.session === "open",
      prompt: null,
    };
    const submission = isTop ? this.memory.submission(tabId) : null;
    if (submission) {
      // The page went on after the form was sent: this document shows the prompt.
      const frame = await this.frameOf(sender);
      if (frame) answer.prompt = this.openPrompt(submission, frame).token;
    }
    return answer;
  }

  private async focus(
    sender: MessageSender,
    message: Extract<ContentMessage, { kind: "content.focus" }>,
  ): Promise<FocusAnswer> {
    const none: FocusAnswer = { token: null };
    const frame = await this.frameOf(sender);
    if (!frame) return none;
    if (message.form === "otp") {
      await this.fillOtp(frame);
      return none;
    }
    // In-page help only with a session; the popup says what is missing otherwise.
    if (this.connection.view.session !== "open") {
      this.connection.ensure(true);
      return none;
    }
    if (message.form === "signup") {
      if (!(await this.opensNow(message))) return { token: null, mark: true };
      return this.openSignup(frame, message.signup);
    }
    const answer = await this.query(frame);
    if (!answer || answer.never_fill || answer.items.length === 0) return none;
    if (answer.cross_origin && answer.policy.cross_origin_frames !== "ask") return none;
    if (!(await this.opensNow(message))) return { token: null, mark: true };
    const opened = this.frames.open({
      kind: "menu",
      frame,
      frameUrl: frame.facts.frame_url,
      view: { view: "list", site: answer.site, items: answer.items.map(menuItem), active: 0 },
      items: answer.items,
      policy: answer.policy,
      item: null,
      generated: null,
      trigger: "inline_menu",
    });
    return { token: opened.token };
  }

  /** The menu opens now: the user clicked the field's mark, or it opens as a field gets the focus
   *  (the options page's choice, read each time so a change counts at once). */
  private async opensNow(
    message: Extract<ContentMessage, { kind: "content.focus" }>,
  ): Promise<boolean> {
    return message.explicit === true || (await menuOnFocus(this.api.storage.local));
  }

  private async query(frame: TrustedFrame): Promise<QueryAnswer | null> {
    const outcome = await this.connection.request({
      op: "query",
      ...frame.facts,
      purpose: "fill",
      username: "",
      password: "",
    });
    if (!outcome.ok) return null;
    this.policy = outcome.value.policy;
    return outcome.value;
  }

  private async openSignup(
    frame: TrustedFrame,
    signup: Extract<ContentMessage, { kind: "content.focus" }>["signup"],
  ): Promise<FocusAnswer> {
    const rules = signup ?? { rules: null, minlength: null, maxlength: null, username: "" };
    const made = await this.generate(frame, rules);
    if (!made) return { token: null };
    const username = rules.username.trim() !== "" ? rules.username : made.username;
    const opened = this.frames.open({
      kind: "menu",
      frame,
      frameUrl: frame.facts.frame_url,
      view: {
        view: "signup",
        site: hostOf(frame.facts.frame_url),
        password: made.password,
        username,
        rules: rules.rules !== null,
      },
      items: [],
      policy: this.policy,
      item: null,
      generated: {
        password: made.password,
        username,
        rules: rules.rules,
        minlength: rules.minlength,
        maxlength: rules.maxlength,
      },
      trigger: "inline_menu",
    });
    return { token: opened.token };
  }

  private async generate(
    frame: TrustedFrame,
    rules: { rules: string | null; minlength: number | null; maxlength: number | null },
  ): Promise<{ password: string; username: string } | null> {
    const outcome = await this.connection.request({
      op: "generate",
      ...frame.facts,
      rules: rules.rules,
      minlength: rules.minlength,
      maxlength: rules.maxlength,
      save: null,
    });
    return outcome.ok
      ? { password: outcome.value.password, username: outcome.value.username }
      : null;
  }

  private async fillOtp(frame: TrustedFrame): Promise<void> {
    const itemId = this.memory.filledItem(frame.tabId);
    if (!itemId) return;
    const outcome = await this.connection.request({ op: "otp", item_id: itemId, ...frame.facts });
    if (!outcome.ok) return;
    await this.send(frame, { kind: "fill.otp", code: outcome.value.code });
  }

  private async pageLoad(sender: MessageSender): Promise<null> {
    if (this.policy?.on_page_load !== true) return null;
    const frame = await this.frameOf(sender);
    if (!frame || frame.frameId !== 0) return null;
    const answer = await this.query(frame);
    // The app checks every condition again; one match is the first of them.
    if (!answer || answer.items.length !== 1 || !answer.policy.on_page_load) return null;
    const item = answer.items[0];
    if (!item) return null;
    const outcome = await this.connection.request({
      op: "fill",
      item_id: item.item_id,
      ...frame.facts,
      trigger: "page_load",
      allow_http: false,
    });
    if (outcome.ok) await this.deliver(frame, outcome.value);
    return null;
  }

  private async submitted(sender: MessageSender, typed: string, password: string): Promise<null> {
    if (password === "") return null;
    const frame = await this.frameOf(sender);
    if (!frame) return null;
    const username = typed.trim() !== "" ? typed : (this.memory.username(frame.tabId) ?? "");
    const previous = this.memory.submission(frame.tabId);
    // The same form reported twice (its submit, then the page going away).
    if (previous && previous.username === username && previous.password === password) return null;
    const outcome = await this.connection.request({
      op: "query",
      ...frame.facts,
      purpose: "save",
      username,
      password,
    });
    if (!outcome.ok || !outcome.value.save || outcome.value.save.action === "none") return null;
    const advice = outcome.value.save;
    const item = outcome.value.items.find((i) => i.item_id === advice.item_id) ?? null;
    const submission: Submission = {
      username,
      password,
      frame,
      action: advice.action === "update" && item ? "update" : "save",
      item,
      site: outcome.value.site,
    };
    this.memory.rememberSubmission(frame.tabId, submission);
    const top = await this.topFrame(frame.tabId);
    if (top) {
      const opened = this.openPrompt(submission, top);
      await this.send(top, { kind: "prompt.open", token: opened.token }).catch(() => undefined);
    }
    return null;
  }

  private async topFrame(tabId: number): Promise<TrustedFrame | null> {
    const tab = await this.api.tabs.get(tabId).catch(() => null);
    const facts = await factsForFrame({ tabId, frameId: 0, tabUrl: tab?.url }, this.framesOf);
    return "error" in facts ? null : facts;
  }

  private openPrompt(submission: Submission, top: TrustedFrame): InlineFrame {
    const view: InlineView =
      submission.action === "update" && submission.item
        ? { view: "update", site: submission.site, item: menuItem(submission.item) }
        : {
            view: "save",
            site: submission.site,
            title: submission.site,
            username: submission.username,
            password: submission.password,
          };
    return this.frames.open({
      kind: "prompt",
      frame: top,
      frameUrl: top.facts.frame_url,
      view,
      items: [],
      policy: this.policy,
      item: submission.item,
      generated: null,
      trigger: "inline_menu",
    });
  }

  private async key(
    sender: MessageSender,
    token: string,
    key: "ArrowDown" | "ArrowUp" | "Enter" | "Escape",
    allowed: boolean,
  ): Promise<null> {
    const frame = this.frames.get(token);
    if (!frame || frame.frame.tabId !== sender.tab?.id || frame.frame.frameId !== sender.frameId)
      return null;
    const view = frame.view;
    if (key === "Escape") return this.close(frame);
    if (
      (key === "ArrowDown" || key === "ArrowUp") &&
      view.view === "list" &&
      view.items.length > 0
    ) {
      const step = key === "ArrowDown" ? 1 : -1;
      this.show(frame, {
        ...view,
        active: (view.active + step + view.items.length) % view.items.length,
      });
      return null;
    }
    if (key !== "Enter") return null;
    if (!allowed || frame.visible === false) return this.close(frame);
    const action: InlineAction | null =
      view.view === "list"
        ? { type: "choose", index: view.active }
        : view.view === "signup"
          ? { type: "use" }
          : view.view === "confirm"
            ? { type: "fill" }
            : null;
    if (action) await this.perform(frame, action);
    return null;
  }

  /** Tell a content script, in the document it spoke from. */
  private send(frame: TrustedFrame, message: BackgroundMessage): Promise<unknown> {
    // Firefox checks the options strictly and takes no documentId there.
    const options =
      TARGET === "chromium" && frame.documentId
        ? { frameId: frame.frameId, documentId: frame.documentId }
        : { frameId: frame.frameId };
    return this.api.tabs.sendMessage(frame.tabId, message, options);
  }

  private async deliver(
    frame: TrustedFrame,
    answer: FillAnswer,
    newPassword = false,
  ): Promise<void> {
    const values = {
      username: answer.username,
      password: answer.password,
      fields: answer.fields,
      otp: answer.otp,
      submit: answer.submit,
    };
    await this.send(frame, {
      kind: "fill",
      values: newPassword ? { ...values, newPassword } : values,
    }).catch(() => undefined);
    this.memory.rememberFill(frame.tabId, answer.item_id);
  }

  // ---- the inline frames ---------------------------------------------------------------------

  private onConnect(port: Port): void {
    const url = port.sender?.url ?? "";
    // One of the extension's own pages: its id, its scheme, its path. (The inline frame's address
    // carries a dynamic id in Chromium, so the origin is not compared.)
    const own = new URL(this.api.runtime.getURL("")).protocol;
    const from = (page: string) => {
      if (port.sender?.id !== this.api.runtime.id) return false;
      try {
        const address = new URL(url);
        return address.protocol === own && address.pathname === `/${page}`;
      } catch {
        return false;
      }
    };
    if (port.name === PORT_INLINE && from("inline.html") && port.sender?.tab)
      return this.inlineConnected(port);
    if (port.name === PORT_POPUP && from("popup.html")) return this.pageConnected(port, "popup");
    if (port.name === PORT_OPTIONS && from("options.html"))
      return this.pageConnected(port, "options");
    port.disconnect();
  }

  private inlineConnected(port: Port): void {
    port.onMessage.addListener((raw) => void this.inline(port, raw));
    port.onDisconnect.addListener(() => {
      const frame = this.frames.byPort(port);
      if (frame) frame.port = null;
    });
  }

  private async inline(port: Port, raw: unknown): Promise<void> {
    const message = inlineMessage(raw);
    if (!message) return;
    if (message.kind === "hello") return this.inlineHello(port, message.token);
    const frame = this.frames.byPort(port);
    if (!frame) return;
    switch (message.kind) {
      case "size": {
        const height = Math.max(0, Math.min(MAX_FRAME_HEIGHT, Math.ceil(message.height)));
        await this.send(frame.frame, { kind: "frame.resize", token: frame.token, height }).catch(
          () => undefined,
        );
        return;
      }
      case "visible":
        frame.visible = message.visible;
        return;
      case "action": {
        if (message.action.type === "close") {
          await this.close(frame);
          return;
        }
        if (!(await this.verify(frame, message.point, message.trusted))) {
          await this.close(frame);
          return;
        }
        await this.perform(frame, message.action);
      }
    }
  }

  /** The frame brings a token: it must be the one handed out for this tab, and sit right under the
   *  frame it was handed to. Anything else is shown nothing. */
  private async inlineHello(port: Port, token: string): Promise<void> {
    const frame = this.frames.get(token);
    const sender = port.sender;
    let valid =
      frame !== undefined &&
      frame.port === null &&
      sender?.tab?.id === frame.frame.tabId &&
      sender.frameId !== undefined;
    if (valid && frame && sender) {
      const tree = await this.framesOf(frame.frame.tabId).catch(() => null);
      const self = tree?.find((f) => f.frameId === sender.frameId);
      // Where the browser lists the frame, it must sit under the one that asked for it. Chromium's
      // webNavigation leaves the extension's own frames out (the browser smoke test found): there
      // the token binds it, which only the content script that made the frame was handed, and
      // which only ever went into that frame's address, inside a closed shadow root.
      valid =
        self === undefined
          ? TARGET === "chromium" && tree !== null
          : self.parentFrameId === frame.frame.frameId;
    }
    if (!valid || !frame) {
      port.disconnect();
      return;
    }
    frame.port = port;
    port.postMessage({ kind: "view", view: frame.view });
  }

  private async verify(
    frame: InlineFrame,
    point: { x: number; y: number } | null,
    trusted: boolean,
  ): Promise<boolean> {
    if (!trusted || frame.visible === false) return false;
    try {
      const answer = await this.send(frame.frame, {
        kind: "frame.verify",
        token: frame.token,
        point,
        trusted,
        visible: frame.visible,
      });
      return typeof answer === "object" && answer !== null && "ok" in answer && answer.ok === true;
    } catch {
      return false;
    }
  }

  private show(frame: InlineFrame, view: InlineView): void {
    frame.view = view;
    try {
      frame.port?.postMessage({ kind: "view", view });
    } catch {
      frame.port = null;
    }
  }

  private async close(frame: InlineFrame): Promise<null> {
    this.frames.close(frame.token);
    await this.send(frame.frame, { kind: "frame.close", token: frame.token }).catch(
      () => undefined,
    );
    return null;
  }

  private notice(frame: InlineFrame, refusal: string): void {
    const site = "site" in frame.view ? frame.view.site : hostOf(frame.frameUrl);
    this.show(frame, { view: "notice", site, refusal });
  }

  private async perform(frame: InlineFrame, action: InlineAction): Promise<void> {
    switch (action.type) {
      case "choose": {
        const item = frame.items[action.index];
        if (!item) return;
        frame.item = item;
        if (frame.policy?.confirm_fill)
          return this.show(frame, { view: "confirm", item: menuItem(item) });
        if (item.downgrade) return this.showHttp(frame, item);
        return this.fillFrom(frame, item, false);
      }
      case "fill": {
        const item = frame.item;
        if (!item) return;
        if (item.downgrade) return this.showHttp(frame, item);
        return this.fillFrom(frame, item, false);
      }
      case "allow_http":
        if (frame.item) await this.fillFrom(frame, frame.item, true);
        return;
      case "use":
        return this.useGenerated(frame);
      case "regenerate": {
        const generated = frame.generated;
        if (!generated || frame.view.view !== "signup") return;
        const made = await this.generate(frame.frame, generated);
        if (!made) return;
        frame.generated = { ...generated, password: made.password };
        this.show(frame, { ...frame.view, password: made.password });
        return;
      }
      case "save":
      case "update":
      case "dismiss":
      case "never":
        return this.answerPrompt(frame, action);
      case "close":
        await this.close(frame);
    }
  }

  private showHttp(frame: InlineFrame, item: ItemSummary): void {
    this.show(frame, {
      view: "http",
      site: "site" in frame.view ? frame.view.site : hostOf(frame.frameUrl),
      item: menuItem(item),
    });
  }

  /** The facts of the menu's frame now: still the same document (and URL, without a documentId). */
  private async refreshed(frame: InlineFrame): Promise<TrustedFrame | null> {
    const tab = await this.api.tabs.get(frame.frame.tabId).catch(() => null);
    const picked = {
      tabId: frame.frame.tabId,
      frameId: frame.frame.frameId,
      tabUrl: tab?.url,
      ...(frame.frame.documentId === undefined
        ? { url: frame.frameUrl }
        : { documentId: frame.frame.documentId }),
    };
    const facts = await factsForFrame(picked, this.framesOf);
    return "error" in facts ? null : facts;
  }

  private async fillFrom(frame: InlineFrame, item: ItemSummary, allowHttp: boolean): Promise<void> {
    const facts = await this.refreshed(frame);
    if (!facts) {
      await this.close(frame);
      return;
    }
    // A login that asks first waits for the user in Lockra (as does a frame under the `ask` rule).
    if (item.protection !== "fill" || facts.facts.frame_chain.length > 0)
      this.show(frame, { view: "waiting", item: menuItem(item) });
    const outcome = await this.connection.request({
      op: "fill",
      item_id: item.item_id,
      ...facts.facts,
      trigger: frame.trigger,
      allow_http: allowHttp,
    });
    if (!outcome.ok) {
      if (outcome.error === "http_downgrade") return this.showHttp(frame, item);
      return this.notice(frame, outcome.error);
    }
    await this.deliver(facts, outcome.value);
    await this.close(frame);
  }

  /** A sign-up form's generated password: saved at once as a new login, then filled. */
  private async useGenerated(frame: InlineFrame): Promise<void> {
    const generated = frame.generated;
    if (!generated) return;
    const facts = await this.refreshed(frame);
    if (!facts) {
      await this.close(frame);
      return;
    }
    const saved = await this.connection.request({
      op: "save",
      ...facts.facts,
      title: "",
      username: generated.username,
      password: generated.password,
      group: null,
    });
    if (!saved.ok) return this.notice(frame, saved.error);
    await this.deliver(
      facts,
      {
        item_id: saved.value.item_id,
        username: generated.username,
        password: generated.password,
        fields: [],
        otp: null,
        submit: false,
      },
      true,
    );
    await this.close(frame);
  }

  private async answerPrompt(
    frame: InlineFrame,
    action: Extract<InlineAction, { type: "save" | "update" | "dismiss" | "never" }>,
  ): Promise<void> {
    const tabId = frame.frame.tabId;
    const submission = this.memory.submission(tabId);
    if (!submission) {
      await this.close(frame);
      return;
    }
    const facts = submission.frame.facts;
    let outcome: { ok: boolean; error?: string } = { ok: true };
    if (action.type === "save") {
      outcome = await this.connection.request({
        op: "save",
        ...facts,
        title: action.title.trim(),
        username: action.username,
        password: submission.password,
        group: action.group.trim() === "" ? null : action.group.trim(),
      });
    } else if (action.type === "update" && submission.item) {
      outcome = await this.connection.request({
        op: "update",
        item_id: submission.item.item_id,
        ...facts,
        password: submission.password,
      });
    } else if (action.type === "never") {
      outcome = await this.connection.request({ op: "site_rule", kind: "never_save", ...facts });
    }
    if (!outcome.ok) return this.notice(frame, outcome.error ?? "invalid");
    this.memory.dropSubmission(tabId);
    await this.close(frame);
  }

  // ---- the shortcut --------------------------------------------------------------------------

  /** The frame a fill goes to without a field asking: the probe in every frame (shortcut.ts). */
  private async pickFrame(tab: Tab): Promise<TrustedFrame | null> {
    const tabId = tab.id;
    if (tabId === undefined || !webUrl(tab.url)) return null;
    let results;
    try {
      results = await this.api.scripting.executeScript({
        target: { tabId, allFrames: true },
        func: probe,
      });
    } catch {
      return null;
    }
    const tree = await this.framesOf(tabId).catch(() => null);
    if (!tree) return null;
    const chosen = chooseFrame(results, tree);
    if (!chosen) return null;
    const facts = await factsForFrame(
      {
        tabId,
        frameId: chosen.frameId,
        tabUrl: tab.url,
        ...(chosen.documentId ? { documentId: chosen.documentId } : {}),
      },
      this.framesOf,
    );
    return "error" in facts ? null : facts;
  }

  /** The content script in a frame without site access (activeTab): put there on demand. */
  private async inject(frame: TrustedFrame): Promise<void> {
    try {
      await this.api.scripting.executeScript({
        target: { tabId: frame.tabId, frameIds: [frame.frameId] },
        files: ["content.js"],
      });
    } catch {
      // Already there, or out of reach: the fill's message says which.
    }
  }

  private async openPopup(): Promise<void> {
    try {
      await this.api.action?.openPopup?.();
    } catch {
      // Not from this context: the toolbar button shows the same.
    }
  }

  private async fillByShortcut(given?: Tab): Promise<void> {
    const tab = given?.id !== undefined ? given : await this.activeTab();
    if (tab?.id === undefined) return;
    this.connection.ensure();
    const frame = await this.pickFrame(tab);
    if (!frame) return;
    const outcome = await this.connection.request({
      op: "query",
      ...frame.facts,
      purpose: "fill",
      username: "",
      password: "",
    });
    if (!outcome.ok) {
      // Lockra locked, not paired, not running: the popup says so and offers what to do.
      await this.openPopup();
      return;
    }
    const answer = outcome.value;
    this.policy = answer.policy;
    const id = this.memory.cycle(
      frame.tabId,
      frame.frameId,
      answer.items.map((i) => i.item_id),
    );
    const item = answer.items.find((i) => i.item_id === id);
    if (!item) {
      await this.openPopup();
      return;
    }
    await this.inject(frame);
    if (answer.policy.confirm_fill || item.downgrade) {
      const view: InlineView = answer.policy.confirm_fill
        ? { view: "confirm", item: menuItem(item) }
        : { view: "http", site: answer.site, item: menuItem(item) };
      const opened = this.frames.open({
        kind: "menu",
        frame,
        frameUrl: frame.facts.frame_url,
        view,
        items: [item],
        policy: answer.policy,
        item,
        generated: null,
        trigger: "shortcut",
      });
      await this.send(frame, { kind: "menu.open", token: opened.token }).catch(() => undefined);
      return;
    }
    const filled = await this.connection.request({
      op: "fill",
      item_id: item.item_id,
      ...frame.facts,
      trigger: "shortcut",
      allow_http: false,
    });
    if (filled.ok) await this.deliver(frame, filled.value);
  }

  // ---- the popup and the options page --------------------------------------------------------

  private pageConnected(port: Port, page: "popup" | "options"): void {
    const context: PopupContext = { tab: null, frame: null, site: null };
    if (page === "popup") this.popups.set(port, context);
    else this.options.add(port);
    port.onMessage.addListener((raw) => void this.page(port, context, raw));
    port.onDisconnect.addListener(() => {
      this.popups.delete(port);
      this.options.delete(port);
    });
  }

  private async page(port: Port, context: PopupContext, raw: unknown): Promise<void> {
    const message = popupMessage(raw);
    if (!message) return;
    await this.pageRequest(port, context, message);
  }

  private async pageRequest(
    port: Port,
    context: PopupContext,
    message: PopupRequest,
  ): Promise<void> {
    const connection = this.connection;
    switch (message.kind) {
      case "hello": {
        connection.ensure();
        await this.readShortcut();
        context.tab = await this.activeTab();
        const snapshot = (
          await this.api.storage.session.get(SNAPSHOT).catch((): Record<string, unknown> => ({}))
        )[SNAPSHOT];
        if (isSnapshot(snapshot) && snapshot.url === context.tab?.url) context.site = snapshot.site;
        await this.post(port, context);
        if (connection.view.session === "open") await this.refreshSite(port, context);
        return;
      }
      case "pair":
        await connection.pair().catch(() => undefined);
        return this.post(port, context);
      case "pair.cancel":
        connection.cancelPairing();
        return;
      case "unlock":
        this.unlockAsked = await connection.unlock();
        return this.postAll();
      case "start":
        connection.startApp(message.show);
        return;
      case "retry":
        connection.ensure();
        return;
      case "search": {
        context.tab ??= await this.activeTab();
        const outcome = await connection.request({
          op: "search",
          text: message.text,
          top_url: context.tab?.url ?? "",
        });
        this.postEvent(
          port,
          outcome.ok
            ? { kind: "search", text: message.text, items: outcome.value.items, error: null }
            : { kind: "search", text: message.text, items: [], error: outcome.error },
        );
        return;
      }
      case "fill":
        return this.popupFill(port, context, message.itemId, message.allowHttp);
      case "copy": {
        const outcome = await connection.request({
          op: "copy",
          item_id: message.itemId,
          part: message.part,
        });
        this.postEvent(port, {
          kind: "result",
          action: "copy",
          itemId: message.itemId,
          outcome: outcome.ok ? "done" : outcome.error,
          part: message.part.type,
        });
        return;
      }
      case "access.changed":
        return this.syncAccess(true);
      case "unpair":
        await connection.forget();
        return this.postAll();
      case "extensions":
        if (TARGET === "chromium")
          await this.api.tabs
            .create({ url: `chrome://extensions/?id=${this.api.runtime.id}` })
            .catch(() => undefined);
        return;
    }
  }

  private async activeTab(): Promise<Tab | null> {
    return (
      (await this.api.tabs.query({ active: true, currentWindow: true }).catch(() => []))[0] ?? null
    );
  }

  /** The current tab's matches, for the frame a fill from the popup goes to. */
  private async refreshSite(port: Port, context: PopupContext): Promise<void> {
    const tab = context.tab;
    if (!tab?.url || !webUrl(tab.url)) {
      context.site = {
        site: tab?.url ? hostOf(tab.url) : "",
        items: [],
        fillable: false,
        neverFill: false,
        crossOrigin: false,
      };
      return this.post(port, context);
    }
    const frame = await this.pickFrame(tab);
    context.frame = frame;
    if (!frame) {
      context.site = {
        site: hostOf(tab.url),
        items: [],
        fillable: false,
        neverFill: false,
        crossOrigin: false,
      };
      return this.post(port, context);
    }
    const answer = await this.query(frame);
    if (!answer) return this.post(port, context);
    context.site = {
      site: answer.site,
      items: answer.items,
      fillable: true,
      neverFill: answer.never_fill,
      crossOrigin: answer.cross_origin,
    };
    await this.api.storage.session
      .set({ [SNAPSHOT]: { url: tab.url, site: context.site } })
      .catch(() => undefined);
    await this.post(port, context);
  }

  private async popupFill(
    port: Port,
    context: PopupContext,
    itemId: string,
    allowHttp: boolean,
  ): Promise<void> {
    const result = (outcome: string) =>
      this.postEvent(port, { kind: "result", action: "fill", itemId, outcome });
    const item = context.site?.items.find((i) => i.item_id === itemId);
    const picked = context.frame;
    if (!item || !picked || !context.tab) return result("not_found");
    if (item.downgrade && !allowHttp) return result("http_downgrade");
    const facts = await factsForFrame(
      {
        tabId: picked.tabId,
        frameId: picked.frameId,
        tabUrl: context.tab.url,
        ...(picked.documentId
          ? { documentId: picked.documentId }
          : { url: picked.facts.frame_url }),
      },
      this.framesOf,
    );
    if ("error" in facts) return result("navigated");
    await this.inject(facts);
    if (item.protection !== "fill") result("waiting");
    const outcome = await this.connection.request({
      op: "fill",
      item_id: itemId,
      ...facts.facts,
      trigger: "popup",
      allow_http: allowHttp,
    });
    if (!outcome.ok) return result(outcome.error);
    await this.deliver(facts, outcome.value);
    result("done");
  }

  private screen(view: ConnectionView): Screen {
    switch (view.link) {
      case "connecting":
        return "loading";
      case "no_host":
        return "no_host";
      case "not_running":
        return "not_running";
      case "incompatible":
        return view.incompatible === "app" ? "update_app" : "update_extension";
      case "ready":
        break;
    }
    if (view.app?.state === "no_vault") return "no_vault";
    if (view.app?.state !== "unlocked") return "locked";
    if (view.pairing.stage === "handshake" || view.pairing.stage === "waiting") return "pairing";
    if (!view.paired) return "unpaired";
    return view.session === "open" ? "ready" : "loading";
  }

  private async state(context: PopupContext | null): Promise<PopupState> {
    const view = this.connection.view;
    const record = await this.connection.pairingRecord().catch(() => null);
    const pairing =
      view.pairing.stage === "waiting"
        ? { code: view.pairing.code, expiresAt: view.pairing.expiresAt }
        : null;
    return {
      screen: this.screen(view),
      starting: view.starting,
      appVersion: view.app?.version ?? null,
      extensionVersion: this.deps.extensionVersion,
      pairing,
      pairingFailed: view.pairing.stage === "failed" ? view.pairing.reason : null,
      pageAccess: this.access,
      site: context?.site ?? null,
      policy: this.policy,
      paused: this.paused,
      shortcut: this.shortcut,
      unlockAsked: this.unlockAsked && view.app?.state === "locked",
      pairedAt: record?.paired_at ?? null,
    };
  }

  private async post(port: Port, context: PopupContext | null): Promise<void> {
    this.postEvent(port, { kind: "state", state: await this.state(context) });
  }

  private postEvent(port: Port, event: PopupEvent): void {
    try {
      port.postMessage(event);
    } catch {
      this.popups.delete(port);
      this.options.delete(port);
    }
  }

  private async postAll(): Promise<void> {
    await Promise.all([
      ...[...this.popups].map(([port, context]) => this.post(port, context)),
      ...[...this.options].map((port) => this.post(port, null)),
    ]);
  }

  private onView(view: ConnectionView): void {
    const opened = view.session === "open" && this.lastSession !== "open";
    this.lastSession = view.session;
    if (view.app?.state !== "locked") this.unlockAsked = false;
    void this.postAll();
    if (opened) void this.sessionOpened();
  }

  private async sessionOpened(): Promise<void> {
    const status = await this.connection.request({ op: "status" });
    if (status.ok) {
      this.policy = status.value.policy;
      this.paused = status.value.paused;
    }
    await Promise.all([
      ...[...this.popups].map(([port, context]) => this.refreshSite(port, context)),
      ...[...this.options].map((port) => this.post(port, null)),
    ]);
  }

  private async locked(): Promise<void> {
    this.memory.clear();
    await Promise.all(this.frames.all().map((frame) => this.close(frame)));
    for (const context of this.popups.values()) {
      context.site = null;
      context.frame = null;
    }
    await this.api.storage.session.remove(SNAPSHOT).catch(() => undefined);
  }

  private async syncAccess(refresh: boolean): Promise<void> {
    let access: boolean;
    try {
      access = await syncContentScript(this.api);
    } catch {
      return;
    }
    const changed = access !== this.access;
    this.access = access;
    if (changed && refresh) this.connection.refreshHello();
    if (changed) await this.postAll();
  }

  private async readShortcut(): Promise<void> {
    const commands = await this.api.commands.getAll().catch(() => []);
    this.shortcut = commands.find((c) => c.name === FILL_COMMAND)?.shortcut ?? "";
  }
}

function isSnapshot(value: unknown): value is { url: string; site: SiteView } {
  return (
    typeof value === "object" &&
    value !== null &&
    "url" in value &&
    typeof value.url === "string" &&
    "site" in value &&
    typeof value.site === "object" &&
    value.site !== null
  );
}
