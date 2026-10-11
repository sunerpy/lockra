import { type CodeView, type EntryView, groupCode } from "@lockra/shared";
import type { KeyboardEvent, MouseEvent, ReactNode } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { CountdownRing, WARNING_SECONDS, remaining } from "./CountdownRing";
import { EntryAvatar } from "./EntryAvatar";
import { Icon } from "./Icon";
import { IconButton } from "./IconButton";
import { OtpCode } from "./OtpCode";

export interface EntryRowProps {
  entry: EntryView;
  /** The entry's code from the last frame; absent until the first frame arrives. */
  code?: CodeView;
  nowMs: number;
  /** Dots until hovered or focused. */
  masked?: boolean;
  still?: boolean;
  onCopy: () => void;
  onNext?: () => void;
  /** Pin or unpin, from a button beside the menu. */
  onFavorite?: () => void;
  /** Edit, from a button beside the menu. */
  onEdit?: () => void;
  /** A right click (or the context-menu key) on the row. */
  onContextMenu?: (event: MouseEvent<HTMLDivElement>) => void;
  /** The overflow menu (edit, reveal, delete, pin). */
  menu?: ReactNode;
  /** Selecting several accounts: the row is a checkbox that a click, Enter or Space ticks, and its
   *  buttons, menu and right click step aside. */
  selection?: { checked: boolean; onToggle: () => void };
  /** Just copied: what the row says in place of its code, in the theme's pill (ink in 墨白). */
  copied?: string;
  className?: string;
}

/** One account: avatar, names, the current code and its ring. The whole row copies on click,
 *  Enter or Space; in the last five seconds the next code shows beside it. HOTP rows have a
 *  "next code" button instead of a ring. Pin and edit sit beside the overflow menu, and a right
 *  click is handed to `onContextMenu`; while selecting, the row is a checkbox instead. */
export function EntryRow({
  entry,
  code,
  nowMs,
  masked = false,
  still = false,
  onCopy,
  onNext,
  onFavorite,
  onEdit,
  onContextMenu,
  menu,
  selection,
  copied,
  className,
}: EntryRowProps) {
  const t = useT();
  const selecting = selection !== undefined;
  const activate = selection?.onToggle ?? onCopy;
  const totp = code !== undefined && code.valid_from_ms !== null && code.valid_until_ms !== null;
  // A frame late at the window's end: the next code is already the right one.
  const expired = totp && code.valid_until_ms !== null && nowMs >= code.valid_until_ms;
  const current =
    code === undefined
      ? undefined
      : expired && code.next_code !== null
        ? code.next_code
        : code.code;
  const left =
    totp && !expired
      ? remaining(code.valid_from_ms ?? 0, code.valid_until_ms ?? 0, nowMs)
      : undefined;
  const warning = left !== undefined && left.seconds <= WARNING_SECONDS;
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      activate();
    }
  };
  return (
    <div
      role={selecting ? "checkbox" : "button"}
      aria-checked={selection?.checked}
      tabIndex={0}
      data-testid="entry-row"
      data-entry={entry.id}
      data-copied={copied !== undefined || undefined}
      title={selecting ? t("codes.selectHint") : t("codes.copyHint")}
      onClick={activate}
      onKeyDown={onKeyDown}
      onContextMenu={selecting ? (event) => event.preventDefault() : onContextMenu}
      className={cx(
        "group grid h-[calc(var(--row-h)+28px)] items-center gap-3 rounded-10 px-3 outline-none transition-colors hover:bg-inset focus-visible:bg-inset",
        selecting
          ? "grid-cols-[auto_auto_minmax(0,1fr)_auto_auto]"
          : "grid-cols-[auto_minmax(0,1fr)_auto_auto]",
        className,
      )}>
      {selection && (
        // The row is the checkbox; this one only shows it.
        <input
          type="checkbox"
          tabIndex={-1}
          aria-hidden
          readOnly
          checked={selection.checked}
          className="pointer-events-none size-4 shrink-0 accent-accent"
          data-testid="row-select"
        />
      )}
      <EntryAvatar
        issuer={entry.issuer}
        account={entry.account}
        color={entry.color}
        mark={entry.mark}
      />
      <div className="flex min-w-0 items-center gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-1.5">
            {/* The pin button says it where there is one; the name says it where there is none. */}
            {entry.favorite && !onFavorite && (
              <Icon name="star" size={12} className="shrink-0 text-accent-text" />
            )}
            <span className="truncate text-[14px] font-medium text-fg" title={entry.issuer}>
              {entry.issuer || entry.account}
            </span>
          </div>
          {entry.issuer !== "" && (
            <div className="truncate text-[12px] text-fg-muted" title={entry.account}>
              {entry.account}
            </div>
          )}
        </div>
        {!selecting && copied === undefined && (
          <span
            aria-hidden
            data-testid="row-hint"
            // No room taken until it shows: a phone (no hover) keeps the whole width for the names.
            className="hidden shrink-0 text-[12px] whitespace-nowrap text-fg-subtle group-hover:inline group-focus-visible:inline">
            {t("codes.copyHint")}
          </span>
        )}
      </div>
      <div className="flex flex-col items-end">
        {copied !== undefined ? (
          <span
            data-testid="row-copied"
            className="inline-flex h-7 items-center gap-1.5 rounded-pill bg-[var(--row-pill-bg)] px-3 text-[13px] font-medium whitespace-nowrap text-[var(--row-pill-fg)]">
            <Icon name="check" size={14} />
            {copied}
          </span>
        ) : current === undefined ? (
          <span className="mono text-[22px] text-fg-subtle">— — —</span>
        ) : (
          <OtpCode
            code={current}
            masked={masked}
            tone={warning ? "warning" : "normal"}
            className={masked ? "group-hover:hidden group-focus-visible:hidden" : undefined}
          />
        )}
        {masked && current !== undefined && copied === undefined && (
          <OtpCode
            code={current}
            tone={warning ? "warning" : "normal"}
            className="hidden group-hover:inline group-focus-visible:inline"
          />
        )}
        {warning && code?.next_code && !masked && copied === undefined && (
          <span data-testid="next-code" className="mono text-[11px] text-fg-subtle">
            {t("codes.nextCode", { code: groupCode(code.next_code) })}
          </span>
        )}
      </div>
      <div
        className="flex items-center gap-1"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}>
        {totp && !expired && code.valid_from_ms !== null && code.valid_until_ms !== null && (
          <CountdownRing
            validFromMs={code.valid_from_ms}
            validUntilMs={code.valid_until_ms}
            nowMs={nowMs}
            still={still}
          />
        )}
        {entry.kind.type === "hotp" && onNext && !selecting && (
          <IconButton icon="refresh" label={t("codes.hotpNext")} size={28} onClick={onNext} />
        )}
        {onFavorite && !selecting && (
          <IconButton
            icon="star"
            label={t("codes.favorite")}
            size={28}
            pressed={entry.favorite}
            onClick={onFavorite}
            data-testid="row-favorite"
          />
        )}
        {onEdit && !selecting && (
          <IconButton
            icon="edit"
            label={t("codes.edit")}
            size={28}
            onClick={onEdit}
            data-testid="row-edit"
          />
        )}
        {!selecting && menu}
      </div>
    </div>
  );
}
