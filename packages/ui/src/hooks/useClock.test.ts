import { act, renderHook } from "@testing-library/react";
import { clockRunning, useClock } from "./useClock";

describe("useClock", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("ticks on whole seconds while mounted and stops after", () => {
    vi.useFakeTimers({ now: 1_790_000_000_400 });
    const { result, unmount } = renderHook(() => useClock());
    const first = result.current;
    expect(first).toBe(1_790_000_000_400);
    act(() => {
      vi.advanceTimersByTime(599);
    });
    expect(result.current).toBe(first);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current).toBe(1_790_000_001_000);
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(result.current).toBe(1_790_000_004_000);
    expect(clockRunning()).toBe(true);
    unmount();
    expect(clockRunning()).toBe(false);
  });

  it("two listeners share one timer", () => {
    vi.useFakeTimers({ now: 5_000 });
    const a = renderHook(() => useClock());
    const b = renderHook(() => useClock());
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(a.result.current).toBe(b.result.current);
    a.unmount();
    expect(clockRunning()).toBe(true);
    b.unmount();
    expect(clockRunning()).toBe(false);
  });
});
