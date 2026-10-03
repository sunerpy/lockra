import { act, cleanup, render } from "@testing-library/react";
import { StrictMode } from "react";
import { type Presence, noteUserLock, useUnlockPrompt } from "./unlock-prompt";

/** A presence the test moves: in front or not, telling the subscribers each time. */
function fakePresence(initially: boolean) {
  let present = initially;
  const listeners = new Set<() => void>();
  const presence: Presence = {
    present: () => present,
    subscribe: (onChange) => {
      listeners.add(onChange);
      return () => listeners.delete(onChange);
    },
  };
  const set = (next: boolean) => {
    present = next;
    act(() => {
      for (const listener of listeners) listener();
    });
  };
  return { presence, leave: () => set(false), come: () => set(true) };
}

/** A check that stays open until the test answers it. */
function fakeCheck() {
  let answer: (() => void) | undefined;
  const calls = { n: 0 };
  const prompt = () => {
    calls.n += 1;
    return new Promise<void>((resolve) => {
      answer = resolve;
    });
  };
  const settle = async () => {
    await act(async () => {
      answer?.();
      await Promise.resolve();
    });
  };
  return { prompt, calls, settle };
}

function Screen({
  active = true,
  presence,
  prompt,
}: {
  active?: boolean;
  presence: Presence;
  prompt: () => Promise<unknown>;
}) {
  useUnlockPrompt({ active, presence, prompt });
  return null;
}

afterEach(() => cleanup());

describe("useUnlockPrompt", () => {
  it("asks once when the lock screen comes up in front, and not again on its own", async () => {
    const { presence, leave, come } = fakePresence(true);
    const check = fakeCheck();
    const view = render(
      <StrictMode>
        <Screen presence={presence} prompt={check.prompt} />
      </StrictMode>,
    );
    expect(check.calls.n).toBe(1);
    view.rerender(
      <StrictMode>
        <Screen presence={presence} prompt={check.prompt} />
      </StrictMode>,
    );
    // The system's dialog takes the focus while it is up, and gives it back as it closes.
    leave();
    come();
    expect(check.calls.n).toBe(1);
    // Cancelled: not again until Lockra is left and comes back.
    await check.settle();
    come();
    expect(check.calls.n).toBe(1);
    leave();
    come();
    expect(check.calls.n).toBe(2);
  });

  it("waits for Lockra to come to the front", () => {
    const { presence, leave, come } = fakePresence(false);
    const check = fakeCheck();
    render(<Screen presence={presence} prompt={check.prompt} />);
    leave();
    expect(check.calls.n).toBe(0);
    come();
    expect(check.calls.n).toBe(1);
  });

  it("does not ask right after the user locked, only once they left and came back", () => {
    const { presence, leave, come } = fakePresence(true);
    const check = fakeCheck();
    noteUserLock();
    render(<Screen presence={presence} prompt={check.prompt} />);
    expect(check.calls.n).toBe(0);
    leave();
    come();
    expect(check.calls.n).toBe(1);
  });

  it("forgets the user's lock once the lock screen has gone", () => {
    const { presence } = fakePresence(true);
    const check = fakeCheck();
    noteUserLock();
    const first = render(<Screen presence={presence} prompt={check.prompt} />);
    first.unmount();
    // The next lock screen (an automatic lock) asks at once.
    render(<Screen presence={presence} prompt={check.prompt} />);
    expect(check.calls.n).toBe(1);
  });

  it("never asks where the check is not the default", () => {
    const { presence, leave, come } = fakePresence(true);
    const check = fakeCheck();
    const view = render(<Screen active={false} presence={presence} prompt={check.prompt} />);
    leave();
    come();
    expect(check.calls.n).toBe(0);
    // Turned on while in front: the screen asks then.
    view.rerender(<Screen presence={presence} prompt={check.prompt} />);
    expect(check.calls.n).toBe(1);
  });
});
