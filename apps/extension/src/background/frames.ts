// The frame facts a request carries (plan §五 「可信的 frame 信息」): taken from the browser, never
// from the page. The content script reports no address at all.
// - frame_url: the sender's URL (Chromium also checks it against the sender's origin, Firefox from
//   126 too); only http and https documents, so about:blank, about:srcdoc, data: and blob: frames
//   get nothing;
// - top_url: the tab's URL (readable after a user action through activeTab, or with site access);
// - frame_chain: webNavigation.getAllFrames, from the frame up its parents to the page; a broken
//   chain is refused, and so is a frame whose document (Chromium 106+, Firefox 153+) or, without a
//   documentId, whose URL is no longer the sender's: it navigated meanwhile.
// The app decides everything else on these facts.
import type { FrameDetails, MessageSender } from "../browser/api";
import type { FrameFacts } from "../protocol/schemas";

export type FramesOf = (tabId: number) => Promise<FrameDetails[] | null>;

export interface TrustedFrame {
  tabId: number;
  frameId: number;
  documentId?: string;
  facts: FrameFacts & { frame_chain: string[] };
}

export type FrameRefusal = "no_tab" | "not_web" | "origin" | "frames" | "chain" | "navigated";

/** The address when it is an http or https document's; null otherwise. */
export function webUrl(url: string | undefined): string | null {
  if (!url) return null;
  try {
    const parsed = new URL(url);
    return parsed.protocol === "https:" || parsed.protocol === "http:" ? url : null;
  } catch {
    return null;
  }
}

/** The most frames a chain may climb: the app refuses more than 32 ancestors anyway. */
const MAX_DEPTH = 64;

/** The frame's ancestors, the nearest first, up to (and without) the page; null when the tree does
 *  not lead from the frame to the page. */
export function chainOf(frames: readonly FrameDetails[], frameId: number): string[] | null {
  const byId = new Map(frames.map((frame) => [frame.frameId, frame]));
  let frame = byId.get(frameId);
  if (!frame) return null;
  const chain: string[] = [];
  for (let depth = 0; frame.parentFrameId !== -1; depth++) {
    const parent = byId.get(frame.parentFrameId);
    if (!parent || depth >= MAX_DEPTH) return null;
    if (parent.parentFrameId !== -1) chain.push(parent.url);
    frame = parent;
  }
  return chain;
}

async function tree(framesOf: FramesOf, tabId: number): Promise<FrameDetails[] | null> {
  try {
    return await framesOf(tabId);
  } catch {
    return null;
  }
}

/** The facts of a content script's message. */
export async function factsFromSender(
  sender: MessageSender,
  framesOf: FramesOf,
): Promise<TrustedFrame | { error: FrameRefusal }> {
  const tabId = sender.tab?.id;
  const frameId = sender.frameId;
  if (tabId === undefined || frameId === undefined) return { error: "no_tab" };
  const frameUrl = webUrl(sender.url);
  const topUrl = webUrl(sender.tab?.url);
  if (!frameUrl || !topUrl) return { error: "not_web" };
  if (sender.origin !== undefined && sender.origin !== new URL(frameUrl).origin)
    return { error: "origin" };
  const frames = await tree(framesOf, tabId);
  if (!frames) return { error: "frames" };
  const frame = frames.find((f) => f.frameId === frameId);
  const chain = chainOf(frames, frameId);
  if (!frame || !chain) return { error: "chain" };
  const sameDocument =
    sender.documentId !== undefined && frame.documentId !== undefined
      ? sender.documentId === frame.documentId
      : frame.url === sender.url;
  if (!sameDocument) return { error: "navigated" };
  const trusted: TrustedFrame = {
    tabId,
    frameId,
    facts: { frame_url: frameUrl, top_url: topUrl, frame_chain: chain },
  };
  if (sender.documentId !== undefined) trusted.documentId = sender.documentId;
  return trusted;
}

/** What is known of a frame the background picked: an injection's result, or a menu's frame. */
export interface PickedFrame {
  tabId: number;
  frameId: number;
  /** The document it was in (Chromium): it must still be the frame's. */
  documentId?: string;
  /** Without a documentId: the URL it had, which must still be the frame's. */
  url?: string;
  tabUrl: string | undefined;
}

/** The facts of a frame the background picked, its URL from the frame tree. */
export async function factsForFrame(
  picked: PickedFrame,
  framesOf: FramesOf,
): Promise<TrustedFrame | { error: FrameRefusal }> {
  const topUrl = webUrl(picked.tabUrl);
  if (!topUrl) return { error: "not_web" };
  const frames = await tree(framesOf, picked.tabId);
  if (!frames) return { error: "frames" };
  const frame = frames.find((f) => f.frameId === picked.frameId);
  const chain = chainOf(frames, picked.frameId);
  if (!frame || !chain) return { error: "chain" };
  if (
    picked.documentId !== undefined &&
    frame.documentId !== undefined &&
    picked.documentId !== frame.documentId
  ) {
    return { error: "navigated" };
  }
  if (picked.url !== undefined && picked.url !== frame.url) return { error: "navigated" };
  const frameUrl = webUrl(frame.url);
  if (!frameUrl) return { error: "not_web" };
  const trusted: TrustedFrame = {
    tabId: picked.tabId,
    frameId: picked.frameId,
    facts: { frame_url: frameUrl, top_url: topUrl, frame_chain: chain },
  };
  const documentId = frame.documentId ?? picked.documentId;
  if (documentId !== undefined) trusted.documentId = documentId;
  return trusted;
}
