import { act, renderHook } from "@testing-library/react";
import { DRAG_EVENT_NAME, type DragListen, useFileDrag } from "./drag";

describe("useFileDrag", () => {
  it("follows the shell's enter and leave, and stops listening on unmount", async () => {
    let handler: ((event: { payload: unknown }) => void) | undefined;
    let unlistened = 0;
    const source: DragListen = async (event, h) => {
      expect(event).toBe(DRAG_EVENT_NAME);
      handler = h;
      return () => {
        unlistened += 1;
      };
    };
    const { result, unmount } = renderHook(() => useFileDrag(source));
    await act(async () => undefined);
    expect(result.current).toBe(false);
    act(() => handler?.({ payload: "enter" }));
    expect(result.current).toBe(true);
    act(() => handler?.({ payload: "leave" }));
    expect(result.current).toBe(false);
    unmount();
    expect(unlistened).toBe(1);
  });

  it("unlistens at once when it was unmounted before listening started", async () => {
    let unlistened = 0;
    let resolve: ((fn: () => void) => void) | undefined;
    const source: DragListen = () =>
      new Promise((r) => {
        resolve = r;
      });
    const { unmount } = renderHook(() => useFileDrag(source));
    unmount();
    await act(async () => resolve?.(() => (unlistened += 1)));
    expect(unlistened).toBe(1);
  });

  it("reports nothing outside Tauri", () => {
    const { result } = renderHook(() => useFileDrag(null));
    expect(result.current).toBe(false);
  });
});
