// The frame a fill goes to when no field asked for it (the shortcut, the popup's 填充): plan §五
// 「快捷键对应的 frame」. `probe` runs in every frame the extension may reach
// (scripting.executeScript, all frames; without site access, activeTab reaches the page's own origin
// only, so frames of other sites are left out, as the default frame rule wants), and `chooseFrame`
// picks: the focused frame whose active element is a field, else the focused frame deepest in the
// tree, else, with the popup holding the focus, the field at the end of the page's focus path, else
// the page itself.
import type { FrameDetails, InjectionResult } from "../browser/api";
import { chainOf } from "./frames";

export interface ProbeResult {
  /** The frame's document has the focus. */
  focused: boolean;
  /** Its active element is a field Lockra fills. */
  editable: boolean;
  /** Its active element is a frame: the focus goes on inside it. */
  framed: boolean;
}

/** Runs in the page's frames from its source alone: nothing from outside the function. */
export function probe(): ProbeResult {
  const active = document.activeElement;
  const fillable = ["", "text", "email", "tel", "password", "number", "url", "search"];
  const editable =
    active instanceof HTMLInputElement &&
    !active.disabled &&
    !active.readOnly &&
    fillable.includes(active.getAttribute("type")?.toLowerCase() ?? "");
  const framed = active instanceof HTMLIFrameElement || active?.tagName === "FRAME";
  return { focused: document.hasFocus(), editable, framed };
}

export interface Chosen {
  frameId: number;
  documentId?: string;
}

export function chooseFrame(
  results: readonly InjectionResult<ProbeResult>[],
  frames: readonly FrameDetails[],
): Chosen | null {
  const known = results.filter((r) => chainOf(frames, r.frameId) !== null);
  // How far below the page: its ancestors in the chain, and the page itself for any frame in it.
  const depth = (r: InjectionResult<ProbeResult>) => {
    const ancestors = chainOf(frames, r.frameId)?.length ?? 0;
    return r.frameId === 0 ? 0 : ancestors + 1;
  };
  const deepest = (candidates: InjectionResult<ProbeResult>[]) =>
    candidates.toSorted((a, b) => depth(b) - depth(a) || a.frameId - b.frameId)[0];
  const byId = new Map(known.map((r) => [r.frameId, r]));
  const parentOf = (frameId: number) =>
    frames.find((f) => f.frameId === frameId)?.parentFrameId ?? -1;
  /** Every frame above `frameId` has its focus on the way down to it. */
  const onFocusPath = (frameId: number): boolean => {
    for (let parent = parentOf(frameId); parent !== -1; parent = parentOf(parent)) {
      if (!byId.get(parent)?.result?.framed) return false;
    }
    return true;
  };
  const picked =
    deepest(known.filter((r) => r.result?.focused && r.result.editable)) ??
    deepest(known.filter((r) => r.result?.focused)) ??
    deepest(known.filter((r) => r.result?.editable && onFocusPath(r.frameId))) ??
    known.find((r) => r.frameId === 0);
  if (!picked) return null;
  return picked.documentId === undefined
    ? { frameId: picked.frameId }
    : { frameId: picked.frameId, documentId: picked.documentId };
}
