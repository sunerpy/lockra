// The frame a shortcut (or the popup) fills: a probe in every frame says whether its document has
// the focus and whether its active element is a field; the background picks from what came back.
import type { FrameDetails, InjectionResult } from "../browser/api";
import { type ProbeResult, chooseFrame, probe } from "./shortcut";

const FRAMES: FrameDetails[] = [
  { frameId: 0, parentFrameId: -1, url: "https://www.example.com/" },
  { frameId: 1, parentFrameId: 0, url: "https://www.example.com/a" },
  { frameId: 2, parentFrameId: 1, url: "https://www.example.com/b" },
  { frameId: 3, parentFrameId: 0, url: "https://ads.example/" },
];

function results(probes: Record<number, Partial<ProbeResult>>): InjectionResult<ProbeResult>[] {
  return Object.entries(probes).map(([frameId, result]) => ({
    frameId: Number(frameId),
    documentId: `doc-${frameId}`,
    result: { focused: false, editable: false, framed: false, ...result },
  }));
}

describe("picking the frame", () => {
  it("takes the focused frame whose active element is a field, the deepest first", () => {
    const picked = chooseFrame(
      results({
        0: { focused: true, framed: true },
        1: { focused: true, editable: true },
        2: { focused: true, editable: true },
        3: {},
      }),
      FRAMES,
    );
    expect(picked).toEqual({ frameId: 2, documentId: "doc-2" });
  });

  it("else the focused frame with the longest chain", () => {
    expect(
      chooseFrame(results({ 0: { focused: true }, 1: { focused: true }, 3: {} }), FRAMES),
    ).toEqual({ frameId: 1, documentId: "doc-1" });
  });

  it("with the popup open (no document has the focus), the field at the end of the focus path", () => {
    // The page's active element is frame 1, whose active element is frame 2, which holds the field;
    // frame 3 also has a field active, but the focus is not on its path.
    const picked = chooseFrame(
      results({
        0: { framed: true },
        1: { framed: true },
        2: { editable: true },
        3: { editable: true },
      }),
      FRAMES,
    );
    expect(picked).toEqual({ frameId: 2, documentId: "doc-2" });
    expect(chooseFrame(results({ 0: { editable: true }, 3: { editable: true } }), FRAMES)).toEqual({
      frameId: 0,
      documentId: "doc-0",
    });
  });

  it("else the page itself; nothing when even the page could not be probed", () => {
    expect(chooseFrame(results({ 0: {}, 1: {} }), FRAMES)).toEqual({
      frameId: 0,
      documentId: "doc-0",
    });
    expect(chooseFrame(results({ 3: {} }), FRAMES)).toBeNull();
    expect(chooseFrame([{ frameId: 0 }], FRAMES)).toEqual({ frameId: 0 });
  });
});

describe("the probe", () => {
  it("reports the document's focus and its active element", () => {
    document.body.innerHTML = `<input id="user"><textarea id="note"></textarea><iframe id="frame"></iframe><input id="off" disabled><input id="box" type="checkbox">`;
    const focus = (id: string) => (document.getElementById(id) as HTMLElement).focus();
    focus("user");
    expect(probe()).toMatchObject({ editable: true, framed: false });
    focus("note");
    expect(probe()).toMatchObject({ editable: false });
    focus("box");
    expect(probe()).toMatchObject({ editable: false });
    focus("frame");
    expect(probe()).toMatchObject({ editable: false, framed: true });
    expect(typeof probe().focused).toBe("boolean");
  });

  it("is self-contained: it runs in the page's frames from its source alone", () => {
    const source = probe.toString();
    // eslint-disable-next-line typescript/no-implied-eval -- the injection serializes the function.
    const rebuilt = new Function(`return (${source})`)() as typeof probe;
    document.body.innerHTML = `<input id="user" type="email">`;
    (document.getElementById("user") as HTMLElement).focus();
    expect(rebuilt()).toMatchObject({ editable: true });
  });
});
