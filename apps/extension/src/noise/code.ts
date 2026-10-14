// The six digits both sides show for a pairing handshake: HKDF-SHA256 of the handshake hash (no salt,
// info `lockra pairing code`), its first four bytes as a big-endian number, modulo a million, with
// leading zeros (the app's `pairing_code`).
import { buffer, utf8 } from "./bytes";

export const PAIRING_CODE_INFO: Uint8Array = utf8.encode("lockra pairing code");

export async function pairingCode(handshakeHash: Uint8Array): Promise<string> {
  const subtle = globalThis.crypto.subtle;
  const ikm = await subtle.importKey("raw", buffer(handshakeHash), "HKDF", false, ["deriveBits"]);
  // No salt: HKDF then uses a hash-length string of zeros, the same key to HMAC as an empty one.
  const bits = await subtle.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt: new Uint8Array(), info: buffer(PAIRING_CODE_INFO) },
    ikm,
    32,
  );
  const n = new DataView(bits).getUint32(0, false) % 1_000_000;
  return String(n).padStart(6, "0");
}
