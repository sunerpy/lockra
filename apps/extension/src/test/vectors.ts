// The channel's contract vectors, written by crates/lockra-browser/tests/protocol.rs (snow and the
// Rust types) and compared byte for byte there: the extension is checked against the same files.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const VECTORS = resolve(import.meta.dirname, "../../../../crates/lockra-browser/tests/vectors");

export function vector(name: "noise.json" | "protocol.json"): unknown {
  return JSON.parse(readFileSync(resolve(VECTORS, name), "utf8"));
}

export interface NoiseVectors {
  prologue: string;
  pairing_code_info: string;
  pairing_code_of_zero_hash: string;
  app_static: { private: string; public: string };
  extension_static: { private: string; public: string };
  pairing: {
    pattern: string;
    initiator_ephemeral: string;
    responder_ephemeral: string;
    messages: [string, string, string];
    handshake_hash: string;
    code: string;
    responder_first_message_plain: string;
    responder_first_message: string;
    initiator_first_message_plain: string;
    initiator_first_message: string;
  };
  session: {
    pattern: string;
    initiator_ephemeral: string;
    responder_ephemeral: string;
    messages: [string, string];
    handshake_hash: string;
    responder_first_message_plain: string;
    responder_first_message: string;
    initiator_messages_plain: string;
    initiator_messages: [string, string];
  };
}

export function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function unhex(text: string): Uint8Array {
  const out = new Uint8Array(text.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = Number.parseInt(text.slice(2 * i, 2 * i + 2), 16);
  return out;
}

/** A fixed X25519 private key as WebCrypto holds one (PKCS #8), with its public key: the vectors'
 *  keys, which the real extension never imports (it generates its own, non-extractable). */
export async function fixedKeyPair(
  privateHex: string,
): Promise<{ privateKey: CryptoKey; publicKey: Uint8Array }> {
  const prefix = unhex("302e020100300506032b656e04220420");
  const pkcs8 = new Uint8Array(prefix.length + 32);
  pkcs8.set(prefix);
  pkcs8.set(unhex(privateHex), prefix.length);
  const privateKey = await crypto.subtle.importKey("pkcs8", pkcs8, { name: "X25519" }, true, [
    "deriveBits",
  ]);
  const jwk = await crypto.subtle.exportKey("jwk", privateKey);
  const publicKey = new Uint8Array(Buffer.from(jwk.x ?? "", "base64url"));
  return { privateKey, publicKey };
}
