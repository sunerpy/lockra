import { type ReactNode, useEffect, useId, useRef } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Keycap } from "./Keycap";

export interface DialogProps {
  open: boolean;
  title: ReactNode;
  children?: ReactNode;
  /** Mono facts row under the body (`history.db · 1.8 MB · 上限 500 条 / 30 天`). */
  facts?: ReactNode;
  actions: ReactNode;
  onClose: () => void;
  /** Footer hint, defaults to `Esc 取消`. */
  hint?: ReactNode;
  width?: number;
  /** A long form: the title and the buttons stay, the body between them scrolls, the dialog at
   *  most 720 px high and 24 px from the window's edges. */
  scroll?: boolean;
}

/** The open dialogs, the most recent last: Esc closes only that one (a confirmation over an editor
 *  closes, the editor stays). */
const openDialogs: symbol[] = [];

/** Modal over a 28 % scrim; Esc and scrim click close. Focuses `[data-autofocus]` or the dialog.
 *  Its title names it (an id of its own, so a dialog over another one keeps its name). */
export function Dialog({
  open,
  title,
  children,
  facts,
  actions,
  onClose,
  hint,
  width = 420,
  scroll = false,
}: DialogProps) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();
  // The latest close handler, so a new one (a re-render) neither re-focuses the dialog nor moves
  // it on top of one opened after it.
  const close = useRef(onClose);
  useEffect(() => {
    close.current = onClose;
  }, [onClose]);

  useEffect(() => {
    if (!open) return;
    const token = Symbol("dialog");
    openDialogs.push(token);
    const node = ref.current;
    const target = node?.querySelector<HTMLElement>("[data-autofocus]") ?? node;
    target?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented || openDialogs.at(-1) !== token) return;
      // Every dialog listens on `document`, so stopPropagation cannot order them; the dialog on
      // top marks the key consumed and outer listeners (the settings dialog) check the flag.
      e.preventDefault();
      e.stopPropagation();
      close.current();
    };
    // Capture phase: it runs before every bubble-phase document listener regardless of the order
    // they were registered in.
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      const at = openDialogs.indexOf(token);
      if (at >= 0) openDialogs.splice(at, 1);
    };
  }, [open]);

  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center scrim"
      onClick={onClose}
      data-testid="dialog-scrim">
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onClick={(e) => {
          e.stopPropagation();
        }}
        className={cx(
          "flex flex-col gap-4 rounded-14 bg-overlay p-6 outline-none hairline shadow-pop overlay-frost",
          scroll && "max-h-[min(720px,calc(100vh-48px))] gap-0 p-0",
        )}
        style={{ width }}>
        <h2
          id={titleId}
          className={cx("text-[16px] font-semibold text-fg", scroll && "px-6 pt-6 pb-3.5")}>
          {title}
        </h2>
        {children !== undefined && (
          <div
            className={cx(
              "text-[13px] leading-5 text-fg-muted",
              scroll && "min-h-0 flex-1 overflow-y-auto px-6 pb-4",
            )}
            data-testid={scroll ? "dialog-body" : undefined}>
            {children}
          </div>
        )}
        {facts !== undefined && (
          <div className={cx("mono text-[11px] text-fg-subtle", scroll && "px-6")}>{facts}</div>
        )}
        <div
          className={cx(
            "flex items-center justify-between gap-3",
            scroll && "border-t border-border px-6 pt-3.5 pb-6",
          )}>
          <span className="mono flex items-center gap-1.5 text-[10px] text-fg-subtle">
            {hint ?? (
              <>
                <Keycap>Esc</Keycap> {t("ui.dialog.escCancel")}
              </>
            )}
          </span>
          <div className="flex items-center gap-2">{actions}</div>
        </div>
      </div>
    </div>
  );
}
