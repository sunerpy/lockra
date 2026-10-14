// The background's connection to Lockra (docs/formats.md §10): a native-messaging port to the host
// `dev.lockra.browser`, which passes frames to the app's socket. One state machine per port:
//
//   hello ─► the app's state and version ─► (paired, unlocked) KK session ─► requests
//                                         └► (user clicks 连接) XX pairing ─► code ─► paired ─► KK
//
// Lockra locking ends the session (the app says `state: locked`); unlocking opens a new one. A
// `revoked` push or `not_running` from the host (Lockra quit) end it too. The session's keys live
// in this object only; the extension's static key is a non-extractable CryptoKey (Keys), and the
// pairing (its id and the app's public key) is all that is kept. Every decision on a request is the
// app's: this side carries messages and reports what the browser says.
import { fromBase64, toBase64, utf8, wipe } from "../noise/bytes";
import type { Channel } from "../noise/channel";
import type { KeyPair } from "../noise/crypto";
import { Initiator } from "../noise/handshake";
import { decodeEnvelope, encodeEnvelope, encodePlain, parseJson } from "../protocol/envelope";
import { clientHello, request, startApp, unlockRequest } from "../protocol/messages";
import {
  ANSWER_SCHEMAS,
  type AnswerOf,
  type AppPlain,
  type AppState,
  type ClientInfo,
  type Envelope,
  type Op,
  type OpName,
  PROTO_MIN,
  type Push,
  type Response,
  appPlainSchema,
  outgoingSchema,
} from "../protocol/schemas";
import type { Clock } from "./clock";

/** The host's end of `runtime.connectNative`. */
export interface NativePort {
  post(raw: unknown): void;
  onMessage(listener: (raw: unknown) => void): void;
  /** `error`: what the browser said (`runtime.lastError`, Firefox's `port.error`). */
  onDisconnect(listener: (error?: string) => void): void;
  disconnect(): void;
}

/** What the extension keeps of a pairing: its id and the app's static public key (base64). */
export interface PairingRecord {
  id: string;
  app_public: string;
  paired_at: number;
}

/** Where the keys live: the static key pair (IndexedDB, non-extractable) and the pairing
 *  (storage.local). */
export interface Keys {
  staticPair(): Promise<KeyPair>;
  pairing(): Promise<PairingRecord | null>;
  setPairing(record: PairingRecord | null): Promise<void>;
}

export interface ConnectionDeps {
  connectNative(): NativePort;
  keys: Keys;
  clock: Clock;
  /** What the hello says of this extension, its site access read fresh. */
  client(): Promise<ClientInfo>;
  /** Fixed ephemeral keys, for tests only. */
  ephemeral?: () => Promise<KeyPair>;
}

export type Link = "connecting" | "no_host" | "not_running" | "incompatible" | "ready";

export type PairingView =
  | { stage: "none" }
  | { stage: "handshake" }
  | { stage: "waiting"; code: string; expiresAt: number | null }
  | { stage: "failed"; reason: string };

export interface ConnectionView {
  link: Link;
  /** Why there is no host: the browser's words. */
  detail?: string;
  /** Lockra, once it answered the hello. */
  app?: { state: AppState; version: string };
  /** Which side needs an update when the versions share none. */
  incompatible?: "app" | "extension";
  /** Lockra is being started (`start_app`): the hello goes every second for thirty seconds. */
  starting: boolean;
  paired: boolean;
  pairing: PairingView;
  session: "none" | "handshake" | "open";
}

/** A request's outcome: the answer, or why there is none (a refusal of the app's, or `not_paired`,
 *  `locked`, `no_vault`, `not_running`, `no_host`, `incompatible`, `ended`, `timeout`,
 *  `invalid_answer`). */
export type Outcome<T> = { ok: true; value: T } | { ok: false; error: string };

const RECONNECT_MS = [1_000, 2_000, 4_000, 8_000, 15_000, 30_000] as const;
const START_POLL_MS = 1_000;
const START_POLL_FOR_MS = 30_000;
const UNLOCK_ANSWER_MS = 5_000;
/** How often pages may make the background look for a missing host, or say hello again to a host
 *  whose Lockra is not running. */
const HOST_RETRY_MS = 60_000;
const HELLO_RETRY_MS = 5_000;
/** How long a request waits for a session that is about to open. */
const SESSION_WAIT_MS = 5_000;
const REQUEST_MS = 15_000;
/** A fill or a copy may wait for the user in Lockra (a minute, `CONFIRM_TTL`). */
const WAITING_REQUEST_MS = 75_000;

interface Pending {
  timer: number;
  /** Read the answer (with the schema of the op asked) or the refusal, and resolve. */
  settle(response: Response): void;
  /** Resolve without an answer. */
  fail(error: string): void;
}

export class Connection {
  private port: NativePort | null = null;
  /** Something came on this port: a host answered (else a closed port means no host). */
  private heard = false;
  private attempts = 0;
  private reconnectTimer: number | undefined;
  private pollTimer: number | undefined;
  private startingUntil = 0;
  private lastAttempt = Number.NEGATIVE_INFINITY;
  private lastHello = Number.NEGATIVE_INFINITY;
  private inbox: Promise<void> = Promise.resolve();
  private outbox: Promise<void> = Promise.resolve();

  private xx: Initiator | null = null;
  private pairingChannel: Channel | null = null;
  private pairingAppKey: Uint8Array | null = null;
  private kk: Initiator | null = null;
  private sessionChannel: Channel | null = null;

  private nextReq = 1;
  private readonly pending = new Map<number, Pending>();
  private readonly listeners = new Set<(view: ConnectionView) => void>();
  private readonly lockListeners = new Set<() => void>();
  private sessionWaiters: (() => void)[] = [];
  private unlockWaiters: ((accepted: boolean) => void)[] = [];

  private state: ConnectionView = {
    link: "connecting",
    starting: false,
    paired: false,
    pairing: { stage: "none" },
    session: "none",
  };

  constructor(private readonly deps: ConnectionDeps) {}

  get view(): ConnectionView {
    return this.state;
  }

  subscribe(listener: (view: ConnectionView) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Lockra locked, quit, or the pairing went: whatever the extension showed of it goes too. */
  onLocked(listener: () => void): void {
    this.lockListeners.add(listener);
  }

  /** Connect once; later calls keep the connection. */
  start(): void {
    if (this.port) return;
    this.connect();
    void this.refreshPaired();
  }

  /** Something needs Lockra now: try again what can be tried. `byPage` (a page loaded or a field
   *  got the focus, which every frame of every page may do) tries a missing host at most once a
   *  minute and a Lockra that is not running at most every five seconds; the user (the popup)
   *  always. */
  ensure(byPage = false): void {
    const now = this.deps.clock.now();
    if (!this.port) {
      if (byPage && now - this.lastAttempt < HOST_RETRY_MS) return;
      this.deps.clock.clearTimeout(this.reconnectTimer);
      this.connect();
      return;
    }
    if (this.state.link === "not_running" || this.state.link === "connecting") {
      if (byPage && now - this.lastHello < HELLO_RETRY_MS) return;
      this.sendHello();
    } else {
      void this.openSession();
    }
  }

  /** Say hello again: the site access changed. The app starts the connection over. */
  refreshHello(): void {
    if (!this.port) return this.ensure();
    this.endSession();
    // The app ends a pairing in progress too: the code shown is no use any more.
    this.endPairing(true);
    this.sendHello();
  }

  /** Start pairing (the popup's 「连接」): Lockra must be unlocked. */
  async pair(): Promise<void> {
    if (this.state.link !== "ready" || this.state.app?.state !== "unlocked") {
      throw new Error("Lockra is not ready to pair");
    }
    this.endSession();
    this.endPairing();
    // A new hello puts the app's side back before any handshake.
    this.sendHello();
    const local = await this.deps.keys.staticPair();
    const handshake = await Initiator.pairing({ local, ...this.ephemeral() });
    this.xx = handshake;
    this.update({ pairing: { stage: "handshake" } });
    const first = await handshake.write();
    this.post(() => encodeEnvelope("hs", first));
  }

  /** Stop a pairing in progress: the request goes from Lockra's screen. */
  cancelPairing(): void {
    this.endPairing();
    this.update({ pairing: { stage: "none" } });
    this.sendHello();
  }

  /** Ask Lockra to show its unlock screen; whether it passed the request on. */
  unlock(): Promise<boolean> {
    if (!this.port) return Promise.resolve(false);
    return new Promise((resolve) => {
      const timer = this.deps.clock.setTimeout(() => {
        this.unlockWaiters = this.unlockWaiters.filter((w) => w !== waiter);
        resolve(false);
      }, UNLOCK_ANSWER_MS);
      const waiter = (accepted: boolean) => {
        this.deps.clock.clearTimeout(timer);
        resolve(accepted);
      };
      this.unlockWaiters.push(waiter);
      this.post(() => encodePlain(unlockRequest()));
    });
  }

  /** Ask the host to start Lockra (`show`: and bring its window up). */
  startApp(show: boolean): void {
    if (!this.port) this.connect();
    this.post(() => encodePlain(startApp(show)));
    if (this.state.link === "not_running") {
      this.startingUntil = this.deps.clock.now() + START_POLL_FOR_MS;
      this.update({ starting: true });
      this.schedulePoll();
    }
  }

  /** Forget the pairing here (the options page's 「取消配对」); Lockra keeps its side until the
   *  user removes it there, and pairing again replaces it. */
  async forget(): Promise<void> {
    await this.deps.keys.setPairing(null);
    this.endSession();
    this.update({ paired: false });
    this.locked();
    if (this.port) this.sendHello();
  }

  /** The pairing kept here (the options page shows when it was made). */
  pairingRecord(): Promise<PairingRecord | null> {
    return this.deps.keys.pairing();
  }

  /** A request over the session, waiting a moment for a session that is opening. */
  async request<O extends OpName>(op: Extract<Op, { op: O }>): Promise<Outcome<AnswerOf<O>>> {
    const blocked = await this.whenSession();
    if (blocked) return { ok: false, error: blocked };
    const channel = this.sessionChannel;
    if (!channel) return { ok: false, error: "ended" };
    const req = this.nextReq++;
    const bytes = utf8.encode(JSON.stringify(request(req, op)));
    const timeout = op.op === "fill" || op.op === "copy" ? WAITING_REQUEST_MS : REQUEST_MS;
    const schema: (typeof ANSWER_SCHEMAS)[O] = ANSWER_SCHEMAS[op.op];
    return new Promise<Outcome<AnswerOf<O>>>((resolve) => {
      const timer = this.deps.clock.setTimeout(() => {
        this.pending.delete(req);
        resolve({ ok: false, error: "timeout" });
      }, timeout);
      this.pending.set(req, {
        timer,
        settle: (response) => {
          if (!response.ok) return resolve({ ok: false, error: response.error });
          const parsed = schema.safeParse(response.result);
          if (!parsed.success) return resolve({ ok: false, error: "invalid_answer" });
          // Parsed with the schema of the op asked: its output is that op's answer, which the
          // compiler cannot follow through the generic index.
          // oxlint-disable-next-line typescript/no-unsafe-type-assertion
          resolve({ ok: true, value: parsed.data as AnswerOf<O> });
        },
        fail: (error) => resolve({ ok: false, error }),
      });
      // Fragments go out in their nonces' order, one message after the other.
      this.outbox = this.outbox.then(async () => {
        if (this.sessionChannel !== channel || !this.port) return;
        const sealed = await channel.seal(bytes);
        wipe(bytes);
        for (const fragment of sealed) this.postNow(encodeEnvelope("msg", fragment));
      });
    });
  }

  // ---- the port ----------------------------------------------------------------------------

  private connect(): void {
    this.heard = false;
    this.lastAttempt = this.deps.clock.now();
    let port: NativePort;
    try {
      port = this.deps.connectNative();
    } catch (error) {
      this.update({
        link: "no_host",
        detail: error instanceof Error ? error.message : String(error),
      });
      return;
    }
    this.port = port;
    port.onMessage((raw) => {
      if (port !== this.port) return;
      this.heard = true;
      this.inbox = this.inbox.then(() => this.handle(raw)).catch(() => this.broken());
    });
    port.onDisconnect((error) => {
      if (port === this.port) this.disconnected(error);
    });
    this.update({ link: "connecting", detail: undefined });
    this.sendHello();
  }

  private disconnected(error?: string): void {
    this.port = null;
    this.endSession();
    this.endPairing(true);
    if (!this.heard) {
      // The browser found no host to start, or the host died before a word: nothing to retry
      // until something asks (the popup opening says what to do).
      this.update({ link: "no_host", detail: error ?? "", starting: false });
      return;
    }
    this.update({ link: "connecting", starting: false });
    this.locked();
    const pause = RECONNECT_MS[Math.min(this.attempts, RECONNECT_MS.length - 1)] ?? 30_000;
    this.attempts++;
    this.reconnectTimer = this.deps.clock.setTimeout(() => {
      if (!this.port) this.connect();
    }, pause);
  }

  /** The host broke the protocol: drop the port and connect again later. */
  private broken(): void {
    const port = this.port;
    if (!port) return;
    port.disconnect();
    this.heard = true;
    this.disconnected();
  }

  /** Queue `make()`'s envelope after everything queued before it. */
  private post(make: () => Envelope | Promise<Envelope>): void {
    this.outbox = this.outbox.then(async () => {
      const envelope = await make();
      this.postNow(envelope);
    });
  }

  private postNow(envelope: Envelope): void {
    const port = this.port;
    if (!port) return;
    try {
      port.post(envelope);
    } catch {
      // A port the browser closed throws; its onDisconnect says why, a moment later. Should that
      // never come, the port counts as closed after a second.
      this.deps.clock.setTimeout(() => {
        if (this.port === port) this.disconnected("the port closed");
      }, 1_000);
    }
  }

  private sendHello(): void {
    this.lastHello = this.deps.clock.now();
    this.post(async () => encodePlain(clientHello(await this.deps.client())));
  }

  private schedulePoll(): void {
    this.deps.clock.clearTimeout(this.pollTimer);
    if (this.deps.clock.now() >= this.startingUntil) {
      if (this.state.starting) this.update({ starting: false });
      return;
    }
    this.pollTimer = this.deps.clock.setTimeout(() => {
      if (this.state.link !== "not_running") return;
      this.sendHello();
      this.schedulePoll();
    }, START_POLL_MS);
  }

  // ---- what comes in -------------------------------------------------------------------------

  private async handle(raw: unknown): Promise<void> {
    const envelope = decodeEnvelope(raw);
    if (!envelope) return this.broken();
    if (envelope.t === "hello") {
      const parsed = appPlainSchema.safeParse(parseJson(envelope.bytes));
      // A plain message of a later version: nothing this one does with it.
      if (parsed.success) await this.plain(parsed.data);
      return;
    }
    if (envelope.t === "hs") return this.handshake(envelope.bytes);
    return this.message(envelope.bytes);
  }

  private async plain(message: AppPlain): Promise<void> {
    switch (message.op) {
      case "hello": {
        this.attempts = 0;
        this.startingUntil = 0;
        this.deps.clock.clearTimeout(this.pollTimer);
        const app = { state: message.state, version: message.app_version };
        if (message.proto === null) {
          this.endSession();
          this.endPairing();
          this.update({
            link: "incompatible",
            app,
            incompatible: message.proto_max < PROTO_MIN ? "app" : "extension",
            starting: false,
          });
          return;
        }
        this.update({ link: "ready", app, incompatible: undefined, starting: false });
        if (message.state === "unlocked") await this.openSession();
        else this.lockedApp();
        return;
      }
      case "state": {
        if (message.state === "not_running") {
          this.endSession();
          this.endPairing(true);
          this.update({ link: "not_running", app: undefined });
          this.locked();
          if (this.state.starting) this.schedulePoll();
          return;
        }
        if (this.state.link !== "ready" || !this.state.app) return;
        this.update({ app: { ...this.state.app, state: message.state } });
        if (message.state === "unlocked") await this.openSession();
        else this.lockedApp();
        return;
      }
      case "unlock": {
        const waiters = this.unlockWaiters;
        this.unlockWaiters = [];
        for (const waiter of waiters) waiter(message.accepted);
        return;
      }
      case "error":
        return this.error(message.error);
    }
  }

  /** The app could not answer a handshake. */
  private async error(error: string): Promise<void> {
    if (this.kk || this.state.session === "handshake") {
      this.endSession();
      if (error === "not_paired") {
        // Lockra no longer has this pairing (removed while the extension was away).
        await this.deps.keys.setPairing(null);
        this.update({ paired: false });
        this.locked();
      }
      return;
    }
    if (this.xx || this.state.pairing.stage === "handshake") {
      this.endPairing();
      this.update({ pairing: { stage: "failed", reason: error } });
      return;
    }
    if (error === "hello_first") this.sendHello();
  }

  private async handshake(bytes: Uint8Array): Promise<void> {
    const xx = this.xx;
    if (xx && this.state.pairing.stage === "handshake") {
      await xx.read(bytes);
      const third = await xx.write();
      this.post(() => encodeEnvelope("hs", third));
      const code = await xx.code();
      this.pairingAppKey = xx.remoteStatic;
      this.pairingChannel = await xx.channel();
      this.xx = null;
      this.update({ pairing: { stage: "waiting", code, expiresAt: null } });
      return;
    }
    const kk = this.kk;
    if (kk && this.state.session === "handshake") {
      await kk.read(bytes);
      this.sessionChannel = await kk.channel();
      this.kk = null;
      // Open once the app says `ready` over it.
    }
    // Otherwise a stage that ended meanwhile (the app answers in order): nothing to do.
  }

  private async message(bytes: Uint8Array): Promise<void> {
    const channel = this.sessionChannel ?? this.pairingChannel;
    if (!channel) return;
    const plain = await channel.open(bytes);
    if (plain === null) return;
    const parsed = outgoingSchema.safeParse(parseJson(plain));
    wipe(plain);
    if (!parsed.success) return;
    if (parsed.data.kind === "push")
      await this.push(parsed.data.push, channel === this.pairingChannel);
    else this.answer(parsed.data.response);
  }

  private async push(push: Push, pairing: boolean): Promise<void> {
    switch (push.push) {
      case "pair_pending": {
        const view = this.state.pairing;
        if (pairing && view.stage === "waiting") {
          this.update({
            pairing: { ...view, expiresAt: this.deps.clock.now() + push.expires_in_ms },
          });
        }
        return;
      }
      case "paired": {
        const key = this.pairingAppKey;
        if (!pairing || !key) return;
        await this.deps.keys.setPairing({
          id: push.pairing,
          app_public: toBase64(key),
          paired_at: this.deps.clock.now(),
        });
        this.endPairing();
        this.update({ paired: true, pairing: { stage: "none" } });
        // The app is back before a handshake: the session follows.
        await this.openSession();
        return;
      }
      case "pair_failed":
        if (!pairing) return;
        this.endPairing();
        this.update({ pairing: { stage: "failed", reason: push.reason } });
        return;
      case "ready":
        if (pairing || this.state.session !== "handshake") return;
        this.update({ session: "open" });
        this.wakeSessionWaiters();
        return;
      case "revoked":
        await this.deps.keys.setPairing(null);
        this.endSession();
        this.update({ paired: false });
        this.locked();
        return;
    }
  }

  private answer(response: Response): void {
    const pending = this.pending.get(response.req);
    if (!pending) return;
    this.pending.delete(response.req);
    this.deps.clock.clearTimeout(pending.timer);
    pending.settle(response);
  }

  // ---- sessions ------------------------------------------------------------------------------

  /** Open a session when everything allows one and none is open or opening. */
  private async openSession(): Promise<void> {
    if (this.state.link !== "ready" || this.state.app?.state !== "unlocked") return;
    if (this.state.session !== "none" || this.xx || this.pairingChannel) return;
    const record = await this.refreshPaired();
    if (!record) return this.wakeSessionWaiters();
    const remote = fromBase64(record.app_public);
    if (!remote || remote.length !== 32) {
      await this.deps.keys.setPairing(null);
      this.update({ paired: false });
      return this.wakeSessionWaiters();
    }
    // Checked again: the awaits above let other messages in.
    if (this.state.session !== "none" || this.xx || this.pairingChannel) return;
    this.update({ session: "handshake" });
    const local = await this.deps.keys.staticPair();
    const handshake = await Initiator.session({ local, remote, ...this.ephemeral() });
    this.kk = handshake;
    const first = await handshake.write();
    this.post(() => encodeEnvelope("hs", first, record.id));
  }

  private async refreshPaired(): Promise<PairingRecord | null> {
    const record = await this.deps.keys.pairing();
    if (this.state.paired !== (record !== null)) this.update({ paired: record !== null });
    return record;
  }

  /** Why no request can go now, after waiting a moment for a session that is opening. */
  private async whenSession(): Promise<string | null> {
    const deadline = this.deps.clock.now() + SESSION_WAIT_MS;
    for (;;) {
      const blocked = this.blocked();
      if (blocked !== "opening") return blocked;
      if (this.deps.clock.now() >= deadline) return "timeout";
      // One wake-up at a time: the state is read again after each.
      // oxlint-disable-next-line no-await-in-loop
      await new Promise<void>((resolve) => {
        const timer = this.deps.clock.setTimeout(
          resolve,
          Math.max(0, deadline - this.deps.clock.now()),
        );
        this.sessionWaiters.push(() => {
          this.deps.clock.clearTimeout(timer);
          resolve();
        });
      });
    }
  }

  /** `null` when a request can go now, `opening` when a session is on its way, else why not. */
  private blocked(): string | null {
    const { link, app, paired, session } = this.state;
    if (session === "open" && this.sessionChannel) return null;
    if (link === "connecting") return "opening";
    if (link !== "ready") return link;
    if (app?.state !== "unlocked") return app?.state ?? "locked";
    if (!paired) return "not_paired";
    return "opening";
  }

  private wakeSessionWaiters(): void {
    const waiters = this.sessionWaiters;
    this.sessionWaiters = [];
    for (const waiter of waiters) waiter();
  }

  private endSession(): void {
    this.kk = null;
    this.sessionChannel = null;
    if (this.state.session !== "none") this.update({ session: "none" });
    for (const [req, pending] of this.pending) {
      this.deps.clock.clearTimeout(pending.timer);
      pending.fail("ended");
      this.pending.delete(req);
    }
    this.wakeSessionWaiters();
  }

  /** Drop a pairing in progress; `failed`: tell the popup it ended without one. */
  private endPairing(failed = false): void {
    const stage = this.state.pairing.stage;
    this.xx = null;
    this.pairingChannel = null;
    if (this.pairingAppKey) wipe(this.pairingAppKey);
    this.pairingAppKey = null;
    if (failed && (stage === "handshake" || stage === "waiting")) {
      this.update({ pairing: { stage: "failed", reason: "ended" } });
    }
  }

  /** The app is locked (or has no vault): nothing of a session survives it. */
  private lockedApp(): void {
    this.endSession();
    const stage = this.state.pairing.stage;
    if (stage === "handshake" || stage === "waiting") {
      this.endPairing();
      this.update({ pairing: { stage: "failed", reason: "locked" } });
    }
    this.locked();
  }

  private locked(): void {
    for (const listener of this.lockListeners) listener();
  }

  private ephemeral(): { ephemeral?: () => Promise<KeyPair> } {
    return this.deps.ephemeral ? { ephemeral: this.deps.ephemeral } : {};
  }

  private update(change: Partial<ConnectionView>): void {
    const next: ConnectionView = { ...this.state, ...change };
    // A field set to undefined goes: the view never holds an undefined value.
    for (const [key, value] of Object.entries(change))
      if (value === undefined) Reflect.deleteProperty(next, key);
    this.state = next;
    for (const listener of this.listeners) listener(next);
    if (next.session === "open" || next.link !== "ready") this.wakeSessionWaiters();
  }
}
