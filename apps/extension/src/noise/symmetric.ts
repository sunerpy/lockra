// The spec's SymmetricState (§5.2): the chaining key, the handshake hash and the cipher that the
// handshake's DH results key.
import { concat, utf8, wipe } from "./bytes";
import { CipherState } from "./channel";
import { HASH_LEN, hkdf2, sha256 } from "./crypto";

export class SymmetricState {
  private cipher = CipherState.empty();

  private constructor(
    private ck: Uint8Array,
    private h: Uint8Array,
  ) {}

  /** InitializeSymmetric: a name of at most 32 bytes padded with zeros, a longer one hashed. */
  static async of(protocolName: string): Promise<SymmetricState> {
    const name = utf8.encode(protocolName);
    let h: Uint8Array;
    if (name.length <= HASH_LEN) {
      h = new Uint8Array(HASH_LEN);
      h.set(name);
    } else {
      h = await sha256(name);
    }
    return new SymmetricState(h.slice(), h);
  }

  get hash(): Uint8Array {
    return this.h.slice();
  }

  async mixHash(data: Uint8Array): Promise<void> {
    this.h = await sha256(concat(this.h, data));
  }

  async mixKey(ikm: Uint8Array): Promise<void> {
    const [ck, key] = await hkdf2(this.ck, ikm);
    wipe(this.ck);
    this.ck = ck;
    this.cipher = await CipherState.of(key);
  }

  async encryptAndHash(plaintext: Uint8Array): Promise<Uint8Array> {
    const ciphertext = await this.cipher.encrypt(this.h, plaintext);
    await this.mixHash(ciphertext);
    return ciphertext;
  }

  async decryptAndHash(ciphertext: Uint8Array): Promise<Uint8Array> {
    const plaintext = await this.cipher.decrypt(this.h, ciphertext);
    await this.mixHash(ciphertext);
    return plaintext;
  }

  /** Split: the initiator's sending cipher, then its receiving one. */
  async split(): Promise<[CipherState, CipherState]> {
    const [first, second] = await hkdf2(this.ck, new Uint8Array());
    wipe(this.ck);
    return [await CipherState.of(first), await CipherState.of(second)];
  }
}
