// The thin adapters between the browser's APIs and the extension's modules: the native port, the
// hello's facts, the content script's runtime, the inline frame's port, the pages' client, the keys,
// what the page shows, the colour scheme.
import {
  FakeBrowser,
  FakeEvent,
  type FakePortEnd,
  MemoryArea,
  portPair,
} from "./test/fake-browser";
import { nativePort, HOST_NAME } from "./background/native";
import { brandsOf, browserName, clientInfo } from "./background/client-info";
import { contentRuntime } from "./content/runtime";
import { inlinePort } from "./inline/port";
import { browserClient } from "./popup/client";
import { KeyStore, type KeyVault } from "./background/keys";
import { shown } from "./content/visibility";
import { followScheme } from "./shared/scheme";
import { FieldIcon } from "./content/field-icon";
import { browserApi } from "./browser/api";
import type { KeyPair } from "./noise/crypto";
import type { Port } from "./browser/api";

describe("the native port", () => {
  it("connects to the host, passes messages, and says why it closed", () => {
    const browser = new FakeBrowser();
    const [page, background] = portPair("native");
    const connect = vi.fn((_name: string): Port => background);
    browser.runtime.connectNative = connect;
    const port = nativePort(browser);
    expect(connect).toHaveBeenCalledWith(HOST_NAME);
    const got: unknown[] = [];
    port.onMessage((raw) => got.push(raw));
    const closed = vi.fn();
    port.onDisconnect(closed);
    port.post({ v: 1 });
    background.onMessage.emit({ v: 2 }, background);
    expect(got).toEqual([{ v: 2 }]);
    browser.runtime.lastError = { message: "Specified native messaging host not found." };
    background.onDisconnect.emit(background);
    expect(closed).toHaveBeenCalledWith("Specified native messaging host not found.");
    browser.runtime.lastError = undefined;
    background.error = { message: "No such native application" };
    background.onDisconnect.emit(background);
    expect(closed).toHaveBeenLastCalledWith("No such native application");
    port.disconnect();
    expect(background.closed).toBe(true);
    expect(page).toBeDefined();
  });
});

describe("the hello's facts", () => {
  it("name the browser from its brands", () => {
    expect(browserName("firefox", new Set())).toBe("firefox");
    expect(browserName("chromium", new Set(["Microsoft Edge", "Chromium"]))).toBe("edge");
    expect(browserName("chromium", new Set(["Brave"]))).toBe("brave");
    expect(browserName("chromium", new Set(["Google Chrome"]))).toBe("chrome");
    expect(browserName("chromium", new Set())).toBe("chromium");
    expect(
      brandsOf({ userAgentData: { brands: [{ brand: "Google Chrome" }, { brand: 3 }, null] } }),
    ).toEqual(new Set(["Google Chrome"]));
    expect(brandsOf({ userAgentData: null })).toEqual(new Set());
    expect(brandsOf({})).toEqual(new Set());
  });

  it("carry the extension's id, version and site access", async () => {
    const browser = new FakeBrowser();
    browser.granted = true;
    expect(await clientInfo(browser, new Set(["Google Chrome"]))).toEqual({
      browser: "chrome",
      label: "",
      extension: browser.runtime.id,
      version: "0.10.0",
      page_access: true,
    });
    browser.permissions.contains = async () => {
      throw new Error("no");
    };
    expect((await clientInfo(browser, new Set())).page_access).toBe(false);
  });
});

describe("the content script's runtime", () => {
  it("sends to the background and answers only the extension's own messages", async () => {
    const browser = new FakeBrowser();
    const sendMessage = vi.fn(async () => "answer");
    browser.runtime.sendMessage = sendMessage;
    const runtime = contentRuntime(browser);
    expect(await runtime.send({ kind: "content.otp" })).toBe("answer");
    expect(runtime.url("inline.html")).toBe(browser.runtime.getURL("inline.html"));
    runtime.listen((message) => (message === "ours" ? Promise.resolve("done") : undefined));
    const respond = vi.fn();
    const listener = browser.runtime.onMessage.listeners[0];
    expect(listener?.("ours", { id: "someone else" }, respond)).toBeUndefined();
    expect(listener?.("theirs", { id: browser.runtime.id }, respond)).toBeUndefined();
    expect(listener?.("ours", { id: browser.runtime.id }, respond)).toBe(true);
    await vi.waitFor(() => expect(respond).toHaveBeenCalledWith("done"));
    runtime.listen(() => Promise.reject(new Error("x")));
    browser.runtime.onMessage.listeners[1]?.("x", { id: browser.runtime.id }, respond);
    await vi.waitFor(() => expect(respond).toHaveBeenLastCalledWith(undefined));
  });
});

describe("the inline frame's port", () => {
  it("passes the background's views and closes, nothing else", () => {
    const browser = new FakeBrowser();
    let made: FakePortEnd | null = null;
    browser.runtime.connect = (info?: { name?: string }) => {
      const [page] = portPair(info?.name ?? "");
      made = page;
      return page;
    };
    const port = inlinePort(browser);
    const got: unknown[] = [];
    port.onMessage((message) => got.push(message));
    const gone = vi.fn();
    port.onDisconnect(gone);
    const end = made as unknown as FakePortEnd;
    expect(end.name).toBe("lockra-inline");
    end.onMessage.emit({ kind: "view", view: { view: "notice", site: "", refusal: "x" } }, end);
    end.onMessage.emit({ kind: "other" }, end);
    end.onMessage.emit("text", end);
    expect(got).toEqual([{ kind: "view", view: { view: "notice", site: "", refusal: "x" } }]);
    end.onDisconnect.emit(end);
    expect(gone).toHaveBeenCalled();
    port.post({ kind: "hello", token: "a".repeat(32) });
  });
});

describe("the pages' client", () => {
  it("connects its port, takes the background's events, and asks for site access", async () => {
    const browser = new FakeBrowser();
    let made: FakePortEnd | null = null;
    browser.runtime.connect = (info?: { name?: string }) => {
      const [page] = portPair(info?.name ?? "");
      made = page;
      return page;
    };
    const close = vi.fn();
    const win = {
      close,
      navigator: { clipboard: { writeText: vi.fn(async () => undefined) } },
    } as unknown as Window;
    const client = browserClient(browser, "lockra-popup", win);
    const events: unknown[] = [];
    const link = client.connect((event) => events.push(event));
    const end = made as unknown as FakePortEnd;
    end.onMessage.emit({ kind: "state", state: {} }, end);
    end.onMessage.emit({ kind: "nope" }, end);
    expect(events).toEqual([{ kind: "state", state: {} }]);
    link.send({ kind: "hello" });
    link.close();
    expect(end.closed).toBe(true);
    // A closed port throws: the client keeps quiet.
    link.send({ kind: "hello" });
    expect(await client.requestAccess()).toBe(true);
    expect(await client.removeAccess()).toBe(true);
    browser.permissions.request = async () => {
      throw new Error("not from a click");
    };
    expect(await client.requestAccess()).toBe(false);
    client.openOptions();
    client.close();
    expect(close).toHaveBeenCalled();
    await client.copyText("https://firlab.app/lockra/");
    expect(await client.shortcuts()).toEqual({
      fill: "Ctrl+Shift+L",
      menu: "Alt+Shift+L",
      popup: "",
    });
    browser.commands.getAll = async () => {
      throw new Error("x");
    };
    expect(await client.shortcuts()).toEqual({ fill: "", menu: "", popup: "" });
    expect(client.target).toBe("chromium");
  });
});

describe("the keys", () => {
  it("make the static key once, keep it, and keep a pairing that reads", async () => {
    const saved: KeyPair[] = [];
    const vault: KeyVault = {
      load: async () => saved[0] ?? null,
      save: async (pair) => {
        saved.push(pair);
      },
    };
    const local = new MemoryArea();
    const keys = new KeyStore(vault, local);
    const [a, b] = await Promise.all([keys.staticPair(), keys.staticPair()]);
    expect(a).toBe(b);
    expect(saved).toHaveLength(1);
    expect(a.privateKey.extractable).toBe(false);
    expect(await new KeyStore(vault, local).staticPair()).toBe(saved[0]);
    expect(await keys.pairing()).toBeNull();
    const record = {
      id: "5c0fe1a2-0b3c-4d5e-8f60-000000000009",
      app_public: btoa("k".repeat(32)),
      paired_at: 1,
    };
    await keys.setPairing(record);
    expect(await keys.pairing()).toEqual(record);
    await local.set({ pairing: { ...record, app_public: "short" } });
    expect(await keys.pairing()).toBeNull();
    await keys.setPairing(null);
    expect(local.data.has("pairing")).toBe(false);
    const failing = new KeyStore(
      {
        load: async () => {
          throw new Error("no IndexedDB");
        },
        save: async () => undefined,
      },
      local,
    );
    await expect(failing.staticPair()).rejects.toThrow("no IndexedDB");
  });
});

describe("what the page shows", () => {
  it("is what has a size, is rendered and is on the page", () => {
    document.body.innerHTML = `<input id="a"><input id="b">`;
    const a = document.getElementById("a") as HTMLInputElement;
    const rect = (r: Partial<DOMRect>) => () =>
      ({
        x: 0,
        y: 0,
        width: 100,
        height: 20,
        top: 0,
        left: 0,
        right: 100,
        bottom: 20,
        toJSON: () => "",
        ...r,
      }) as DOMRect;
    a.getBoundingClientRect = rect({});
    expect(shown(a)).toBe(true);
    a.getBoundingClientRect = rect({ width: 0 });
    expect(shown(a)).toBe(false);
    a.getBoundingClientRect = rect({ left: -1000, right: -900 });
    expect(shown(a)).toBe(false);
    a.getBoundingClientRect = rect({});
    a.checkVisibility = () => false;
    expect(shown(a)).toBe(false);
    expect(shown(document.createElement("input"))).toBe(false);
    expect(shown(document.createElementNS("http://www.w3.org/2000/svg", "svg"))).toBe(false);
  });
});

describe("the colour scheme", () => {
  it("picks 澄空 or 澄空 · 夜 with the browser's, and follows it", () => {
    const changes = new FakeEvent<() => void>();
    let dark = true;
    const media = {
      get matches() {
        return dark;
      },
      addEventListener: (_: string, l: () => void) => changes.addListener(l),
    };
    const win = { matchMedia: () => media } as unknown as Window;
    followScheme(document, win);
    expect(document.documentElement.dataset.theme).toBe("glass_night");
    dark = false;
    changes.emit();
    expect(document.documentElement.dataset.theme).toBe("glass");
    followScheme(document, {} as Window);
    expect(document.documentElement.dataset.theme).toBe("glass");
  });
});

describe("the field's mark", () => {
  it("sits at the field's right end and opens the menu on the user's click only", () => {
    document.body.innerHTML = `<input id="f">`;
    const field = document.getElementById("f") as HTMLInputElement;
    const opened = vi.fn();
    const icon = new FieldIcon(document, "chrome-extension://x/icons/icon-32.png", opened);
    icon.show(field);
    expect(icon.host.isConnected).toBe(true);
    expect(icon.host.getAttribute("style")).toContain("position: fixed !important");
    icon.host.dispatchEvent(new MouseEvent("click"));
    expect(opened).not.toHaveBeenCalled();
    const down = new MouseEvent("mousedown", { cancelable: true });
    icon.host.dispatchEvent(down);
    expect(down.defaultPrevented).toBe(true);
    icon.hide();
    expect(icon.host.isConnected).toBe(false);
    icon.layout();
  });
});

describe("the browser's API", () => {
  it("is `browser` where there is one, else `chrome`", () => {
    const original = { browser: globalThis.browser, chrome: globalThis.chrome };
    const fake = new FakeBrowser();
    globalThis.browser = undefined;
    globalThis.chrome = fake;
    expect(browserApi()).toBe(fake);
    const firefox = new FakeBrowser();
    globalThis.browser = firefox;
    expect(browserApi()).toBe(firefox);
    globalThis.browser = undefined;
    globalThis.chrome = undefined;
    expect(() => browserApi()).toThrow("no WebExtension API here");
    globalThis.browser = original.browser;
    globalThis.chrome = original.chrome;
  });
});

describe("the system clock", () => {
  it("tells the time and runs timers", async () => {
    const { systemClock } = await import("./background/clock");
    expect(Math.abs(systemClock.now() - Date.now())).toBeLessThan(1_000);
    const ran = vi.fn();
    const id = systemClock.setTimeout(ran, 0);
    systemClock.clearTimeout(id);
    await new Promise((resolve) => systemClock.setTimeout(() => resolve(null), 0));
    expect(ran).not.toHaveBeenCalled();
  });
});
