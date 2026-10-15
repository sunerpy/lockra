// A typed wrapper over the WebExtension APIs the extension uses, the same in Chromium (`chrome.*`,
// MV3, promises) and Firefox (`browser.*`): only these members, typed as both browsers document
// them, so a call that one of them lacks shows here. Written by hand rather than taken from
// @types/chrome: the surface is small, and the types say exactly what the extension relies on.

export interface BrowserEvent<F extends (...args: never[]) => unknown> {
  addListener(listener: F): void;
  removeListener(listener: F): void;
}

export interface Tab {
  id?: number;
  url?: string;
  windowId?: number;
  active?: boolean;
}

export interface MessageSender {
  tab?: Tab;
  frameId?: number;
  id?: string;
  url?: string;
  /** Chrome 80, Firefox 126. */
  origin?: string;
  /** Chrome 106, Firefox 153. */
  documentId?: string;
}

export interface Port {
  name: string;
  sender?: MessageSender;
  /** Firefox: why a native port closed. */
  error?: { message?: string } | null;
  postMessage(message: unknown): void;
  disconnect(): void;
  onMessage: BrowserEvent<(message: unknown, port: Port) => void>;
  onDisconnect: BrowserEvent<(port: Port) => void>;
}

export interface FrameDetails {
  frameId: number;
  parentFrameId: number;
  url: string;
  /** Chrome 106, Firefox 153. */
  documentId?: string;
  errorOccurred?: boolean;
}

export interface InjectionResult<T> {
  frameId: number;
  /** Chrome 106. */
  documentId?: string;
  result?: T;
}

export interface InjectionTarget {
  tabId: number;
  frameIds?: number[];
  allFrames?: boolean;
}

export interface RegisteredContentScript {
  id: string;
  matches?: string[];
  js?: string[];
  allFrames?: boolean;
  runAt?: "document_start" | "document_end" | "document_idle";
  persistAcrossSessions?: boolean;
}

export interface Permissions {
  origins?: string[];
  permissions?: string[];
}

export interface StorageArea {
  get(keys: string | string[] | null): Promise<Record<string, unknown>>;
  set(items: Record<string, unknown>): Promise<void>;
  remove(keys: string | string[]): Promise<void>;
}

export interface Command {
  name?: string;
  description?: string;
  shortcut?: string;
}

/** Who has a browser setting (`privacy`): the browser's own value can be changed by this extension
 *  (`controllable_by_this_extension`), already is (`controlled_by_this_extension`), or cannot be
 *  (another extension's or a policy's). */
export type LevelOfControl =
  | "not_controllable"
  | "controlled_by_other_extensions"
  | "controllable_by_this_extension"
  | "controlled_by_this_extension";

/** A browser setting the extension may hold (`ChromeSetting` in Chromium, `BrowserSetting` in
 *  Firefox): set takes it over, clear gives the user's value back. */
export interface BrowserSetting<T> {
  get(details: Record<string, never>): Promise<{ value: T; levelOfControl: LevelOfControl }>;
  set(details: { value: T }): Promise<unknown>;
  clear(details: Record<string, never>): Promise<unknown>;
}

export interface InstalledDetails {
  reason: "install" | "update" | "browser_update" | "chrome_update" | "shared_module_update";
}

/** The `sendResponse` style works in both browsers: return true to answer later. */
export type MessageListener = (
  message: unknown,
  sender: MessageSender,
  sendResponse: (response?: unknown) => void,
) => boolean | undefined;

export interface BrowserApi {
  runtime: {
    id: string;
    lastError?: { message?: string } | undefined;
    getURL(path: string): string;
    getManifest(): { version: string; name?: string };
    connectNative(application: string): Port;
    connect(info?: { name?: string }): Port;
    sendMessage(message: unknown): Promise<unknown>;
    openOptionsPage(): Promise<void>;
    onMessage: BrowserEvent<MessageListener>;
    onConnect: BrowserEvent<(port: Port) => void>;
    onStartup: BrowserEvent<() => void>;
    onInstalled: BrowserEvent<(details: InstalledDetails) => void>;
  };
  tabs: {
    query(query: {
      active?: boolean;
      currentWindow?: boolean;
      lastFocusedWindow?: boolean;
    }): Promise<Tab[]>;
    get(tabId: number): Promise<Tab>;
    sendMessage(
      tabId: number,
      message: unknown,
      options?: { frameId?: number; documentId?: string },
    ): Promise<unknown>;
    create(properties: { url: string }): Promise<Tab>;
    onRemoved: BrowserEvent<(tabId: number) => void>;
  };
  scripting: {
    executeScript<T>(injection: {
      target: InjectionTarget;
      func?: () => T;
      files?: string[];
    }): Promise<InjectionResult<T>[]>;
    registerContentScripts(scripts: RegisteredContentScript[]): Promise<void>;
    unregisterContentScripts(filter?: { ids?: string[] }): Promise<void>;
    getRegisteredContentScripts(filter?: { ids?: string[] }): Promise<RegisteredContentScript[]>;
  };
  webNavigation: {
    getAllFrames(details: { tabId: number }): Promise<FrameDetails[] | null>;
  };
  permissions: {
    contains(permissions: Permissions): Promise<boolean>;
    request(permissions: Permissions): Promise<boolean>;
    remove(permissions: Permissions): Promise<boolean>;
    onAdded: BrowserEvent<(permissions: Permissions) => void>;
    onRemoved: BrowserEvent<(permissions: Permissions) => void>;
  };
  storage: {
    local: StorageArea;
    session: StorageArea;
  };
  commands: {
    getAll(): Promise<Command[]>;
    onCommand: BrowserEvent<(command: string, tab?: Tab) => void>;
  };
  i18n: {
    getUILanguage(): string;
  };
  /** The toolbar button: `openPopup` from a user action (Chrome 127+, Firefox). */
  action?: {
    openPopup?(): Promise<void>;
  };
  /** Present once the optional `privacy` permission is granted. The address and card autofill are
   *  Chromium's (70+); Firefox has the password saving alone. */
  privacy?: {
    services: {
      passwordSavingEnabled: BrowserSetting<boolean>;
      autofillAddressEnabled?: BrowserSetting<boolean>;
      autofillCreditCardEnabled?: BrowserSetting<boolean>;
    };
  };
}

/** `browser` in Firefox, `chrome` in Chromium. */
export function browserApi(): BrowserApi {
  const api = globalThis.browser ?? globalThis.chrome;
  if (!api) throw new Error("no WebExtension API here");
  return api;
}

/** Which browser family the build is for (set at build time). */
export const TARGET: "chromium" | "firefox" = LOCKRA_TARGET;
