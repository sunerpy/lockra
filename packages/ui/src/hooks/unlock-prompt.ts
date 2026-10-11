// The lock screen asks for the system's check by itself where it is the default unlock (Settings ›
// Security): as the screen comes up in front (Lockra starting, or locking by itself), and each time
// Lockra comes back to the front while locked; not right after the user locked it, and after a
// check that was cancelled or failed only once Lockra has been left and come back. Never while
// Lockra is not in front: the system's dialog would come up over whatever the user is doing.
import { useEffect, useRef, useState } from "react";

/** Whether Lockra is in front, and when that may have changed. */
export interface Presence {
  present(): boolean;
  /** Calls `onChange` whenever presence may have changed; returns the unsubscribe. */
  subscribe(onChange: () => void): () => void;
}

/** The desktop: Lockra's window has the focus. */
export const windowFocus: Presence = {
  present: () => document.visibilityState === "visible" && document.hasFocus(),
  subscribe: (onChange) => {
    window.addEventListener("focus", onChange);
    window.addEventListener("blur", onChange);
    document.addEventListener("visibilitychange", onChange);
    return () => {
      window.removeEventListener("focus", onChange);
      window.removeEventListener("blur", onChange);
      document.removeEventListener("visibilitychange", onChange);
    };
  },
};

/** The phone: the app is on the screen. */
export const pageVisible: Presence = {
  present: () => document.visibilityState === "visible",
  subscribe: (onChange) => {
    document.addEventListener("visibilitychange", onChange);
    return () => document.removeEventListener("visibilitychange", onChange);
  },
};

// The user locked the vault themselves (the lock button, the shortcut): the next lock screen waits
// for them to leave and come back before it asks. Forgotten once that lock screen has gone.
let userLocked = false;

/** Call just before the user's own lock. */
export function noteUserLock(): void {
  userLocked = true;
}

export interface UnlockPromptOptions {
  /** The check is the default unlock and can run now. */
  active: boolean;
  presence: Presence;
  /** The check itself (what its button does); its outcome is the caller's to show. */
  prompt: () => Promise<unknown>;
}

export function useUnlockPrompt({ active, presence, prompt }: UnlockPromptOptions): void {
  const [armedAtStart] = useState(() => !userLocked);
  const armed = useRef(armedAtStart);
  const running = useRef(false);
  const ask = useRef(prompt);
  useEffect(() => {
    ask.current = prompt;
  }, [prompt]);
  useEffect(
    () => () => {
      userLocked = false;
    },
    [],
  );
  useEffect(() => {
    if (!active) return undefined;
    const update = () => {
      // The system's dialog takes the focus while it is up and gives it back as it closes.
      if (running.current) return;
      if (!presence.present()) {
        armed.current = true;
        return;
      }
      if (!armed.current) return;
      armed.current = false;
      running.current = true;
      void ask
        .current()
        .catch(() => undefined)
        .finally(() => {
          running.current = false;
        });
    };
    update();
    return presence.subscribe(update);
  }, [active, presence]);
}
