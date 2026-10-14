// The checks a click in the inline frame passes before it counts (plan §四 「防点击劫持」), as pure
// predicates over what the content script measured: jsdom has no layout, so the geometry, the
// styles and what lies on top are given here.
import {
  type ClickFacts,
  MIN_VISIBLE_MS,
  type StyleFacts,
  clickAllowed,
  sameRect,
  styleClean,
} from "./clickjack";

const CLEAN: StyleFacts = {
  opacity: "1",
  filter: "none",
  clipPath: "none",
  mixBlendMode: "normal",
  visibility: "visible",
  display: "block",
  mask: "none",
};

const RECT = { left: 100, top: 200, width: 340, height: 120 };

function facts(change: Partial<ClickFacts> = {}): ClickFacts {
  return {
    trusted: true,
    now: 10_000,
    stableSince: 10_000 - MIN_VISIBLE_MS,
    rect: RECT,
    stableRect: RECT,
    open: true,
    topmost: true,
    styles: [CLEAN, CLEAN, CLEAN],
    ioVisible: true,
    tampered: false,
    ...change,
  };
}

describe("a click in the inline frame", () => {
  it("counts once the frame has shown, unmoved, for 300 ms, on top and untouched", () => {
    expect(clickAllowed(facts())).toEqual({ ok: true });
    // Firefox has no IntersectionObserver v2: unknown is not a no.
    expect(clickAllowed(facts({ ioVisible: null }))).toEqual({ ok: true });
  });

  it("is refused too soon after the frame showed or moved", () => {
    expect(clickAllowed(facts({ stableSince: 10_000 - MIN_VISIBLE_MS + 1 }))).toEqual({
      ok: false,
      reason: "too_soon",
    });
    expect(clickAllowed(facts({ rect: { ...RECT, top: 210 } }))).toEqual({
      ok: false,
      reason: "moved",
    });
    expect(clickAllowed(facts({ rect: { ...RECT, width: 300 } }))).toEqual({
      ok: false,
      reason: "moved",
    });
  });

  it("is refused when something else is on top, or the frame is not open in the top layer", () => {
    expect(clickAllowed(facts({ topmost: false }))).toEqual({ ok: false, reason: "covered" });
    expect(clickAllowed(facts({ open: false }))).toEqual({ ok: false, reason: "covered" });
    expect(clickAllowed(facts({ ioVisible: false }))).toEqual({ ok: false, reason: "occluded" });
  });

  it("is refused when the frame or an ancestor is see-through, filtered, clipped or blended", () => {
    const cases: [Partial<StyleFacts>, string][] = [
      [{ opacity: "0" }, "opacity 0"],
      [{ opacity: "0.99" }, "opacity just under 1"],
      [{ filter: "blur(2px)" }, "a filter"],
      [{ filter: "opacity(0.5)" }, "an opacity filter"],
      [{ clipPath: "inset(0 0 100% 0)" }, "a clip path"],
      [{ mixBlendMode: "multiply" }, "a blend mode"],
      [{ visibility: "hidden" }, "hidden"],
      [{ display: "none" }, "not displayed"],
      [{ mask: "url(#m)" }, "a mask"],
    ];
    const judged = cases.map(([change, name]) => ({
      name,
      clean: styleClean({ ...CLEAN, ...change }),
      // On the frame's host, or on <html> (`html { opacity: 0 }`).
      host: clickAllowed(facts({ styles: [{ ...CLEAN, ...change }, CLEAN] })),
      root: clickAllowed(facts({ styles: [CLEAN, CLEAN, { ...CLEAN, ...change }] })),
    }));
    const refused = { ok: false, reason: "styled" };
    expect(judged).toEqual(
      cases.map(([, name]) => ({ name, clean: false, host: refused, root: refused })),
    );
    expect(styleClean({ ...CLEAN, mask: undefined })).toBe(true);
    expect(styleClean({ ...CLEAN, opacity: "1.0" })).toBe(true);
  });

  it("is refused when the page touched the frame's host, or the click was not the user's", () => {
    expect(clickAllowed(facts({ tampered: true }))).toEqual({ ok: false, reason: "tampered" });
    expect(clickAllowed(facts({ trusted: false }))).toEqual({ ok: false, reason: "untrusted" });
  });
});

describe("the geometry", () => {
  it("allows a pixel of rounding", () => {
    expect(sameRect(RECT, { left: 100.4, top: 199.6, width: 340.5, height: 120 })).toBe(true);
    expect(sameRect(RECT, { ...RECT, left: 102 })).toBe(false);
  });
});
