// Envelopes and the plain messages they carry before a handshake. An envelope that does not read
// (another version, an unknown kind, a `d` that is not the app's base64) ends the connection, as on
// the app's side; a plain message this version does not know is ignored.
import { fromBase64, toBase64, utf8 } from "../noise/bytes";
import {
  type AppPlain,
  type ClientPlain,
  ENVELOPE_VERSION,
  type Envelope,
  type Kind,
  appPlainSchema,
  envelopeSchema,
} from "./schemas";

export function encodeEnvelope(t: Kind, bytes: Uint8Array, pairing?: string): Envelope {
  const envelope: Envelope = { v: ENVELOPE_VERSION, t, d: toBase64(bytes) };
  if (pairing !== undefined) envelope.p = pairing;
  return envelope;
}

export interface ReadEnvelope {
  t: Kind;
  bytes: Uint8Array;
  pairing?: string;
}

/** An envelope as it came from the host; `null` when it is not one. */
export function decodeEnvelope(raw: unknown): ReadEnvelope | null {
  const parsed = envelopeSchema.safeParse(raw);
  if (!parsed.success) return null;
  const bytes = fromBase64(parsed.data.d);
  if (!bytes) return null;
  return parsed.data.p === undefined
    ? { t: parsed.data.t, bytes }
    : { t: parsed.data.t, bytes, pairing: parsed.data.p };
}

/** A plain message in its envelope. */
export function encodePlain(message: ClientPlain): Envelope {
  return encodeEnvelope("hello", utf8.encode(JSON.stringify(message)));
}

/** The app's (or the host's) plain message in an envelope; `null` when there is none this version
 *  reads. */
export function readPlain(raw: unknown): AppPlain | null {
  const envelope = decodeEnvelope(raw);
  if (!envelope || envelope.t !== "hello") return null;
  const json = parseJson(envelope.bytes);
  if (json === undefined) return null;
  const parsed = appPlainSchema.safeParse(json);
  return parsed.success ? parsed.data : null;
}

/** JSON from bytes; `undefined` when they are not UTF-8 JSON. */
export function parseJson(bytes: Uint8Array): unknown {
  try {
    return JSON.parse(utf8.decode(bytes));
  } catch {
    return undefined;
  }
}
