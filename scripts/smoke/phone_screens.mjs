#!/usr/bin/env node
// Phone screenshots for the documentation site and the READMEs: the phone app's dev build (its
// in-memory core, MockBackend) in a Chrome driven over the DevTools protocol, in a browser context
// of its own that is disposed at the end. A device cannot be used: the app blocks screenshots
// (FLAG_SECURE). The mock module is patched as it loads, so the phone shows the accounts the
// desktop screenshots show (scripts/smoke/site_screens.py writes them to the state file), with
// real RFC 6238 codes and the clock frozen at the desktop shot's moment: the same codes on both.
// Run by scripts/capture-site-screens.sh. Every wait is a condition with a deadline.
//
// Usage: phone_screens.mjs --cdp <host:port> --url <phone dev server> --lang en|zh
//                          --state <state.json> --out <dir> [--passwords]
// Writes <out>/<scheme>-<page>.png (scheme light|dark, page codes|add|sync), 1080 x 1920; with
// --passwords the dev build offers password management (its `?passwords`, the desktop's logins
// from the state file) and the logins and a login's page come too (logins|login).
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

const { values: args } = parseArgs({
  options: {
    cdp: { type: "string" },
    url: { type: "string" },
    lang: { type: "string" },
    state: { type: "string" },
    out: { type: "string" },
    passwords: { type: "boolean" },
  },
});
for (const name of ["cdp", "url", "lang", "state", "out"]) {
  if (!args[name]) throw new Error(`phone-screens: --${name} is required`);
}
if (typeof WebSocket !== "function")
  throw new Error("phone-screens: needs Node 22 or later (WebSocket)");

// 405 x 720 CSS px at 8/3: 1080 x 1920, a 9:16 phone.
const WIDTH = 405;
const HEIGHT = 720;
const SCALE = 8 / 3;
const LOCALE = { en: "en-US", zh: "zh-CN" }[args.lang];
if (!LOCALE) throw new Error(`phone-screens: no locale for ${args.lang}`);
const state = JSON.parse(readFileSync(args.state, "utf8"));
mkdirSync(args.out, { recursive: true });

// Appended to the mock module: its sample vault becomes the desktop's accounts, and its stand-in
// codes become real ones (SHA-1 HOTP over the Base32 secret; every account here is SHA-1).
const PATCH = `
;(() => {
  const ACCOUNTS = ${JSON.stringify(state.accounts)};
  sampleEntries = () => ACCOUNTS.map((a) => mockEntry(a.issuer, a.account, a.options));
  // The desktop's logins too, in the shot's language, each code linked by its account's issuer.
  const LOGINS = ${JSON.stringify(state.logins ?? [])};
  if (LOGINS.length > 0)
    sampleItems = (entries = []) =>
      LOGINS.map((l) => ({
        view: mockItemView(l.title, {
          ...l.view,
          has_password: true,
          totp_entry: (l.code && entries.find((e) => e.view.issuer === l.code)?.view.id) || null,
        }),
        password: l.password,
      }));
  const sha1 = (bytes) => {
    const size = ((bytes.length + 9 + 63) >> 6) << 6;
    const m = new Uint8Array(size);
    m.set(bytes);
    m[bytes.length] = 0x80;
    const view = new DataView(m.buffer);
    view.setUint32(size - 4, (bytes.length * 8) >>> 0);
    view.setUint32(size - 8, Math.floor((bytes.length * 8) / 2 ** 32));
    const h = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
    const w = new Uint32Array(80);
    for (let off = 0; off < size; off += 64) {
      for (let i = 0; i < 16; i += 1) w[i] = view.getUint32(off + i * 4);
      for (let i = 16; i < 80; i += 1) {
        const x = w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16];
        w[i] = (x << 1) | (x >>> 31);
      }
      let [a, b, c, d, e] = h;
      for (let i = 0; i < 80; i += 1) {
        const [f, k] =
          i < 20 ? [(b & c) | (~b & d), 0x5a827999]
          : i < 40 ? [b ^ c ^ d, 0x6ed9eba1]
          : i < 60 ? [(b & c) | (b & d) | (c & d), 0x8f1bbcdc]
          : [b ^ c ^ d, 0xca62c1d6];
        const t = (((a << 5) | (a >>> 27)) + f + e + k + w[i]) >>> 0;
        e = d;
        d = c;
        c = ((b << 30) | (b >>> 2)) >>> 0;
        b = a;
        a = t;
      }
      h[0] = (h[0] + a) >>> 0;
      h[1] = (h[1] + b) >>> 0;
      h[2] = (h[2] + c) >>> 0;
      h[3] = (h[3] + d) >>> 0;
      h[4] = (h[4] + e) >>> 0;
    }
    const out = new Uint8Array(20);
    h.forEach((x, i) => new DataView(out.buffer).setUint32(i * 4, x));
    return out;
  };
  const hmac = (key, message) => {
    const k = new Uint8Array(64);
    k.set(key.length > 64 ? sha1(key) : key);
    const inner = new Uint8Array(64 + message.length);
    const outer = new Uint8Array(64 + 20);
    for (let i = 0; i < 64; i += 1) {
      inner[i] = k[i] ^ 0x36;
      outer[i] = k[i] ^ 0x5c;
    }
    inner.set(message, 64);
    outer.set(sha1(inner), 64);
    return sha1(outer);
  };
  const base32 = (text) => {
    const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    const out = [];
    let value = 0;
    let bits = 0;
    for (const ch of text.toUpperCase()) {
      const v = alphabet.indexOf(ch);
      if (v < 0) continue;
      value = ((value << 5) | v) & 0xfff;
      bits += 5;
      if (bits >= 8) {
        out.push((value >>> (bits - 8)) & 0xff);
        bits -= 8;
      }
    }
    return new Uint8Array(out);
  };
  fakeCode = (secret, counter, digits) => {
    const message = new Uint8Array(8);
    new DataView(message.buffer).setUint32(0, Math.floor(counter / 2 ** 32));
    new DataView(message.buffer).setUint32(4, counter >>> 0);
    const mac = hmac(base32(secret), message);
    const o = mac[19] & 0xf;
    const bin = ((mac[o] & 0x7f) << 24) | (mac[o + 1] << 16) | (mac[o + 2] << 8) | mac[o + 3];
    return String(bin % 10 ** digits).padStart(digits, "0");
  };
})();
`;

// Before the page's own scripts: the system language, no scrollbars (a phone shows none at rest),
// and Date frozen at the given moment.
const startScript = (at) => `
Object.defineProperty(navigator, 'language', { get: () => '${LOCALE}' });
Object.defineProperty(navigator, 'languages', { get: () => ['${LOCALE}'] });
(() => {
  const Real = Date;
  const at = ${Number(at)};
  function Frozen(...rest) {
    if (!new.target) return new Real(at).toString();
    return rest.length ? new Real(...rest) : new Real(at);
  }
  Frozen.prototype = Real.prototype;
  Frozen.now = () => at;
  Frozen.parse = Real.parse;
  Frozen.UTC = Real.UTC;
  window.Date = Frozen;
})();
document.addEventListener('DOMContentLoaded', () => {
  const style = document.createElement('style');
  style.textContent = '*{scrollbar-width:none!important}*::-webkit-scrollbar{display:none!important}';
  document.head.append(style);
});
`;

const version = await (await fetch(`http://${args.cdp}/json/version`)).json();
// The browser names its own address; through a tunnel, the endpoint to use is the one given.
const ws = new WebSocket(version.webSocketDebuggerUrl.replace(/^ws:\/\/[^/]+/, `ws://${args.cdp}`));
await new Promise((resolve, reject) => {
  ws.addEventListener("open", resolve, { once: true });
  ws.addEventListener("error", reject, { once: true });
});
let id = 0;
const pending = new Map();
const listeners = new Set();
ws.addEventListener("message", (event) => {
  const msg = JSON.parse(event.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    if (msg.error) reject(new Error(JSON.stringify(msg.error)));
    else resolve(msg.result);
  } else if (msg.method) for (const listener of listeners) listener(msg);
});
const send = (method, params = {}, sessionId) =>
  new Promise((resolve, reject) => {
    id += 1;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
const deadline = (promise, ms, what) =>
  Promise.race([
    promise,
    new Promise((_, reject) => setTimeout(() => reject(new Error(`${what} timed out`)), ms)),
  ]);

/** One fresh page per scheme: unlock, then each page in turn. */
async function capture(scheme) {
  const { browserContextId } = await send("Target.createBrowserContext", { disposeOnDetach: true });
  let targetId;
  try {
    ({ targetId } = await send("Target.createTarget", { url: "about:blank", browserContextId }));
    const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
    const s = (method, params) => send(method, params, sessionId);
    await s("Page.enable");
    await s("Runtime.enable");
    await s("Emulation.setDeviceMetricsOverride", {
      width: WIDTH,
      height: HEIGHT,
      deviceScaleFactor: SCALE,
      mobile: true,
    });
    await s("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
    await s("Emulation.setLocaleOverride", { locale: LOCALE });
    await s("Emulation.setEmulatedMedia", {
      features: [
        { name: "prefers-color-scheme", value: scheme },
        { name: "prefers-reduced-motion", value: "reduce" },
      ],
    });
    await s("Network.enable");
    await s("Network.setUserAgentOverride", {
      userAgent:
        "Mozilla/5.0 (Linux; Android 15; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36",
      acceptLanguage: LOCALE,
    });
    await s("Page.addScriptToEvaluateOnNewDocument", {
      source: startScript(state.shots[`codes-${scheme}`]),
    });
    await s("Fetch.enable", {
      patterns: [{ urlPattern: "*mock-backend*", requestStage: "Response" }],
    });
    let patched = 0;
    const onPaused = async (msg) => {
      if (msg.method !== "Fetch.requestPaused" || msg.sessionId !== sessionId) return;
      const { requestId, responseHeaders, responseStatusCode } = msg.params;
      const body = await s("Fetch.getResponseBody", { requestId });
      let text = body.base64Encoded ? Buffer.from(body.body, "base64").toString("utf8") : body.body;
      if (/export function sampleEntries\b/.test(text) && /export function fakeCode\b/.test(text)) {
        text += PATCH;
        patched += 1;
      }
      await s("Fetch.fulfillRequest", {
        requestId,
        responseCode: responseStatusCode,
        responseHeaders,
        body: Buffer.from(text).toString("base64"),
      });
    };
    listeners.add(onPaused);
    const evaluate = async (expression) => {
      const r = await s("Runtime.evaluate", {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (r.exceptionDetails)
        throw new Error(
          `${r.exceptionDetails.text} ${r.exceptionDetails.exception?.description ?? ""}`,
        );
      return r.result.value;
    };
    const until = async (expression, what, ms = 30000) => {
      const end = Date.now() + ms;
      for (;;) {
        if (await evaluate(expression).catch(() => false)) return;
        if (Date.now() > end) throw new Error(`phone-screens: timed out waiting for ${what}`);
        await new Promise((r) => setTimeout(r, 200));
      }
    };
    const click = (selector) =>
      evaluate(
        `(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return false; el.click(); return true; })()`,
      );
    const shot = async (page) => {
      // Two captures alike: nothing is still moving.
      let last = null;
      let data = null;
      const end = Date.now() + 8000;
      for (;;) {
        ({ data } = await deadline(
          s("Page.captureScreenshot", { format: "png" }),
          20000,
          "the screenshot",
        ));
        if (data === last || Date.now() > end) break;
        last = data;
        await new Promise((r) => setTimeout(r, 250));
      }
      const file = join(args.out, `${scheme}-${page}.png`);
      writeFileSync(file, Buffer.from(data, "base64"));
      console.log(`phone-screens: ${file}`);
    };

    const page = new URL(args.url);
    if (args.passwords) page.searchParams.set("passwords", "");
    await s("Page.navigate", { url: page.href });
    await until(
      "!!document.querySelector('[data-testid=page-unlock] input[type=password]')",
      "the lock screen",
    );
    if (patched !== 1) throw new Error("phone-screens: the mock module was not patched");
    // The mock's own password (packages/shared/src/mock-backend.ts, MOCK_PASSWORD).
    await evaluate(`(() => {
      const input = document.querySelector('[data-testid=page-unlock] input[type=password]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, 'correct horse battery');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.form ? input.form.requestSubmit() : input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    })()`);
    const theme = scheme === "dark" ? "glass_night" : "glass";
    await until(
      `!!document.querySelector('[data-testid=page-codes]') && document.documentElement.dataset.theme === '${theme}'`,
      "the codes page",
    );
    await until(
      `document.querySelectorAll('[data-testid=page-codes] [data-testid=row-more]').length === ${state.accounts.length}`,
      "every account",
    );
    await shot("codes");
    if (args.passwords) {
      await click("[data-testid=dock-logins]");
      await until(
        "document.querySelectorAll('[data-testid=page-logins] [data-testid=password-row]').length > 0",
        "the logins",
      );
      await shot("logins");
      await click("[data-testid=page-logins] [data-testid=password-row]");
      await until(
        "!!document.querySelector('[data-testid=page-login] [data-testid=login-edit]')",
        "a login",
      );
      await shot("login");
      await evaluate("history.go(-2)");
      await until(
        "!!document.querySelector('[data-testid=page-codes] [data-testid=codes-add]')",
        "the codes page again",
      );
    }
    await click("[data-testid=codes-add]");
    await until(
      "!!document.querySelector('[data-testid=page-add] [data-testid=add-manual]')",
      "the add page",
    );
    await shot("add");
    await click("[data-testid=page-add] button");
    await until("!!document.querySelector('[data-testid=dock-settings]')", "the codes page again");
    await click("[data-testid=dock-settings]");
    await until("!!document.querySelector('[data-testid=settings-sync]')", "the settings page");
    await click("[data-testid=settings-sync]");
    await until(
      "!!document.querySelector('[data-testid=page-sync] [data-testid=sync-setup-open]')",
      "the sync page",
    );
    await shot("sync");
    listeners.delete(onPaused);
    await send("Target.detachFromTarget", { sessionId }).catch(() => {});
  } finally {
    if (targetId) await send("Target.closeTarget", { targetId }).catch(() => {});
    await send("Target.disposeBrowserContext", { browserContextId }).catch(() => {});
  }
}

let status = 0;
try {
  for (const scheme of ["light", "dark"]) await capture(scheme);
} catch (error) {
  console.error(error.message);
  status = 1;
} finally {
  ws.close();
}
process.exit(status);
