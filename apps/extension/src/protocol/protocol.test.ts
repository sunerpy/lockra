// @vitest-environment node
// The extension's protocol against every message the app's types write (tests/vectors/protocol.json):
// what the app sends parses with these schemas, and what the extension builds is the same JSON.
import { vector } from "../test/vectors";
import { utf8 } from "../noise/bytes";
import { decodeEnvelope, encodeEnvelope, encodePlain, readPlain } from "./envelope";
import { clientHello, copyPart, request, startApp, unlockRequest } from "./messages";
import {
  REFUSALS,
  appPlainSchema,
  clientPlainSchema,
  envelopeSchema,
  fillAnswerSchema,
  foundSchema,
  generateAnswerSchema,
  otpAnswerSchema,
  outgoingSchema,
  policySchema,
  pushSchema,
  queryAnswerSchema,
  refusalSchema,
  requestSchema,
  responseSchema,
  savedSchema,
  statusAnswerSchema,
  doneSchema,
} from "./schemas";

interface Fixtures {
  envelopes: Record<string, unknown>;
  client_plain: Record<string, unknown>;
  app_plain: Record<string, unknown>;
  requests: Record<string, unknown>;
  responses: Record<string, { req: number; ok: boolean; result?: unknown; error?: string }>;
  pushes: Record<string, unknown>;
  refusals: string[];
  policy_default: unknown;
}

const f = vector("protocol.json") as Fixtures;

const facts = {
  frame_url: "https://accounts.example.com/login",
  top_url: "https://www.example.com/",
  frame_chain: ["https://www.example.com/frame"],
};
const item = "5c0fe1a2-0b3c-4d5e-8f60-000000000001";

describe("zod", () => {
  it("runs without eval, which the extension's CSP forbids", async () => {
    const { globalConfig } = await import("zod/v4/core");
    expect(globalConfig.jitless).toBe(true);
  });
});

describe("envelopes", () => {
  it("read as the app writes them, and are written the same way", () => {
    expect(
      Object.entries(f.envelopes).filter(([, raw]) => !envelopeSchema.safeParse(raw).success),
    ).toEqual([]);
    expect(encodeEnvelope("hello", utf8.encode('{"op":"unlock_request"}'))).toEqual(
      f.envelopes.hello,
    );
    expect(encodeEnvelope("hs", utf8.encode("handshake"))).toEqual(f.envelopes.handshake);
    expect(
      encodeEnvelope("hs", utf8.encode("handshake"), "5c0fe1a2-0b3c-4d5e-8f60-000000000009"),
    ).toEqual(f.envelopes.session_handshake);
    expect(encodeEnvelope("msg", utf8.encode("sealed"))).toEqual(f.envelopes.message);
    const read = decodeEnvelope(f.envelopes.session_handshake);
    expect(read && utf8.decode(read.bytes)).toBe("handshake");
    expect(read?.pairing).toBe("5c0fe1a2-0b3c-4d5e-8f60-000000000009");
  });

  it("refuse another version, a kind of their own, or a `d` that is not padded base64", () => {
    expect(decodeEnvelope({ v: 2, t: "hello", d: "" })).toBeNull();
    expect(decodeEnvelope({ v: 1, t: "other", d: "" })).toBeNull();
    expect(decodeEnvelope({ v: 1, t: "msg", d: "c2VhbGVk=" })).toBeNull();
    expect(decodeEnvelope({ v: 1, t: "msg", d: "c2VhbGV" })).toBeNull();
    expect(decodeEnvelope("not an object")).toBeNull();
    // `d` may be left out: no bytes.
    expect(decodeEnvelope({ v: 1, t: "hello" })?.bytes).toEqual(new Uint8Array());
  });
});

describe("plain messages", () => {
  it("the extension's are the app's examples", () => {
    const hello = clientHello({
      browser: "chrome",
      label: "Work",
      extension: "abcdefghijklmnopabcdefghijklmnop",
      version: "0.10.0",
      page_access: true,
    });
    expect(hello).toEqual(f.client_plain.hello);
    expect(unlockRequest()).toEqual(f.client_plain.unlock_request);
    for (const raw of Object.values(f.client_plain))
      expect(clientPlainSchema.parse(raw)).toEqual(raw);
    // Host-only, never forwarded to the app: starting Lockra, and bringing it up.
    expect(startApp()).toEqual(f.client_plain.start_app);
    expect(startApp(true)).toEqual(f.client_plain.start_app_show);
    expect(f.client_plain.start_app_show).toEqual({ op: "start_app", show: true });
  });

  it("the app's read, the host's `not_running` too", () => {
    expect(
      Object.entries(f.app_plain).filter(([, raw]) => !appPlainSchema.safeParse(raw).success),
    ).toEqual([]);
    const hello = appPlainSchema.parse(f.app_plain.hello);
    expect(hello).toEqual({
      op: "hello",
      proto_min: 1,
      proto_max: 1,
      proto: 1,
      app_version: "0.10.0",
      state: "unlocked",
    });
    const none = appPlainSchema.parse(f.app_plain.hello_incompatible);
    expect(none.op === "hello" && none.proto).toBeNull();
    expect(appPlainSchema.parse(f.app_plain.state_not_running)).toEqual({
      op: "state",
      state: "not_running",
    });
    expect(appPlainSchema.safeParse({ op: "state", state: "sleeping" }).success).toBe(false);
    expect(appPlainSchema.safeParse({ op: "later_op" }).success).toBe(false);
  });

  it("go through an envelope and back", () => {
    const envelope = encodePlain(unlockRequest());
    expect(envelope).toEqual(f.envelopes.hello);
    expect(
      readPlain(encodeEnvelope("hello", utf8.encode(JSON.stringify(f.app_plain.unlock)))),
    ).toEqual({
      op: "unlock",
      accepted: true,
    });
    // Not JSON, not UTF-8, or a message of a later version: nothing.
    expect(readPlain(encodeEnvelope("hello", utf8.encode("{")))).toBeNull();
    expect(readPlain(encodeEnvelope("hello", Uint8Array.of(0xff)))).toBeNull();
    expect(readPlain(encodeEnvelope("hello", utf8.encode('{"op":"later"}')))).toBeNull();
    expect(
      readPlain(encodeEnvelope("msg", utf8.encode(JSON.stringify(f.app_plain.unlock)))),
    ).toBeNull();
  });
});

describe("requests", () => {
  it("are built exactly as the app's examples", () => {
    const built: Record<string, unknown> = {
      status: request(1, { op: "status" }),
      query: request(2, { op: "query", ...facts, purpose: "fill", username: "", password: "" }),
      query_save: request(3, {
        op: "query",
        ...facts,
        purpose: "save",
        username: "alice",
        password: "s3cret",
      }),
      fill: request(4, {
        op: "fill",
        item_id: item,
        ...facts,
        trigger: "inline_menu",
        allow_http: false,
      }),
      fill_likeliest: request(5, {
        op: "fill",
        item_id: null,
        ...facts,
        trigger: "shortcut",
        allow_http: true,
      }),
      fill_next_step: request(13, {
        op: "fill",
        item_id: item,
        ...facts,
        trigger: "next_step",
        allow_http: false,
      }),
      otp: request(6, { op: "otp", item_id: item, ...facts }),
      otp_asked: request(15, { op: "otp", item_id: item, ...facts, trigger: "popup" }),
      copy: request(7, { op: "copy", item_id: item, part: copyPart("password") }),
      save: request(8, {
        op: "save",
        ...facts,
        title: "",
        username: "alice",
        password: "s3cret",
        group: "Work",
      }),
      update: request(9, { op: "update", item_id: item, ...facts, password: "n3w" }),
      generate: request(10, {
        op: "generate",
        ...facts,
        rules: "minlength: 20; required: lower; required: digit;",
        minlength: 12,
        maxlength: 64,
        save: { username: null, title: "Example" },
      }),
      copy_generated: request(14, {
        op: "copy_generated",
        ...facts,
        password: "q7vx2mz9kd4w8p1r6t3y",
      }),
      site_rule: request(11, { op: "site_rule", kind: "never_save", ...facts }),
      search: request(12, { op: "search", text: "exam", top_url: "https://www.example.com/" }),
      delete: request(16, { op: "delete", item_id: item, ...facts }),
      site_matching: request(17, { op: "site_matching", ...facts, host: true }),
      link_code: request(18, { op: "link_code", item_id: item, ...facts }),
    };
    expect(Object.keys(built).toSorted()).toEqual(Object.keys(f.requests).toSorted());
    for (const [name, raw] of Object.entries(f.requests))
      expect({ name, built: built[name] }).toEqual({ name, built: raw });
    expect(
      Object.entries(f.requests).filter(([, raw]) => !requestSchema.safeParse(raw).success),
    ).toEqual([]);
  });

  it("leave an empty frame chain out, as the app writes it", () => {
    const top = {
      frame_url: "https://example.com/",
      top_url: "https://example.com/",
      frame_chain: [],
    };
    expect(request(1, { op: "otp", item_id: item, ...top })).toEqual({
      req: 1,
      op: "otp",
      item_id: item,
      frame_url: "https://example.com/",
      top_url: "https://example.com/",
    });
    expect(copyPart("field", item)).toEqual({ type: "field", id: item });
  });
});

describe("answers and pushes", () => {
  it("read with the schema of the request they answer", () => {
    const schemas: Record<string, { safeParse: (v: unknown) => { success: boolean } }> = {
      status: statusAnswerSchema,
      query: queryAnswerSchema,
      query_save: queryAnswerSchema,
      fill: fillAnswerSchema,
      otp: otpAnswerSchema,
      saved: savedSchema,
      generated: generateAnswerSchema,
      found: foundSchema,
      done: doneSchema,
    };
    const { refused, ...answered } = f.responses;
    expect(responseSchema.parse(refused)).toEqual({ req: 5, ok: false, error: "http_downgrade" });
    const read = Object.entries(answered).map(([name, raw]) => {
      const response = responseSchema.parse(raw);
      return {
        name,
        req: response.req === raw.req,
        ok: response.ok,
        result: schemas[name]?.safeParse(raw.result).success,
      };
    });
    expect(read).toEqual(
      Object.keys(answered).map((name) => ({ name, req: true, ok: true, result: true })),
    );
    const query = queryAnswerSchema.parse(f.responses.query?.result);
    expect(query.items[0]?.title).toBe("Example");
    expect(query.level).toBe("host");
    const save = queryAnswerSchema.parse(f.responses.query_save?.result);
    expect(save.save).toEqual({ action: "update", item_id: item });
    const fill = fillAnswerSchema.parse(f.responses.fill?.result);
    expect(fill.fields).toEqual([{ page_field: "pin", value: "4242" }]);
  });

  it("pushes and answers are told apart", () => {
    expect(
      Object.values(f.pushes).map((raw) => [
        pushSchema.safeParse(raw).success,
        outgoingSchema.parse(raw).kind,
      ]),
    ).toEqual(Object.values(f.pushes).map(() => [true, "push"]));
    expect(outgoingSchema.parse(f.responses.status).kind).toBe("response");
    expect(outgoingSchema.safeParse({ push: "later" }).success).toBe(false);
    expect(pushSchema.parse(f.pushes.pair_failed)).toEqual({
      push: "pair_failed",
      reason: "expired",
    });
  });

  it("every refusal is known, and the policy's defaults are the app's", () => {
    expect(f.refusals).toEqual([...REFUSALS]);
    for (const refusal of f.refusals) expect(refusalSchema.parse(refusal)).toBe(refusal);
    expect(policySchema.parse(f.policy_default)).toEqual(f.policy_default);
    expect(policySchema.parse({})).toEqual(f.policy_default);
    // A frame rule of a later version is kept as it came; the extension then never fills such frames.
    expect(policySchema.parse({ cross_origin_frames: "later" }).cross_origin_frames).toBe("later");
  });
});
