// A fake Lockra behind a fake native port, for the background's tests: the responder's side of both
// handshakes (built on the extension's own primitives; the vectors check those against snow), the
// app's plain answers, pushes, and canned answers to requests. It follows the app's service loop
// (crates/lockra-browser/src/server.rs) closely enough to drive the state machine through pairing,
// sessions, locking, revoking and the host's `not_running`.
import { concat, fromBase64, toBase64, utf8 } from "../noise/bytes";
import { Channel } from "../noise/channel";
import { type KeyPair, dh, generateKeyPair } from "../noise/crypto";
import { PAIRING_PATTERN, PROLOGUE, SESSION_PATTERN } from "../noise/handshake";
import { pairingCode } from "../noise/code";
import { SymmetricState } from "../noise/symmetric";
import type { NativePort } from "../background/connection";

type Raw = Record<string, unknown>;

const TAG = 16;

class Responder {
  private e: KeyPair | null = null;
  private re: Uint8Array | null = null;
  rs: Uint8Array | null = null;

  private constructor(
    readonly pattern: "XX" | "KK",
    readonly state: SymmetricState,
    private readonly local: KeyPair,
  ) {}

  static async pairing(local: KeyPair): Promise<Responder> {
    const state = await SymmetricState.of(PAIRING_PATTERN);
    await state.mixHash(PROLOGUE);
    return new Responder("XX", state, local);
  }

  static async session(local: KeyPair, remote: Uint8Array): Promise<Responder> {
    const state = await SymmetricState.of(SESSION_PATTERN);
    await state.mixHash(PROLOGUE);
    await state.mixHash(remote);
    await state.mixHash(local.publicKey);
    const responder = new Responder("KK", state, local);
    responder.rs = remote;
    return responder;
  }

  /** XX's first message, or KK's (`e, es, ss`). */
  async readFirst(message: Uint8Array): Promise<void> {
    const re = message.slice(0, 32);
    this.re = re;
    await this.state.mixHash(re);
    if (this.pattern === "KK") {
      await this.state.mixKey(await dh(this.local.privateKey, re));
      await this.state.mixKey(await dh(this.local.privateKey, this.rs ?? new Uint8Array()));
    }
    await this.state.decryptAndHash(message.subarray(32));
  }

  /** XX's `e, ee, s, es` or KK's `e, ee, se`. */
  async writeSecond(): Promise<Uint8Array> {
    const e = await generateKeyPair();
    this.e = e;
    const re = this.re ?? new Uint8Array();
    await this.state.mixHash(e.publicKey);
    await this.state.mixKey(await dh(e.privateKey, re));
    const parts = [e.publicKey];
    if (this.pattern === "XX") {
      parts.push(await this.state.encryptAndHash(this.local.publicKey));
      await this.state.mixKey(await dh(this.local.privateKey, re));
    } else {
      await this.state.mixKey(await dh(e.privateKey, this.rs ?? new Uint8Array()));
    }
    parts.push(await this.state.encryptAndHash(new Uint8Array()));
    return concat(...parts);
  }

  /** XX's `s, se`. */
  async readThird(message: Uint8Array): Promise<void> {
    if (!this.e) throw new Error("the second message first");
    const rs = await this.state.decryptAndHash(message.subarray(0, 32 + TAG));
    this.rs = rs;
    await this.state.mixKey(await dh(this.e.privateKey, rs));
    await this.state.decryptAndHash(message.subarray(32 + TAG));
  }

  async channel(): Promise<Channel> {
    const [initiatorSends, responderSends] = await this.state.split();
    return new Channel(responderSends, initiatorSends);
  }
}

export type Handler = (request: Raw) => Raw | Promise<Raw>;

/** One connection's worth of the app and the host before it. */
export class FakeApp {
  key!: KeyPair;
  state: "no_vault" | "locked" | "unlocked" = "unlocked";
  version = "0.10.0";
  protoMin = 1;
  protoMax = 1;
  /** The host cannot reach the app. */
  running = true;
  /** Pairings the app keeps: id → the extension's public key. */
  peers = new Map<string, Uint8Array>();
  /** What the extension posted, decoded: plain messages as JSON, others by kind. */
  posted: { t: string; plain?: Raw; p?: string }[] = [];
  /** Requests received over sessions. */
  requests: Raw[] = [];
  handlers: Record<string, Handler> = {
    status: () => ({
      pairing: [...this.peers.keys()][0],
      app_version: this.version,
      policy: {},
      items: 3,
      paused: false,
    }),
  };
  /** The pairing code the app shows, once the third message came. */
  code: string | null = null;
  /** Make the next pairing fail at once with this reason. */
  refusePairing: string | null = null;
  ports: FakePort[] = [];
  /** connectNative fails: no host is installed. */
  noHost: string | null = null;

  private stage: "fresh" | "pairing" | "waiting" | "session" = "fresh";
  private responder: Responder | null = null;
  private channel: Channel | null = null;
  private sessionPairing: string | null = null;
  private helloSeen = false;
  private queue: Promise<void> = Promise.resolve();
  private sealing: Promise<void> = Promise.resolve();
  private pendingPeerKey: Uint8Array | null = null;

  static async start(): Promise<FakeApp> {
    const app = new FakeApp();
    app.key = await generateKeyPair();
    return app;
  }

  get port(): FakePort | undefined {
    return this.ports.at(-1);
  }

  /** What `connectNative` returns. */
  connect = (): NativePort => {
    const port = new FakePort(this);
    this.ports.push(port);
    this.stage = "fresh";
    this.helloSeen = false;
    this.channel = null;
    if (this.noHost !== null) {
      const error = this.noHost;
      queueMicrotask(() => port.close(error));
    }
    return port;
  };

  receive(raw: Raw): void {
    this.queue = this.queue.then(() => this.handle(raw)).catch(() => this.port?.close("app error"));
  }

  /** Done handling what came in so far. */
  async idle(): Promise<void> {
    let seen: Promise<void> | null = null;
    while (seen !== this.queue) {
      seen = this.queue;
      await seen;
    }
  }

  private async handle(raw: Raw): Promise<void> {
    const bytes = fromBase64(typeof raw.d === "string" ? raw.d : "") ?? new Uint8Array();
    const t = typeof raw.t === "string" ? raw.t : "";
    const p = typeof raw.p === "string" ? raw.p : null;
    if (t === "hello") {
      const plain = JSON.parse(utf8.decode(bytes)) as Raw;
      this.posted.push({ t, plain });
      return this.plain(plain);
    }
    this.posted.push(p === null ? { t } : { t, p });
    if (!this.running) return;
    if (t === "hs") return this.handshake(bytes, p);
    if (t === "msg") return this.message(bytes);
  }

  private async plain(plain: Raw): Promise<void> {
    if (plain.op === "start_app") {
      return;
    }
    if (!this.running) {
      if (plain.op === "hello") this.sendPlain({ op: "state", state: "not_running" });
      return;
    }
    if (plain.op === "hello") {
      this.endStage();
      this.helloSeen = true;
      const proto =
        Math.max(Number(plain.proto_min), this.protoMin) <=
        Math.min(Number(plain.proto_max), this.protoMax)
          ? Math.min(Number(plain.proto_max), this.protoMax)
          : null;
      this.sendPlain({
        op: "hello",
        proto_min: this.protoMin,
        proto_max: this.protoMax,
        proto,
        app_version: this.version,
        state: this.state,
      });
    } else if (plain.op === "unlock_request") {
      this.sendPlain({ op: "unlock", accepted: this.state === "locked" });
    }
  }

  private async handshake(bytes: Uint8Array, pairing: string | null): Promise<void> {
    if (!this.helloSeen) return this.sendPlain({ op: "error", error: "hello_first" });
    if (this.state !== "unlocked") return this.sendPlain({ op: "error", error: "locked" });
    if (this.stage === "fresh" && pairing === null) {
      this.responder = await Responder.pairing(this.key);
      await this.responder.readFirst(bytes);
      this.stage = "pairing";
      return this.send("hs", await this.responder.writeSecond());
    }
    if (this.stage === "pairing" && this.responder) {
      await this.responder.readThird(bytes);
      this.code = await pairingCode(this.responder.state.hash);
      this.pendingPeerKey = this.responder.rs;
      this.channel = await this.responder.channel();
      this.stage = "waiting";
      this.responder = null;
      await this.seal({ push: "pair_pending", expires_in_ms: 120_000 });
      if (this.refusePairing) {
        await this.seal({ push: "pair_failed", reason: this.refusePairing });
        this.channel = null;
        this.stage = "fresh";
      }
      return;
    }
    if (this.stage === "fresh" && pairing !== null) {
      const peer = this.peers.get(pairing);
      if (!peer) return this.sendPlain({ op: "error", error: "not_paired" });
      const responder = await Responder.session(this.key, peer);
      try {
        await responder.readFirst(bytes);
      } catch {
        return this.sendPlain({ op: "error", error: "not_paired" });
      }
      this.send("hs", await responder.writeSecond());
      this.channel = await responder.channel();
      this.stage = "session";
      this.sessionPairing = pairing;
      return this.seal({ push: "ready", pairing });
    }
    this.port?.close("protocol");
  }

  private async message(bytes: Uint8Array): Promise<void> {
    if (this.stage !== "session" || !this.channel) return this.port?.close("protocol");
    const plain = await this.channel.open(bytes);
    if (plain === null) return;
    const request = JSON.parse(utf8.decode(plain)) as Raw;
    this.requests.push(request);
    const handler = this.handlers[String(request.op)];
    if (!handler) return this.seal({ req: request.req, ok: false, error: "invalid" });
    // Answered when the handler is done, without holding up what comes next (as the app's fills
    // that wait for the user).
    void Promise.resolve(handler(request)).then((answer) =>
      typeof answer.error === "string"
        ? this.seal({ req: request.req, ok: false, error: answer.error })
        : this.seal({ req: request.req, ok: true, result: answer }),
    );
  }

  // ---- what the test makes the app do ----

  /** The user approves the pairing in Lockra. */
  async approve(id = crypto.randomUUID()): Promise<string> {
    await this.idle();
    if (this.stage !== "waiting" || !this.pendingPeerKey) throw new Error("nothing to approve");
    this.peers.set(id, this.pendingPeerKey);
    await this.seal({ push: "paired", pairing: id });
    this.channel = null;
    this.stage = "fresh";
    return id;
  }

  async deny(): Promise<void> {
    await this.idle();
    await this.seal({ push: "pair_failed", reason: "denied" });
    this.channel = null;
    this.stage = "fresh";
  }

  async lock(): Promise<void> {
    await this.idle();
    this.state = "locked";
    this.endStage();
    this.sendPlain({ op: "state", state: "locked" });
  }

  async unlock(): Promise<void> {
    await this.idle();
    this.state = "unlocked";
    this.sendPlain({ op: "state", state: "unlocked" });
  }

  /** The user removes the pairing in Lockra. */
  async revoke(): Promise<void> {
    await this.idle();
    if (this.sessionPairing) this.peers.delete(this.sessionPairing);
    if (this.stage === "session") await this.seal({ push: "revoked" });
    this.endStage();
  }

  /** The app quits: the host says so and stays. */
  async quit(): Promise<void> {
    await this.idle();
    this.running = false;
    this.endStage();
    this.sendPlain({ op: "state", state: "not_running" });
  }

  /** A push of the app's own, on the open session. */
  async push(message: Raw): Promise<void> {
    await this.idle();
    await this.seal(message);
  }

  private endStage(): void {
    this.stage = "fresh";
    this.channel = null;
    this.responder = null;
    this.sessionPairing = null;
  }

  private sendPlain(message: Raw): void {
    this.send("hello", utf8.encode(JSON.stringify(message)));
  }

  private send(t: string, bytes: Uint8Array): void {
    this.port?.deliver({ v: 1, t, d: toBase64(bytes) });
  }

  /** Seal and send in order: the fragments' nonces are the order the extension reads them in. */
  private seal(message: Raw): Promise<void> {
    const channel = this.channel;
    this.sealing = this.sealing.then(async () => {
      if (!channel || channel !== this.channel) return;
      for (const sealed of await channel.seal(utf8.encode(JSON.stringify(message))))
        this.send("msg", sealed);
    });
    return this.sealing;
  }
}

export class FakePort implements NativePort {
  private messageListeners: ((raw: unknown) => void)[] = [];
  private closeListeners: ((error?: string) => void)[] = [];
  closed = false;

  constructor(private readonly app: FakeApp) {}

  post(raw: unknown): void {
    if (this.closed) throw new Error("posted to a closed port");
    // A copy, as structured clone would make.
    this.app.receive(JSON.parse(JSON.stringify(raw)) as Raw);
  }

  onMessage(listener: (raw: unknown) => void): void {
    this.messageListeners.push(listener);
  }

  onDisconnect(listener: (error?: string) => void): void {
    this.closeListeners.push(listener);
  }

  disconnect(): void {
    this.closed = true;
  }

  /** The host sends a message, a moment later as a real port does. */
  deliver(raw: unknown): void {
    if (this.closed) return;
    setTimeout(() => {
      if (this.closed) return;
      for (const listener of this.messageListeners) listener(raw);
    }, 0);
  }

  /** The host went away. */
  close(error?: string): void {
    if (this.closed) return;
    this.closed = true;
    setTimeout(() => {
      for (const listener of this.closeListeners) listener(error);
    }, 0);
  }
}
