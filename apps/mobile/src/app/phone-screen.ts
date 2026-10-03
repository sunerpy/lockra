// The vault locks as the app leaves the screen (App.tsx), but not when Lockra itself opens a
// screen of the phone's over it: the camera, the photo picker, the camera permission prompt. The
// app is hidden behind those, not left. The call comes back as that screen closes, often before
// the app is in front again, so its hiding lasts until the app is visible once more. Leaving the
// app from the camera's page locks the vault in Rust (src-tauri/src/scanner.rs); from the photo
// picker, which runs in another app, the auto-lock time does.

let open = 0;
let returning = false;

/** A screen of the phone's own is over the app, or closing. */
export function phoneScreenOpen(): boolean {
  return open > 0 || returning;
}

/** Run `work`, which opens such a screen. */
export async function overPhoneScreen<T>(work: () => Promise<T>): Promise<T> {
  open += 1;
  try {
    return await work();
  } finally {
    open -= 1;
    if (open === 0 && document.visibilityState === "hidden") returning = true;
  }
}

if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") returning = false;
  });
}
