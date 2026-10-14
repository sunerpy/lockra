// The messages the extension writes, shaped as the app's types serialize them (protocol.test.ts
// compares them with the app's examples).
import {
  type ClientInfo,
  type ClientPlain,
  type CopyPart,
  type Op,
  PROTO_MAX,
  PROTO_MIN,
  type Request,
} from "./schemas";

/** The first message of a connection: the versions this extension speaks and what it is. */
export function clientHello(client: ClientInfo): ClientPlain {
  return { op: "hello", proto_min: PROTO_MIN, proto_max: PROTO_MAX, ...client };
}

/** Ask Lockra to come up with its unlock screen (at most every ten seconds; the app decides). */
export function unlockRequest(): ClientPlain {
  return { op: "unlock_request" };
}

/** For the host only: start Lockra; with `show`, bring its window up too. */
export function startApp(show = false): ClientPlain {
  return show ? { op: "start_app", show: true } : { op: "start_app" };
}

/** Request `req`: `{"req": n, "op": …}`, an empty frame chain left out as the app writes it. */
export function request(req: number, op: Op): Request {
  if ("frame_chain" in op && op.frame_chain !== undefined && op.frame_chain.length === 0) {
    const { frame_chain: _, ...rest } = op;
    return { req, ...rest };
  }
  return { req, ...op };
}

export function copyPart(type: "username" | "email" | "password" | "totp"): CopyPart;
export function copyPart(type: "field", id: string): CopyPart;
export function copyPart(type: CopyPart["type"], id?: string): CopyPart {
  return type === "field" ? { type, id: id ?? "" } : { type };
}
