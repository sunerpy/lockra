// Byte helpers for the channel: UTF-8, concatenation, comparison, wiping and base64 (the envelope's
// `d`, standard alphabet with padding, as the app's `data_encoding::BASE64` writes and requires).

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

export const utf8 = {
  encode: (text: string): Uint8Array => encoder.encode(text),
  /** Throws on bytes that are not UTF-8. */
  decode: (bytes: Uint8Array): string => decoder.decode(bytes),
};

export function concat(...parts: readonly Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

export function equal(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= (a[i] ?? 0) ^ (b[i] ?? 0);
  return diff === 0;
}

/** Overwrite a buffer that held key material. A JS heap keeps no promise beyond this one. */
export function wipe(bytes: Uint8Array): void {
  bytes.fill(0);
}

/** The bytes as a fresh ArrayBuffer-backed copy, the type WebCrypto takes. */
export function buffer(bytes: Uint8Array): Uint8Array<ArrayBuffer> {
  const copy = new Uint8Array(new ArrayBuffer(bytes.length));
  copy.set(bytes);
  return copy;
}

const CHUNK = 0x8000;

export function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}

const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

/** The bytes of standard, padded base64; `null` for anything else. */
export function fromBase64(text: string): Uint8Array | null {
  if (!BASE64.test(text)) return null;
  const binary = atob(text);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  // atob ignores non-zero padding bits; the app's decoder does not: a non-canonical encoding is not ours.
  return toBase64(out) === text ? out : null;
}
