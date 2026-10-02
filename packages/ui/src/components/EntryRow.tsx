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
  className?: string;
}

/** One account: avatar, names, the current code and its ring. The whole row copies on click,
 *  Enter or Space; in the last five seconds the next code shows beside it. HOTP rows have a
 *  "next code" button instead of a ring. Pin and edit sit beside the overflow menu, and a right
 *  click is handed to `onContextMenu`. */
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
  className,
}: EntryRowProps) {
  const t = useT();
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
      onCopy();
    }
  };
  return (
    <div
      role="button"
      tabIndex={0}
      data-testid="entry-row"
      data-entry={entry.id}
      title={t("codes.copyHint")}
      onClick={onCopy}
      onKeyDown={onKeyDown}
      onContextMenu={onContextMenu}
      className={cx(
        "group grid h-[calc(var(--row-h)+28px)] grid-cols-[auto_minmax(0,1fr)_auto_auto] items-center gap-3 rounded-10 px-3 outline-none transition-colors hover:bg-inset focus-visible:bg-inset",
        className,
      )}>
      <EntryAvatar issuer={entry.issuer} account={entry.account} />
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-1.5">
          {entry.favorite && <Icon name="star" size={12} className="shrink-0 text-accent-text" />}
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
      <div className="flex flex-col items-end">
        {current === undefined ? (
          <span className="mono text-[22px] text-fg-subtle">— — —</span>
        ) : (
          <OtpCode
            code={current}
            masked={masked}
            tone={warning ? "warning" : "normal"}
            className={masked ? "group-hover:hidden group-focus-visible:hidden" : undefined}
          />
        )}
        {masked && current !== undefined && (
          <OtpCode
            code={current}
            tone={warning ? "warning" : "normal"}
            className="hidden group-hover:inline group-focus-visible:inline"
          />
        )}
        {warning && code?.next_code && !masked && (
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
        {entry.kind.type === "hotp" && onNext && (
          <IconButton icon="refresh" label={t("codes.hotpNext")} size={28} onClick={onNext} />
        )}
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
        {onEdit && (
          <IconButton
            icon="edit"
            label={t("codes.edit")}
            size={28}
            onClick={onEdit}
            data-testid="row-edit"
          />
        )}
        {menu}
      </div>
    </div>
  );
}
