// The extension's side of the handshakes (docs/formats.md §10), the initiator in both:
// - pairing, Noise_XX_25519_AESGCM_SHA256: `-> e`, `<- e, ee, s, es`, `-> s, se`; neither side knows
//   the other yet, both end with the other's static key and show six digits from the hash;
// - a session, Noise_KK_25519_AESGCM_SHA256 between the paired keys: `-> s` `<- s` known first, then
//   `-> e, es, ss`, `<- e, ee, se`, with fresh ephemeral keys (forward secrecy per connection).
// Both bind the prologue `lockra-browser/1`. The ephemeral keys come from WebCrypto; the tests give
// fixed ones (`ephemeral`) to reproduce snow's messages byte for byte.
import { concat, utf8 } from "./bytes";
import { Channel } from "./channel";
import { pairingCode } from "./code";
import { DH_LEN, type KeyPair, dh, generateKeyPair } from "./crypto";
import { NoiseError } from "./errors";
import { SymmetricState } from "./symmetric";

export { NoiseError } from "./errors";
export { PAIRING_CODE_INFO } from "./code";
export type { KeyPair } from "./crypto";

export const PAIRING_PATTERN = "Noise_XX_25519_AESGCM_SHA256";
export const SESSION_PATTERN = "Noise_KK_25519_AESGCM_SHA256";
export const PROLOGUE: Uint8Array = utf8.encode("lockra-browser/1");

const TAG = 16;

type Step = "write" | "read";

interface Options {
  /** The extension's static key pair. */
  local: KeyPair;
  /** Where the ephemeral key comes from: WebCrypto's generator, or a fixed key in the tests. */
  ephemeral?: () => Promise<KeyPair>;
}

export class Initiator {
  private e: KeyPair | null = null;
  private re: Uint8Array | null = null;
  private step = 0;
  private failed = false;

  private constructor(
    private readonly pattern: "XX" | "KK",
    private readonly state: SymmetricState,
    private readonly local: KeyPair,
    private rs: Uint8Array | null,
    private readonly makeEphemeral: () => Promise<KeyPair>,
  ) {}

  /** Pairing (XX) under the extension's static key. */
  static async pairing(options: Options): Promise<Initiator> {
    const state = await SymmetricState.of(PAIRING_PATTERN);
    await state.mixHash(PROLOGUE);
    return new Initiator("XX", state, options.local, null, options.ephemeral ?? generateKeyPair);
  }

  /** A session (KK) with the app's static key `remote`, learnt when pairing. */
  static async session(options: Options & { remote: Uint8Array }): Promise<Initiator> {
    if (options.remote.length !== DH_LEN) throw new NoiseError("state");
    const state = await SymmetricState.of(SESSION_PATTERN);
    await state.mixHash(PROLOGUE);
    // The pre-messages: the initiator's static key, then the responder's.
    await state.mixHash(options.local.publicKey);
    await state.mixHash(options.remote);
    return new Initiator(
      "KK",
      state,
      options.local,
      options.remote.slice(),
      options.ephemeral ?? generateKeyPair,
    );
  }

  /** The steps of each pattern, in order. */
  private get steps(): readonly Step[] {
    return this.pattern === "XX" ? ["write", "read", "write"] : ["write", "read"];
  }

  get finished(): boolean {
    return this.step >= this.steps.length;
  }

  /** The app's static key: from the second message on (XX), from the start (KK). */
  get remoteStatic(): Uint8Array | null {
    return this.rs ? this.rs.slice() : null;
  }

  /** The handshake hash so far (the code's input once finished). */
  get hash(): Uint8Array {
    return this.state.hash;
  }

  /** The six digits the app shows too; once the handshake is done. */
  async code(): Promise<string> {
    if (!this.finished) throw new NoiseError("state");
    return pairingCode(this.state.hash);
  }

  /** The next handshake message, carrying `payload`. */
  async write(payload: Uint8Array = new Uint8Array()): Promise<Uint8Array> {
    this.expect("write");
    return this.guard(() => this.writeStep(payload));
  }

  /** Read the app's handshake message; its payload. */
  async read(message: Uint8Array): Promise<Uint8Array> {
    this.expect("read");
    return this.guard(() => this.readStep(message));
  }

  /** A step that fails leaves the hash and keys half-mixed: the handshake is over. (A step asked
   *  out of order is refused before it touches anything.) */
  private async guard<T>(step: () => Promise<T>): Promise<T> {
    try {
      return await step();
    } catch (error) {
      this.failed = true;
      throw error instanceof NoiseError ? error : new NoiseError("decrypt");
    }
  }

  private async writeStep(payload: Uint8Array): Promise<Uint8Array> {
    const state = this.state;
    const parts: Uint8Array[] = [];
    if (this.step === 0) {
      // -> e, then KK's es, ss.
      const e = await this.makeEphemeral();
      this.e = e;
      parts.push(e.publicKey);
      await state.mixHash(e.publicKey);
      if (this.pattern === "KK") {
        const rs = this.need(this.rs);
        await state.mixKey(await dh(e.privateKey, rs));
        await state.mixKey(await dh(this.local.privateKey, rs));
      }
    } else {
      // XX's -> s, se.
      parts.push(await state.encryptAndHash(this.local.publicKey));
      await state.mixKey(await dh(this.local.privateKey, this.need(this.re)));
    }
    parts.push(await state.encryptAndHash(payload));
    this.step++;
    return concat(...parts);
  }

  private async readStep(message: Uint8Array): Promise<Uint8Array> {
    const state = this.state;
    const e = this.need(this.e);
    if (message.length < DH_LEN) throw new NoiseError("short");
    // <- e, ee
    const re = message.slice(0, DH_LEN);
    let rest = message.subarray(DH_LEN);
    await state.mixHash(re);
    await state.mixKey(await dh(e.privateKey, re));
    if (this.pattern === "XX") {
      // s, es
      if (rest.length < DH_LEN + TAG) throw new NoiseError("short");
      const rs = await state.decryptAndHash(rest.subarray(0, DH_LEN + TAG));
      rest = rest.subarray(DH_LEN + TAG);
      this.rs = rs;
      await state.mixKey(await dh(e.privateKey, rs));
    } else {
      // se
      await state.mixKey(await dh(this.local.privateKey, re));
    }
    this.re = re;
    const payload = await state.decryptAndHash(rest);
    this.step++;
    return payload;
  }

  /** The encrypted channel, once the handshake is done. */
  async channel(): Promise<Channel> {
    if (!this.finished) throw new NoiseError("state");
    const [sending, receiving] = await this.state.split();
    return new Channel(sending, receiving);
  }

  private expect(step: Step): void {
    if (this.failed || this.steps[this.step] !== step) throw new NoiseError("state");
  }

  private need<T>(value: T | null): T {
    if (value === null) throw new NoiseError("state");
    return value;
  }
}
