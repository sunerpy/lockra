// A sheet that rises from the bottom of the phone's screen over a dim backdrop: a tap on the
// backdrop, Esc or the phone's back gesture closes it. While it is open it holds a history entry of
// its own (with the page's depth, app/nav.tsx), so the back gesture closes the sheet and not the
// page under it.
import { type ReactNode, useEffect, useId, useRef } from "react";
import { createPortal } from "react-dom";

const MARK = "lockraSheet";

function marked(state: unknown): boolean {
  return typeof state === "object" && state !== null && MARK in state;
}

export function BottomSheet({
  title,
  onClose,
  children,
  "data-testid": testId,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  "data-testid"?: string;
}) {
  const titleId = useId();
  const panel = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  useEffect(() => {
    close.current = onClose;
  }, [onClose]);
  useEffect(() => {
    const base: object =
      typeof history.state === "object" && history.state !== null ? history.state : {};
    history.pushState({ ...base, [MARK]: true }, "");
    const onPop = (event: PopStateEvent) => {
      if (!marked(event.state)) close.current();
    };
    window.addEventListener("popstate", onPop);
    panel.current?.focus();
    return () => {
      window.removeEventListener("popstate", onPop);
      // Closed from inside (a pick, the backdrop, Esc): its entry goes, and the page stays.
      if (marked(history.state)) history.back();
    };
  }, []);
  return createPortal(
    <div className="fixed inset-0 z-50 flex flex-col justify-end" data-testid={testId}>
      <button
        type="button"
        aria-label={title}
        tabIndex={-1}
        className="absolute inset-0 scrim"
        onClick={() => close.current()}
      />
      <div
        ref={panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onKeyDown={(e) => {
          if (e.key !== "Escape") return;
          e.preventDefault();
          close.current();
        }}
        className="relative flex max-h-[80vh] flex-col rounded-t-14 bg-surface pb-[max(env(safe-area-inset-bottom),0.75rem)] shadow-pop outline-none">
        <div aria-hidden className="mx-auto mt-2 h-1 w-10 shrink-0 rounded-full bg-border" />
        <h2 id={titleId} className="px-4 pt-3 pb-2 text-[16px] font-semibold text-fg">
          {title}
        </h2>
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-2">{children}</div>
      </div>
    </div>,
    document.body,
  );
}
