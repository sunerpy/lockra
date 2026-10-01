// The shell reports a file drag over the window as "enter" / "leave" (never the paths: dropped
// files go to the import in Rust). Outside Tauri nothing is reported.
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

export const DRAG_EVENT_NAME = "lockra://drag";

export type DragListen = (
  event: string,
  handler: (event: { payload: unknown }) => void,
) => Promise<() => void>;

export function useFileDrag(source: DragListen | null = isTauri() ? listen : null): boolean {
  const [dragging, setDragging] = useState(false);
  useEffect(() => {
    if (source === null) return undefined;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void source(DRAG_EVENT_NAME, (event) => setDragging(event.payload === "enter"))
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [source]);
  return dragging;
}
