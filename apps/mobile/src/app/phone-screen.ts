// The vault locks as the app leaves the screen (App.tsx), but not when Lockra itself opens a
// screen of the phone's over it: the camera, the photo picker, the camera permission prompt. The
// app is hidden behind those, not left. Should the user leave from there, the vault locks as the
// call returns (from the camera's page, Rust locks it at once: src-tauri/src/scanner.rs).

let open = 0;

/** A screen of the phone's own is over the app. */
export function phoneScreenOpen(): boolean {
  return open > 0;
}

/** Run `work`, which opens such a screen; if the app is hidden still when it returns, `lock`. */
export async function overPhoneScreen<T>(work: () => Promise<T>, lock: () => void): Promise<T> {
  open += 1;
  try {
    return await work();
  } finally {
    open -= 1;
    if (open === 0 && document.visibilityState === "hidden") lock();
  }
}
