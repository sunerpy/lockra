// Notices from the core become toasts; errors a page catches become toasts too, translated.
import {
  type ErrorCode,
  type Notice,
  errorText,
  isLockraError,
  noticeIsProblem,
  noticeText,
} from "@lockra/shared";
import type { ToastStore } from "../components/Toast";
import { useT } from "../i18n/I18nProvider";
import { type ReactNode, createContext, useCallback, useContext, useMemo } from "react";

export interface Toaster {
  notice: (notice: Notice) => void;
  error: (error: unknown) => void;
  info: (message: string) => void;
}

const ToasterContext = createContext<Toaster | undefined>(undefined);

export function ToasterProvider({ store, children }: { store: ToastStore; children: ReactNode }) {
  const t = useT();
  // `push` is stable; the store object is new on every render, and a toaster that changed with it
  // would re-run every effect that depends on it (the export viewer refetched its page).
  const { push } = store;
  const notice = useCallback(
    (n: Notice) => {
      push({ message: noticeText(t, n), tone: noticeIsProblem(n) ? "danger" : "neutral" });
    },
    [push, t],
  );
  const error = useCallback(
    (e: unknown) => {
      const code: ErrorCode = isLockraError(e) ? e.code : "internal";
      push({ message: errorText(t, code), tone: "danger" });
    },
    [push, t],
  );
  const info = useCallback(
    (message: string) => {
      push({ message });
    },
    [push],
  );
  const value = useMemo(() => ({ notice, error, info }), [notice, error, info]);
  return <ToasterContext.Provider value={value}>{children}</ToasterContext.Provider>;
}

export function useToaster(): Toaster {
  const value = useContext(ToasterContext);
  if (!value) throw new Error("useToaster outside <ToasterProvider>");
  return value;
}
