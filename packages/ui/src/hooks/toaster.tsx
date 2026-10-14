// Notices from the core become toasts; errors a page catches become toasts too, translated.
import {
  type ErrorCode,
  type Notice,
  errorText,
  isLockraError,
  noticeIsProblem,
  noticeText,
} from "@lockra/shared";
import type { EntryView } from "@lockra/shared";
import { useBackend } from "../backend/BackendProvider";
import type { ToastStore } from "../components/Toast";
import { useT } from "../i18n/I18nProvider";
import {
  type ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
} from "react";

export interface Toaster {
  /** A core notice; a copy names its account when `name` is given. */
  notice: (notice: Notice, name?: string) => void;
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
    (n: Notice, name?: string) => {
      push({
        message: noticeText(t, n, name),
        tone: noticeIsProblem(n) ? "danger" : "neutral",
        ...(n.type === "copied" || n.type === "item_copied" ? { icon: "check" as const } : {}),
      });
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

/** The core's notices as toasts (inside the i18n tree, so they are translated); a copy names the
 *  account it came from. */
export function NoticeBridge() {
  const { backend, state, items } = useBackend();
  const toaster = useToaster();
  const entries = useRef<readonly EntryView[]>([]);
  useEffect(() => {
    entries.current = state?.entries ?? [];
  }, [state]);
  useEffect(
    () =>
      backend.on((event) => {
        if (event.type !== "notice") return;
        const notice = event.notice;
        if (notice.type === "item_copied") {
          // A login's copy names the login, from the list the page asked for last.
          const item = items.snapshot()?.find((i) => i.id === notice.item_id);
          toaster.notice(notice, item ? item.title || item.username || item.email : undefined);
          return;
        }
        const entry =
          notice.type === "copied"
            ? entries.current.find((e) => e.id === notice.entry_id)
            : undefined;
        toaster.notice(notice, entry ? entry.issuer || entry.account : undefined);
      }),
    [backend, toaster, items],
  );
  return null;
}
