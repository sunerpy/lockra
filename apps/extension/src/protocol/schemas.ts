// The channel's messages (docs/formats.md §10) as zod schemas: the app's types in
// crates/lockra-browser/src/protocol.rs and crates/lockra-core/src/browser.rs are the source of truth,
// and protocol.test.ts reads every example they write (tests/vectors/protocol.json) with these.
// Objects keep the fields they know and drop the rest, as serde does, so a later app's additions do
// not break an older extension; a message of an op or push this version does not know fails to parse
// and is ignored.
import { z } from "zod";

// No `new Function`: the extension's CSP forbids eval, and zod's probe for it would be reported as
// a CSP violation even though zod catches it.
z.config({ jitless: true });

export const ENVELOPE_VERSION = 1;
/** The protocol versions this extension speaks. */
export const PROTO_MIN = 1;
export const PROTO_MAX = 1;

const id = z.guid();
const count = z.number().int().nonnegative();

// ---- envelopes ----------------------------------------------------------------------------------

export const KINDS = ["hello", "hs", "msg"] as const;
export type Kind = (typeof KINDS)[number];

/** `{"v":1,"t":"hello"|"hs"|"msg","d":"<base64>"}`, with `p` (the pairing) on a session's first
 *  handshake message. */
export const envelopeSchema = z.object({
  v: z.literal(ENVELOPE_VERSION),
  t: z.enum(KINDS),
  d: z.string().default(""),
  p: id.optional(),
});
export type Envelope = z.infer<typeof envelopeSchema>;

// ---- before a handshake ---------------------------------------------------------------------------

/** What the extension says of itself: shown in Lockra, no decision rests on it. */
export const clientInfoSchema = z.object({
  browser: z.string(),
  label: z.string(),
  extension: z.string(),
  version: z.string(),
  page_access: z.boolean(),
});
export type ClientInfo = z.infer<typeof clientInfoSchema>;

export const clientPlainSchema = z.discriminatedUnion("op", [
  z.object({
    op: z.literal("hello"),
    proto_min: count,
    proto_max: count,
    ...clientInfoSchema.shape,
  }),
  z.object({ op: z.literal("unlock_request") }),
  // Host-only (PR 9's host handles it and never forwards it): start Lockra, and with `show` bring
  // its window up.
  z.object({ op: z.literal("start_app"), show: z.boolean().optional() }),
]);
export type ClientPlain = z.infer<typeof clientPlainSchema>;

export const APP_STATES = ["no_vault", "locked", "unlocked"] as const;
export type AppState = (typeof APP_STATES)[number];
/** Host-only: the host cannot reach the app's socket (Lockra is not running). */
export const NOT_RUNNING = "not_running";

export const appPlainSchema = z.discriminatedUnion("op", [
  z.object({
    op: z.literal("hello"),
    proto_min: count,
    proto_max: count,
    /** The newest version both speak; null when they share none. */
    proto: count.nullable(),
    app_version: z.string(),
    state: z.enum(APP_STATES),
  }),
  z.object({ op: z.literal("state"), state: z.enum([...APP_STATES, NOT_RUNNING]) }),
  z.object({ op: z.literal("unlock"), accepted: z.boolean() }),
  /** `hello_first`, `version`, `bad_message`, or a refusal (`locked`, `not_paired`, `busy`,
   *  `unavailable`). */
  z.object({ op: z.literal("error"), error: z.string() }),
]);
export type AppPlain = z.infer<typeof appPlainSchema>;

// ---- requests -------------------------------------------------------------------------------------

/** Where a request comes from, as the background took it from the browser (never from the page). */
export const frameFactsSchema = z.object({
  frame_url: z.string(),
  top_url: z.string(),
  /** The frame's ancestors, the nearest first, without the page itself; left out when empty. */
  frame_chain: z.array(z.string()).optional(),
});
export type FrameFacts = z.infer<typeof frameFactsSchema>;

export const TRIGGERS = ["shortcut", "popup", "inline_menu", "page_load", "next_step"] as const;
export type Trigger = (typeof TRIGGERS)[number];

export const copyPartSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("username") }),
  z.object({ type: z.literal("email") }),
  z.object({ type: z.literal("password") }),
  z.object({ type: z.literal("totp") }),
  z.object({ type: z.literal("field"), id }),
]);
export type CopyPart = z.infer<typeof copyPartSchema>;

const facts = frameFactsSchema.shape;
const nullableString = z.string().nullable();

export const opSchema = z.discriminatedUnion("op", [
  z.object({ op: z.literal("status") }),
  z.object({
    op: z.literal("query"),
    ...facts,
    purpose: z.enum(["fill", "save"]),
    username: z.string(),
    password: z.string(),
  }),
  z.object({
    op: z.literal("fill"),
    item_id: id.nullable(),
    ...facts,
    trigger: z.enum(TRIGGERS),
    allow_http: z.boolean(),
  }),
  z.object({ op: z.literal("otp"), item_id: id, ...facts }),
  z.object({ op: z.literal("copy"), item_id: id, part: copyPartSchema }),
  z.object({
    op: z.literal("save"),
    ...facts,
    title: z.string(),
    username: z.string(),
    password: z.string(),
    group: nullableString,
  }),
  z.object({ op: z.literal("update"), item_id: id, ...facts, password: z.string() }),
  z.object({
    op: z.literal("generate"),
    ...facts,
    rules: nullableString,
    minlength: count.nullable(),
    maxlength: count.nullable(),
    save: z.object({ username: nullableString, title: z.string() }).nullable(),
  }),
  /** The sign-up menu's copy: the app copies only a password it generated for this site. */
  z.object({ op: z.literal("copy_generated"), ...facts, password: z.string() }),
  z.object({ op: z.literal("site_rule"), kind: z.enum(["never_save", "never_fill"]), ...facts }),
  z.object({ op: z.literal("search"), text: z.string(), top_url: z.string() }),
]);
export type Op = z.infer<typeof opSchema>;
export type OpName = Op["op"];

export const requestSchema = z.intersection(z.object({ req: count }), opSchema);
export type Request = z.infer<typeof requestSchema>;

// ---- answers --------------------------------------------------------------------------------------

/** Why a request gets nothing; the extension words each for the user (i18n `extension.refusal`). */
export const REFUSALS = [
  "unavailable",
  "locked",
  "not_paired",
  "paused",
  "busy",
  "invalid",
  "never_fill",
  "never_save",
  "no_match",
  "cross_origin",
  "http_downgrade",
  "protected",
  "denied",
  "expired",
  "rate_limited",
  "not_found",
] as const;
export type Refusal = (typeof REFUSALS)[number];
export const refusalSchema = z.enum(REFUSALS);

/** What the extensions may do (`local.browser.policy`): the app's choice, reported. A frame rule of
 *  a later version is kept as it came and counts as `never`. */
export const policySchema = z.object({
  fill_otp: z.boolean().default(true),
  copy_otp: z.boolean().default(true),
  auto_submit: z.boolean().default(false),
  confirm_fill: z.boolean().default(false),
  cross_origin_frames: z.string().default("never"),
  on_page_load: z.boolean().default(false),
  /** Lockra matches the port too (its decision; an older Lockra sends none). */
  match_port: z.boolean().default(false),
});
export type Policy = z.infer<typeof policySchema>;

export const PROTECTIONS = ["fill", "confirm", "no_extension"] as const;

/** A login as the extension lists it: no secret. */
export const itemSummarySchema = z.object({
  item_id: id,
  title: z.string(),
  username: z.string(),
  email: z.string(),
  has_password: z.boolean(),
  has_totp: z.boolean(),
  /** A level of a later version is kept; anything but `fill` waits for Lockra or is refused there. */
  protection: z.string(),
  favorite: z.boolean(),
  /** Its matching URL is `https` while the page is `http`: the user agrees in the page first. */
  downgrade: z.boolean(),
  /** It matches the frame (a search also finds logins of other sites, to copy only). */
  matches: z.boolean(),
});
export type ItemSummary = z.infer<typeof itemSummarySchema>;

export const statusAnswerSchema = z.object({
  pairing: id,
  app_version: z.string(),
  policy: policySchema,
  items: count,
  paused: z.boolean(),
});
export type StatusAnswer = z.infer<typeof statusAnswerSchema>;

export const MATCH_LEVELS = ["domain", "host", "pattern", "exact"] as const;

export const queryAnswerSchema = z.object({
  site: z.string(),
  level: z.string().nullable(),
  items: z.array(itemSummarySchema),
  never_fill: z.boolean(),
  never_save: z.boolean(),
  cross_origin: z.boolean(),
  policy: policySchema,
  save: z.object({ action: z.enum(["none", "save", "update"]), item_id: id.nullable() }).nullable(),
});
export type QueryAnswer = z.infer<typeof queryAnswerSchema>;

export const fillAnswerSchema = z.object({
  item_id: id,
  username: z.string(),
  password: z.string(),
  fields: z.array(z.object({ page_field: z.string(), value: z.string() })),
  otp: nullableString,
  submit: z.boolean(),
});
export type FillAnswer = z.infer<typeof fillAnswerSchema>;

/** `submit`: send the code's form once it is in (an app before 0.10 does not say: not sent). */
export const otpAnswerSchema = z.object({
  code: z.string(),
  copied: z.boolean(),
  submit: z.boolean().default(false),
});
export type OtpAnswer = z.infer<typeof otpAnswerSchema>;

export const foundSchema = z.object({ items: z.array(itemSummarySchema) });
export const savedSchema = z.object({ item_id: id });
export const generateAnswerSchema = z.object({
  password: z.string(),
  username: z.string(),
  item_id: id.nullable(),
});
export type GenerateAnswer = z.infer<typeof generateAnswerSchema>;
export const doneSchema = z.object({});

/** Which answer each op gets. */
export const ANSWER_SCHEMAS = {
  status: statusAnswerSchema,
  query: queryAnswerSchema,
  fill: fillAnswerSchema,
  otp: otpAnswerSchema,
  copy: doneSchema,
  save: savedSchema,
  update: doneSchema,
  generate: generateAnswerSchema,
  copy_generated: doneSchema,
  site_rule: doneSchema,
  search: foundSchema,
} as const satisfies Record<OpName, z.ZodType>;
export type AnswerOf<O extends OpName> = z.infer<(typeof ANSWER_SCHEMAS)[O]>;

/** `{"req":1,"ok":true,"result":…}` or `{"req":1,"ok":false,"error":"no_match"}`; a refusal of a
 *  later version keeps its name. */
export const responseSchema = z.discriminatedUnion("ok", [
  z.object({ req: count, ok: z.literal(true), result: z.unknown() }),
  z.object({ req: count, ok: z.literal(false), error: z.string() }),
]);
export type Response = z.infer<typeof responseSchema>;

/** What the app tells over a channel by itself. */
export const pushSchema = z.discriminatedUnion("push", [
  z.object({ push: z.literal("pair_pending"), expires_in_ms: count }),
  z.object({ push: z.literal("paired"), pairing: id }),
  z.object({ push: z.literal("pair_failed"), reason: z.string() }),
  z.object({ push: z.literal("ready"), pairing: id }),
  z.object({ push: z.literal("revoked") }),
]);
export type Push = z.infer<typeof pushSchema>;

/** A message over a channel: an answer or a push. */
export const outgoingSchema = z.union([
  pushSchema.transform((push) => ({ kind: "push" as const, push })),
  responseSchema.transform((response) => ({ kind: "response" as const, response })),
]);
export type Outgoing = z.infer<typeof outgoingSchema>;
