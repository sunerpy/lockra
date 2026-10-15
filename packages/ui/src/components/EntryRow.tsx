import { type CodeView, type EntryView, groupCode } from "@lockra/shared";
import type { KeyboardEvent, MouseEvent, ReactNode, SyntheticEvent } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { keepKey } from "../keys";
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
  /** A right click (or the context-menu key) on the row. */
  onContextMenu?: (event: MouseEvent<HTMLDivElement>) => void;
  /** The overflow menu (edit, reveal, delete, pin). */
  menu?: ReactNode;
  /** Selecting several accounts: the row is a checkbox that a click, Enter or Space ticks, and its
   *  buttons, menu and right click step aside. */
  selection?: { checked: boolean; onToggle: () => void };
  /** Just copied: what the row says in place of its code, in the theme's pill (ink in 墨白). */
  copied?: string;
  /** Its own card on the canvas (the phone, board A2), not a row inside the list's card. */
  card?: boolean;
  className?: string;
}

const stop = (event: SyntheticEvent) => event.stopPropagation();

/** One account (boards A1, A2): avatar, names, the current code and its ring. The whole row copies
 *  on click, Enter or Space; in the last five seconds the next code shows under it. HOTP rows have
 *  a "next code" button instead of a ring. A pinned account has a star after its name; the pin
 *  button and the menu sit after the ring, shown with a mouse only while the row is hovered or
 *  holds the focus. A right click is handed to `onContextMenu`; while selecting, the row is a
 *  checkbox instead. */
export function EntryRow({
  entry,
  code,
  nowMs,
  masked = false,
  still = false,
  onCopy,
  onNext,
  onFavorite,
  onContextMenu,
  menu,
  selection,
  copied,
  card = false,
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
  const codeSize = card ? 22 : 26;
  const actions = !selecting && (onFavorite !== undefined || menu !== undefined);
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
        "group flex items-center gap-3 outline-none transition-colors focus-visible:bg-inset",
        card
          ? "h-[calc(var(--row-h)+38px)] rounded-14 bg-surface px-3.5 hairline panel"
          : "h-[calc(var(--row-h)+32px)] rounded-10 px-4 hover:bg-inset",
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
        size={40}
      />
      <div className="flex min-w-0 flex-1 items-center gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-1.5">
            <span className="truncate text-[15px] font-semibold text-fg" title={entry.issuer}>
              {entry.issuer || entry.account}
            </span>
            {entry.favorite && (
              <Icon
                name="star"
                size={13}
                fill="currentColor"
                className="shrink-0 text-warning"
                data-testid="row-pinned"
              />
            )}
          </div>
          {entry.issuer !== "" && (
            <div
              className={cx("truncate text-fg-muted", card ? "text-[12px]" : "text-[12.5px]")}
              title={entry.account}>
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
      <div className="flex shrink-0 flex-col items-end">
        {copied !== undefined ? (
          <span
            data-testid="row-copied"
            className="inline-flex h-7 items-center gap-1.5 rounded-pill bg-[var(--row-pill-bg)] px-3 text-[13px] font-medium whitespace-nowrap text-[var(--row-pill-fg)]">
            <Icon name="check" size={14} />
            {copied}
          </span>
        ) : current === undefined ? (
          <span className="mono text-fg-subtle" style={{ fontSize: codeSize }}>
            — — —
          </span>
        ) : (
          <OtpCode
            code={current}
            masked={masked}
            size={codeSize}
            weight="semibold"
            tone={warning ? "warning" : "normal"}
            className={masked ? "group-hover:hidden group-focus-visible:hidden" : undefined}
          />
        )}
        {masked && current !== undefined && copied === undefined && (
          <OtpCode
            code={current}
            size={codeSize}
            weight="semibold"
            tone={warning ? "warning" : "normal"}
            className="hidden group-hover:inline group-focus-visible:inline"
          />
        )}
        {warning && code?.next_code && !masked && copied === undefined && (
          <span data-testid="next-code" className="mono text-[11.5px] text-fg-subtle">
            {t("codes.nextCode", { code: groupCode(code.next_code) })}
          </span>
        )}
      </div>
      {totp && !expired && code.valid_from_ms !== null && code.valid_until_ms !== null && (
        <CountdownRing
          validFromMs={code.valid_from_ms}
          validUntilMs={code.valid_until_ms}
          nowMs={nowMs}
          size={card ? 26 : 30}
          stroke={card ? 3 : 3.5}
          still={still}
          className="shrink-0"
        />
      )}
      {/* While selecting, a counter row keeps the ring's room, so the codes stay in one column. */}
      {entry.kind.type === "hotp" && selecting && (
        <span
          aria-hidden
          data-testid="row-ring-slot"
          className={cx("shrink-0", card ? "size-[28px]" : "size-[30px]")}
        />
      )}
      {entry.kind.type === "hotp" && onNext && !selecting && (
        <button
          type="button"
          aria-label={t("codes.hotpNext")}
          title={t("codes.hotpNext")}
          onClick={(event) => {
            event.stopPropagation();
            onNext();
          }}
          onKeyDown={keepKey}
          className={cx(
            "inline-flex shrink-0 items-center justify-center rounded-full bg-accent-soft text-accent-text transition-opacity hover:opacity-80",
            card ? "size-[28px]" : "size-[30px]",
          )}>
          <Icon name="refresh" size={16} />
        </button>
      )}
      {actions && (
        <div
          data-testid="row-actions"
          className={cx(
            "flex shrink-0 items-center gap-1",
            // With a mouse, they wait for the row: hovered, focused, or its menu open.
            !card &&
              "transition-opacity pointer-fine:opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 has-[[aria-expanded=true]]:opacity-100",
          )}
          onClick={stop}
          onKeyDown={keepKey}>
          {onFavorite && (
            <IconButton
              icon="star"
              label={t("codes.favorite")}
              size={28}
              pressed={entry.favorite}
              onClick={onFavorite}
              data-testid="row-favorite"
            />
          )}
          {menu}
        </div>
      )}
    </div>
  );
}
