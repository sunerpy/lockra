import { act, renderHook } from "@testing-library/react";
import { NOW_REFRESH_MS, useNow } from "./useNow";

describe("useNow", () => {
  // Regression (2026-10-11): it returned seconds, and every caller hands it to `relativeTime` or
  // `syncStatusLine` as milliseconds, so 上次备份 and the sync times always read 「刚刚」.
  it("returns the time in milliseconds, whole seconds, and refreshes on the shared interval", () => {
    vi.useFakeTimers();
    // A date the real clock has not reached yet, so the module-level cache (taken from the real
    // clock at import) can never be ahead of the faked time; the first subscriber refreshes it.
    const start = new Date("2036-09-24T10:00:00Z");
    vi.setSystemTime(start);
    const first = renderHook(() => useNow());
    const second = renderHook(() => useNow());
    const initial = first.result.current;
    expect(initial).toBe(start.getTime());
    expect(second.result.current).toBe(initial);
    act(() => {
      vi.setSystemTime(new Date("2036-09-24T10:05:00.750Z"));
      vi.advanceTimersByTime(NOW_REFRESH_MS);
    });
    expect(first.result.current).toBe(Math.floor(Date.now() / 1000) * 1000);
    expect(first.result.current - initial).toBeGreaterThanOrEqual(5 * 60_000);
    first.unmount();
    second.unmount();
    act(() => {
      vi.advanceTimersByTime(NOW_REFRESH_MS);
    });
    vi.useRealTimers();
  });
});
