// The encrypted channel of a finished handshake (docs/formats.md §10): an application message goes as
// at most eight Noise messages of at most 65 535 bytes, each fragment's plaintext a flag byte (1 when
// more follow, 0 on the last) and then its part of the message; at most 512 KiB in all.
import { concat } from "./bytes";
import { Aead } from "./crypto";
import { NoiseError } from "./errors";

/** The longest Noise message. */
export const MAX_NOISE = 65_535;
/** The most fragments of one application message. */
export const MAX_FRAGMENTS = 8;
/** The longest application message. */
export const MAX_MESSAGE = 512 * 1024;
const TAG = 16;
/** The most application bytes one fragment carries: the Noise message less the tag and the flag. */
export const FRAGMENT = MAX_NOISE - TAG - 1;
/** The spec's nonce limit, far beyond any session: 2^64 − 1 is reserved. */
const MAX_NONCE = Number.MAX_SAFE_INTEGER;

/** One direction's key and counter (the spec's CipherState); without a key it passes text through,
 *  as the first message of a handshake does. */
export class CipherState {
  private n = 0;

  private constructor(private readonly aead: Aead | null) {}

  /** A cipher under `key` (32 bytes, wiped once imported). */
  static async of(key: Uint8Array): Promise<CipherState> {
    if (key.length !== 32) throw new NoiseError("state");
    return new CipherState(await Aead.of(key));
  }

  static empty(): CipherState {
    return new CipherState(null);
  }

  get hasKey(): boolean {
    return this.aead !== null;
  }

  async encrypt(ad: Uint8Array, plaintext: Uint8Array): Promise<Uint8Array> {
    if (!this.aead) return plaintext;
    const n = this.next();
    return this.aead.encrypt(n, ad, plaintext);
  }

  async decrypt(ad: Uint8Array, ciphertext: Uint8Array): Promise<Uint8Array> {
    if (!this.aead) return ciphertext;
    if (ciphertext.length < TAG) throw new NoiseError("short");
    // The counter moves only for a message that decrypts, as in snow.
    let plain: Uint8Array;
    try {
      plain = await this.aead.decrypt(this.n, ad, ciphertext);
    } catch {
      throw new NoiseError("decrypt");
    }
    this.next();
    return plain;
  }

  private next(): number {
    if (this.n >= MAX_NONCE) throw new NoiseError("state");
    return this.n++;
  }
}

/** The channel: the extension's sending and receiving ciphers. */
export class Channel {
  private inbox: Uint8Array[] = [];
  private inboxBytes = 0;

  constructor(
    private readonly sending: CipherState,
    private readonly receiving: CipherState,
  ) {}

  /** `message` as the Noise messages to send, in order. */
  async seal(message: Uint8Array): Promise<Uint8Array[]> {
    if (message.length > MAX_MESSAGE) throw new NoiseError("too_long");
    const chunks: Uint8Array[] = [];
    for (let at = 0; at < message.length; at += FRAGMENT)
      chunks.push(message.subarray(at, at + FRAGMENT));
    if (chunks.length === 0) chunks.push(new Uint8Array());
    if (chunks.length > MAX_FRAGMENTS) throw new NoiseError("too_long");
    // Each call takes its nonce as it is made, so the fragments keep their order.
    return Promise.all(
      chunks.map((chunk, index) =>
        this.sending.encrypt(
          new Uint8Array(),
          concat(Uint8Array.of(index === chunks.length - 1 ? 0 : 1), chunk),
        ),
      ),
    );
  }

  /** Take in one Noise message: the application message once its last fragment came, else null. */
  async open(sealed: Uint8Array): Promise<Uint8Array | null> {
    const plain = await this.receiving.decrypt(new Uint8Array(), sealed);
    const flag = plain[0];
    if (flag === undefined || flag > 1) throw new NoiseError("malformed");
    const chunk = plain.subarray(1);
    if (this.inbox.length + 1 > MAX_FRAGMENTS || this.inboxBytes + chunk.length > MAX_MESSAGE) {
      throw new NoiseError("too_long");
    }
    this.inbox.push(chunk);
    this.inboxBytes += chunk.length;
    if (flag === 1) return null;
    const message = concat(...this.inbox);
    this.inbox = [];
    this.inboxBytes = 0;
    return message;
  }
}
