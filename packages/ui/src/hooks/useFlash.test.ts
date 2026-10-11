import { act, renderHook } from "@testing-library/react";
import { FLASH_MS, useFlash } from "./useFlash";

describe("useFlash", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("marks one item for a moment, and a new flash starts the time again", () => {
    const { result, unmount } = renderHook(() => useFlash());
    expect(result.current[0]).toBeUndefined();
    act(() => result.current[1]("a"));
    expect(result.current[0]).toBe("a");
    act(() => {
      vi.advanceTimersByTime(FLASH_MS - 600);
    });
    act(() => result.current[1]("a"));
    act(() => {
      vi.advanceTimersByTime(FLASH_MS - 1);
    });
    expect(result.current[0]).toBe("a");
    act(() => result.current[1]("b"));
    expect(result.current[0]).toBe("b");
    act(() => {
      vi.advanceTimersByTime(FLASH_MS);
    });
    expect(result.current[0]).toBeUndefined();
    act(() => result.current[1]("c"));
    unmount();
    // The timer went with the component.
    expect(vi.getTimerCount()).toBe(0);
  });
});
