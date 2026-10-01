import { MockBackend } from "@lockra/shared/mock";
import { fireEvent, renderHook } from "@testing-library/react";
import { ACTIVITY_THROTTLE_MS, useActivityPing } from "./activity";

describe("useActivityPing", () => {
  it("tells the core about input at most every 15 seconds", () => {
    const backend = new MockBackend({ phase: "no_vault" });
    let now = 100_000;
    const { unmount, rerender } = renderHook(
      ({ active }) => useActivityPing(backend, active, () => now),
      { initialProps: { active: true } },
    );
    const pings = () => backend.calls.filter((c) => c.command === "activity").length;
    fireEvent.keyDown(window, { key: "a" });
    fireEvent.pointerDown(window);
    expect(pings()).toBe(1);
    now += ACTIVITY_THROTTLE_MS;
    fireEvent.pointerDown(window);
    expect(pings()).toBe(2);
    rerender({ active: false });
    now += ACTIVITY_THROTTLE_MS;
    fireEvent.keyDown(window, { key: "a" });
    expect(pings()).toBe(2);
    unmount();
  });
});
