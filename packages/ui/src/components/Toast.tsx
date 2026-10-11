import { useCallback, useEffect, useRef, useState } from "react";
import { cx } from "../cx";
import { Icon, type IconName } from "./Icon";
import { Keycap } from "./Keycap";

export interface ToastAction {
  label: string;
  keys?: string;
  onClick: () => void;
}

export interface ToastItem {
  id: string;
  message: string;
  action?: ToastAction;
  /** Milliseconds before auto-dismiss (2 000 / 3 000 / 5 000 by tone). */
  duration: number;
  tone?: "neutral" | "danger";
  /** Before the message (a check for a copy). */
  icon?: IconName;
}

export interface ToastProps {
  toast: ToastItem;
  onDismiss: (id: string) => void;
}

/** A 48 px pill in the theme's ink (danger red for a failure), its text in the surface colour, with
 *  a 2 px drain bar shrinking over the undo window (board A1: dark on the light themes, light on
 *  the dark ones). */
export function Toast({ toast, onDismiss }: ToastProps) {
  const danger = toast.tone === "danger";
  return (
    <div
      role={danger ? "alert" : "status"}
      data-tone={toast.tone ?? "neutral"}
      className={cx(
        "relative flex h-12 max-w-[min(560px,calc(100vw-48px))] min-w-80 items-center gap-3 overflow-hidden rounded-pill px-5 text-[13px] text-surface-solid shadow-pop",
        danger ? "bg-danger" : "bg-fg",
      )}>
      {toast.icon && <Icon name={toast.icon} size={16} className="shrink-0" />}
      <span className="min-w-0 flex-1 truncate">{toast.message}</span>
      {toast.action && (
        <button
          type="button"
          className="flex shrink-0 items-center gap-1.5 font-semibold hover:underline"
          onClick={() => {
            toast.action?.onClick();
            onDismiss(toast.id);
          }}>
          {toast.action.label}
          {toast.action.keys && <Keycap>{toast.action.keys}</Keycap>}
        </button>
      )}
      <span
        className="absolute bottom-0 left-0 h-0.5 bg-[var(--surface)] opacity-40"
        style={{ animation: `lk-drain ${toast.duration}ms linear forwards` }}
      />
    </div>
  );
}

export interface ToastViewportProps {
  toasts: readonly ToastItem[];
  onDismiss: (id: string) => void;
}

/** Bottom-centre stack, newest at the bottom, max 3 visible; `--toast-bottom` lifts it (above the
 *  phone's dock). */
export function ToastViewport({ toasts, onDismiss }: ToastViewportProps) {
  const visible = toasts.slice(-3);
  if (visible.length === 0) return null;
  return (
    <div
      data-testid="toast-viewport"
      className="pointer-events-none fixed bottom-[var(--toast-bottom,1.5rem)] left-1/2 z-40 flex -translate-x-1/2 flex-col items-center gap-2">
      {visible.map((t) => (
        <div key={t.id} className="pointer-events-auto">
          <Toast toast={t} onDismiss={onDismiss} />
        </div>
      ))}
    </div>
  );
}

export type ToastInput = Omit<ToastItem, "id" | "duration"> & { duration?: number };

export interface ToastStore {
  toasts: ToastItem[];
  push: (input: ToastInput) => string;
  dismiss: (id: string) => void;
}

/** Queue with auto-dismiss timers; timers are cleared on unmount. */
export function useToasts(): ToastStore {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  const counter = useRef(0);

  const dismiss = useCallback((id: string) => {
    const t = timers.current.get(id);
    if (t !== undefined) clearTimeout(t);
    timers.current.delete(id);
    setToasts((prev) => prev.filter((x) => x.id !== id));
  }, []);

  const push = useCallback(
    (input: ToastInput) => {
      counter.current += 1;
      const id = `toast-${counter.current}`;
      const duration = input.duration ?? 3000;
      setToasts((prev) => [...prev, { ...input, id, duration }]);
      timers.current.set(
        id,
        setTimeout(() => {
          dismiss(id);
        }, duration),
      );
      return id;
    },
    [dismiss],
  );

  useEffect(() => {
    const map = timers.current;
    return () => {
      for (const t of map.values()) clearTimeout(t);
      map.clear();
    };
  }, []);

  return { toasts, push, dismiss };
}
