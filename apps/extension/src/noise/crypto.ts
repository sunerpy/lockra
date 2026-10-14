// The primitives of Noise_*_25519_AESGCM_SHA256, on WebCrypto (Chrome 133+, Firefox 130+ for X25519):
// SHA-256, HMAC-SHA256 and Noise's HKDF, X25519, AES-256-GCM.
import { buffer, concat, wipe } from "./bytes";

const subtle = (): SubtleCrypto => globalThis.crypto.subtle;

/** An X25519 key pair: the private key stays a CryptoKey (non-extractable outside the tests), the
 *  public key is its 32 bytes. */
export interface KeyPair {
  privateKey: CryptoKey;
  publicKey: Uint8Array;
}

export const DH_LEN = 32;
export const HASH_LEN = 32;

export async function sha256(data: Uint8Array): Promise<Uint8Array> {
  return new Uint8Array(await subtle().digest("SHA-256", buffer(data)));
}

export async function hmac(key: Uint8Array, data: Uint8Array): Promise<Uint8Array> {
  const k = await subtle().importKey("raw", buffer(key), { name: "HMAC", hash: "SHA-256" }, false, [
    "sign",
  ]);
  return new Uint8Array(await subtle().sign("HMAC", k, buffer(data)));
}

/** Noise's HKDF with two outputs (the spec's §4.3): the chaining key and the input key material. */
export async function hkdf2(
  chainingKey: Uint8Array,
  ikm: Uint8Array,
): Promise<[Uint8Array, Uint8Array]> {
  const temp = await hmac(chainingKey, ikm);
  const first = await hmac(temp, Uint8Array.of(1));
  const second = await hmac(temp, concat(first, Uint8Array.of(2)));
  wipe(temp);
  return [first, second];
}

/** A new key pair from the browser's generator; the private key cannot be exported. */
export async function generateKeyPair(): Promise<KeyPair> {
  const pair = await subtle().generateKey({ name: "X25519" }, false, ["deriveBits"]);
  if (!("privateKey" in pair)) throw new TypeError("X25519 made no key pair");
  const publicKey = new Uint8Array(await subtle().exportKey("raw", pair.publicKey));
  return { privateKey: pair.privateKey, publicKey };
}

/** X25519 of our private key and their public key. */
export async function dh(privateKey: CryptoKey, publicKey: Uint8Array): Promise<Uint8Array> {
  const theirs = await subtle().importKey("raw", buffer(publicKey), { name: "X25519" }, true, []);
  return new Uint8Array(
    await subtle().deriveBits({ name: "X25519", public: theirs }, privateKey, 256),
  );
}

/** AES-256-GCM under a 32-byte key, the nonce 32 zero bits then the counter big-endian (the spec's
 *  AESGCM functions). The raw key is wiped once imported. */
export class Aead {
  private constructor(private readonly key: CryptoKey) {}

  static async of(raw: Uint8Array): Promise<Aead> {
    const key = await subtle().importKey("raw", buffer(raw), { name: "AES-GCM" }, false, [
      "encrypt",
      "decrypt",
    ]);
    wipe(raw);
    return new Aead(key);
  }

  async encrypt(n: number, ad: Uint8Array, plaintext: Uint8Array): Promise<Uint8Array> {
    const sealed = await subtle().encrypt(
      { name: "AES-GCM", iv: nonce(n), additionalData: buffer(ad), tagLength: 128 },
      this.key,
      buffer(plaintext),
    );
    return new Uint8Array(sealed);
  }

  /** Throws (WebCrypto's OperationError) when the tag does not match. */
  async decrypt(n: number, ad: Uint8Array, ciphertext: Uint8Array): Promise<Uint8Array> {
    const plain = await subtle().decrypt(
      { name: "AES-GCM", iv: nonce(n), additionalData: buffer(ad), tagLength: 128 },
      this.key,
      buffer(ciphertext),
    );
    return new Uint8Array(plain);
  }
}

function nonce(n: number): Uint8Array<ArrayBuffer> {
  const iv = new Uint8Array(12);
  new DataView(iv.buffer).setBigUint64(4, BigInt(n), false);
  return iv;
}
