// What the background accepts from its own pages and content scripts, checked before use: a
// content script runs in the page's renderer, so its messages are read as carefully as the app's.
import { z } from "zod";
import { copyPartSchema } from "../protocol/schemas";
import type { ContentMessage, InlineToBackground, PopupRequest } from "../shared/messages";

const text = z.string().max(4096);
const token = z.string().regex(/^[0-9a-f]{32}$/);
const point = z.object({ x: z.number().finite(), y: z.number().finite() }).nullable();

const contentSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("content.start"), top: z.boolean() }),
  z.object({
    kind: z.literal("content.focus"),
    form: z.enum(["login", "signup", "username", "otp"]),
    explicit: z.literal(true).optional(),
    signup: z
      .object({
        rules: text.nullable(),
        minlength: z.number().int().min(0).max(4096).nullable(),
        maxlength: z.number().int().min(0).max(4096).nullable(),
        username: text,
      })
      .optional(),
  }),
  z.object({ kind: z.literal("content.otp") }),
  z.object({ kind: z.literal("content.next_step") }),
  z.object({ kind: z.literal("content.page_load") }),
  z.object({
    kind: z.literal("content.submitted"),
    username: text,
    password: text,
    signup: z.boolean(),
  }),
  z.object({ kind: z.literal("content.username"), username: text }),
  z.object({
    kind: z.literal("content.key"),
    token,
    key: z.enum(["ArrowDown", "ArrowUp", "Enter", "Escape"]),
    allowed: z.boolean(),
  }),
  z.object({ kind: z.literal("content.closed"), token }),
]);

const actionSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("choose"), index: z.number().int().min(0).max(1000) }),
  z.object({ type: z.literal("fill") }),
  z.object({ type: z.literal("allow_http") }),
  z.object({ type: z.literal("use") }),
  z.object({ type: z.literal("regenerate") }),
  z.object({ type: z.literal("copy") }),
  z.object({ type: z.literal("save"), title: text, username: text, group: text }),
  z.object({ type: z.literal("update") }),
  z.object({ type: z.literal("dismiss") }),
  z.object({ type: z.literal("never") }),
  z.object({ type: z.literal("close") }),
]);

const inlineSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("hello"), token }),
  z.object({ kind: z.literal("size"), height: z.number().finite() }),
  z.object({ kind: z.literal("visible"), visible: z.boolean().nullable() }),
  z.object({ kind: z.literal("action"), action: actionSchema, point, trusted: z.boolean() }),
]);

const popupSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("hello") }),
  z.object({ kind: z.literal("pair") }),
  z.object({ kind: z.literal("pair.cancel") }),
  z.object({ kind: z.literal("unlock") }),
  z.object({ kind: z.literal("start"), show: z.boolean() }),
  z.object({ kind: z.literal("retry") }),
  z.object({ kind: z.literal("search"), text: z.string().max(256) }),
  z.object({ kind: z.literal("fill"), itemId: z.guid(), allowHttp: z.boolean() }),
  z.object({ kind: z.literal("copy"), itemId: z.guid(), part: copyPartSchema }),
  z.object({ kind: z.literal("access.changed") }),
  z.object({ kind: z.literal("unpair") }),
  z.object({ kind: z.literal("extensions") }),
]);

export function contentMessage(raw: unknown): ContentMessage | null {
  const parsed = contentSchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
}

export function inlineMessage(raw: unknown): InlineToBackground | null {
  const parsed = inlineSchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
}

export function popupMessage(raw: unknown): PopupRequest | null {
  const parsed = popupSchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
}
