// A fake WebExtension API for the background's tests: tabs with their frame trees, content scripts
// that answer per frame, ports for the popup and the inline frames, permissions and storage.
import type {
  BrowserApi,
  BrowserEvent,
  BrowserSetting,
  Command,
  FrameDetails,
  InjectionResult,
  InjectionTarget,
  LevelOfControl,
  MessageListener,
  MessageSender,
  Permissions,
  Port,
  RegisteredContentScript,
  StorageArea,
  Tab,
} from "../browser/api";
import type { ProbeResult } from "../background/shortcut";

// oxlint-disable-next-line typescript/no-explicit-any -- a test double's listeners take anything.
export class FakeEvent<F extends (...args: any[]) => unknown> implements BrowserEvent<F> {
  listeners: F[] = [];
  addListener(listener: F): void {
    this.listeners.push(listener);
  }
  removeListener(listener: F): void {
    this.listeners = this.listeners.filter((l) => l !== listener);
  }
  emit(...args: Parameters<F>): unknown[] {
    return this.listeners.map((listener) => listener(...args));
  }
}

/** A browser setting the fake holds: the user's value, and who holds it. */
export class FakeSetting implements BrowserSetting<boolean> {
  value = true;
  holder: "user" | "us" | "other" | "policy" = "user";
  async get(_: Record<string, never>) {
    const levelOfControl: LevelOfControl =
      this.holder === "us"
        ? "controlled_by_this_extension"
        : this.holder === "other"
          ? "controlled_by_other_extensions"
          : this.holder === "policy"
            ? "not_controllable"
            : "controllable_by_this_extension";
    return { value: this.value, levelOfControl };
  }
  async set({ value }: { value: boolean }) {
    // As the browser does: accepted, and overridden at once where another holds it.
    if (this.holder === "other" || this.holder === "policy") return;
    this.holder = "us";
    this.value = value;
  }
  async clear(_: Record<string, never>) {
    if (this.holder !== "us") return;
    this.holder = "user";
    this.value = true;
  }
}

export class MemoryArea implements StorageArea {
  data = new Map<string, unknown>();
  async get(keys: string | string[] | null): Promise<Record<string, unknown>> {
    const wanted = keys === null ? [...this.data.keys()] : Array.isArray(keys) ? keys : [keys];
    return Object.fromEntries(
      wanted.filter((k) => this.data.has(k)).map((k) => [k, structuredClone(this.data.get(k))]),
    );
  }
  async set(items: Record<string, unknown>): Promise<void> {
    for (const [key, value] of Object.entries(items)) this.data.set(key, structuredClone(value));
  }
  async remove(keys: string | string[]): Promise<void> {
    for (const key of Array.isArray(keys) ? keys : [keys]) this.data.delete(key);
  }
}

/** One end of a runtime port; `peer` is the other. */
export class FakePortEnd implements Port {
  onMessage = new FakeEvent<(message: unknown, port: Port) => void>();
  onDisconnect = new FakeEvent<(port: Port) => void>();
  peer!: FakePortEnd;
  closed = false;
  error: { message?: string } | null = null;
  received: unknown[] = [];
  constructor(
    readonly name: string,
    readonly sender?: MessageSender,
  ) {}
  /** What a page posts to the background (or the background to the page). */
  send(message: unknown): void {
    this.transmit(message);
  }
  postMessage(message: unknown): void {
    this.transmit(message);
  }
  private transmit(message: unknown): void {
    if (this.closed) throw new Error("Attempting to use a disconnected port object");
    const copy = structuredClone(message);
    queueMicrotask(() => {
      if (this.peer.closed) return;
      this.peer.received.push(copy);
      this.peer.onMessage.emit(copy, this.peer);
    });
  }
  disconnect(): void {
    if (this.closed) return;
    this.closed = true;
    queueMicrotask(() => {
      if (this.peer.closed) return;
      this.peer.closed = true;
      this.peer.onDisconnect.emit(this.peer);
    });
  }
}

export function portPair(name: string, sender?: MessageSender): [FakePortEnd, FakePortEnd] {
  const page = new FakePortEnd(name);
  const background = new FakePortEnd(name, sender);
  page.peer = background;
  background.peer = page;
  return [page, background];
}

export type ContentHandler = (message: unknown) => unknown;

export class FakeBrowser implements BrowserApi {
  /** How often the extension reloaded itself (`runtime.reload`). */
  reloads = 0;
  openTabs: Tab[] = [{ id: 1, url: "https://www.example.com/login", active: true, windowId: 1 }];
  frames = new Map<number, FrameDetails[]>([
    [
      1,
      [
        {
          frameId: 0,
          parentFrameId: -1,
          url: "https://www.example.com/login",
          documentId: "doc-0",
        },
      ],
    ],
  ]);
  /** What each frame's content script answers: `${tabId}:${frameId}`. */
  content = new Map<string, ContentHandler>();
  /** Messages sent to content scripts, with their target. */
  sent: { tabId: number; frameId?: number; documentId?: string; message: unknown }[] = [];
  probes = new Map<number, ProbeResult>();
  injected: { target: InjectionTarget; files?: string[] }[] = [];
  registered: RegisteredContentScript[] = [];
  granted = false;
  /** The optional `privacy` permission, and what the user answers when asked for it. */
  privacyGranted = false;
  privacyAnswer = true;
  services = {
    passwordSavingEnabled: new FakeSetting(),
    autofillAddressEnabled: new FakeSetting(),
    autofillCreditCardEnabled: new FakeSetting(),
  };
  created: string[] = [];
  popupOpened = 0;

  runtime = {
    id: "testextensionidtestextensionidte",
    lastError: undefined as { message?: string } | undefined,
    getURL: (path: string) => `chrome-extension://testextensionidtestextensionidte/${path}`,
    getManifest: () => ({ version: "0.10.0" }),
    connectNative: (_application: string): Port => {
      throw new Error("the tests connect through FakeApp");
    },
    connect: (): Port => {
      throw new Error("extension pages connect in the tests through connectPort");
    },
    sendMessage: async (): Promise<unknown> => undefined,
    openOptionsPage: async () => undefined,
    reload: (): void => {
      this.reloads += 1;
    },
    onMessage: new FakeEvent<MessageListener>(),
    onConnect: new FakeEvent<(port: Port) => void>(),
    onStartup: new FakeEvent<() => void>(),
    onInstalled: new FakeEvent<
      (details: {
        reason: "install" | "update" | "browser_update" | "chrome_update" | "shared_module_update";
      }) => void
    >(),
  };

  tabs = {
    query: async () => this.openTabs.filter((tab) => tab.active),
    get: async (tabId: number) => {
      const tab = this.openTabs.find((t) => t.id === tabId);
      if (!tab) throw new Error(`no tab ${tabId}`);
      return tab;
    },
    sendMessage: async (
      tabId: number,
      message: unknown,
      options?: { frameId?: number; documentId?: string },
    ) => {
      this.sent.push({ tabId, ...options, message });
      const handler = this.content.get(`${tabId}:${options?.frameId ?? 0}`);
      if (!handler)
        throw new Error("Could not establish connection. Receiving end does not exist.");
      return handler(message);
    },
    create: async (properties: { url: string }) => {
      this.created.push(properties.url);
      return { id: 99, url: properties.url };
    },
    onRemoved: new FakeEvent<(tabId: number) => void>(),
  };

  scripting = {
    executeScript: async <T>(injection: {
      target: InjectionTarget;
      func?: () => T;
      files?: string[];
    }): Promise<InjectionResult<T>[]> => {
      if (injection.files) {
        this.injected.push({ target: injection.target, files: injection.files });
        return [];
      }
      const frames = this.frames.get(injection.target.tabId) ?? [];
      return frames
        .filter((f) => this.probes.has(f.frameId))
        .map((f) => ({
          frameId: f.frameId,
          documentId: f.documentId,
          result: this.probes.get(f.frameId) as T,
        }));
    },
    registerContentScripts: async (scripts: RegisteredContentScript[]) => {
      this.registered.push(...scripts);
    },
    unregisterContentScripts: async (filter?: { ids?: string[] }) => {
      this.registered = this.registered.filter((s) => !filter?.ids?.includes(s.id));
    },
    getRegisteredContentScripts: async (filter?: { ids?: string[] }) =>
      this.registered.filter((s) => !filter?.ids || filter.ids.includes(s.id)),
  };

  webNavigation = {
    getAllFrames: async ({ tabId }: { tabId: number }) => this.frames.get(tabId) ?? null,
  };

  permissions = {
    contains: async (asked: Permissions) =>
      asked.permissions?.includes("privacy") ? this.privacyGranted : this.granted,
    request: async (asked: Permissions) => {
      if (asked.permissions?.includes("privacy")) {
        this.privacyGranted = this.privacyAnswer;
        return this.privacyAnswer;
      }
      this.granted = true;
      return true;
    },
    remove: async (asked: Permissions) => {
      if (asked.permissions?.includes("privacy")) this.privacyGranted = false;
      else this.granted = false;
      return true;
    },
    onAdded: new FakeEvent<(permissions: Permissions) => void>(),
    onRemoved: new FakeEvent<(permissions: Permissions) => void>(),
  };

  storage = { local: new MemoryArea(), session: new MemoryArea() };

  /** As the browser does: there once the permission is. */
  get privacy() {
    return this.privacyGranted ? { services: this.services } : undefined;
  }

  commands = {
    getAll: async (): Promise<Command[]> => [{ name: "fill-login", shortcut: "Ctrl+Shift+L" }],
    onCommand: new FakeEvent<(command: string, tab?: Tab) => void>(),
  };

  i18n = { getUILanguage: () => "zh-CN" };

  action = {
    openPopup: async () => {
      this.popupOpened++;
    },
  };

  /** A content script's message, as the browser delivers it to the background. */
  fromContent(message: unknown, sender: Partial<MessageSender> = {}): Promise<unknown> {
    const tab = this.openTabs[0];
    const frameId = sender.frameId ?? 0;
    const frame = this.frames.get(tab?.id ?? 1)?.find((f) => f.frameId === frameId);
    const full: MessageSender = {
      id: this.runtime.id,
      tab: { ...tab },
      frameId,
      url: frame?.url,
      origin: frame?.url ? new URL(frame.url).origin : undefined,
      documentId: frame?.documentId,
      ...sender,
    };
    return new Promise((resolve) => {
      const answers = this.runtime.onMessage.emit(message, full, resolve);
      if (!answers.includes(true)) resolve(undefined);
    });
  }

  /** An extension page (the popup, an inline frame) connects; the page's end. */
  connectPort(name: string, sender?: MessageSender): FakePortEnd {
    const [page, background] = portPair(name, sender);
    this.runtime.onConnect.emit(background);
    return page;
  }
}
