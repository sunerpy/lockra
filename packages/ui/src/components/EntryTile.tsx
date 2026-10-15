import { type CodeView, type EntryView, groupCode } from "@lockra/shared";
import type { KeyboardEvent, MouseEvent, ReactNode, SyntheticEvent } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { keepKey } from "../keys";
import { Button } from "./Button";
import { WARNING_SECONDS, remaining } from "./CountdownRing";
import { EntryAvatar } from "./EntryAvatar";
import { Icon } from "./Icon";
import { IconButton } from "./IconButton";
import { OtpCode } from "./OtpCode";

export interface EntryTileProps {
  entry: EntryView;
  /** The entry's code from the last frame; absent until the first frame arrives. */
  code?: CodeView;
  nowMs: number;
  /** Dots until hovered or focused. */
  masked?: boolean;
  still?: boolean;
  /** `sm` is the phone's card, two to a row. */
  size?: "md" | "sm";
  /** Just copied: what the card says in place of its timer. */
  copied?: string;
  onCopy: () => void;
  onNext?: () => void;
  /** Pin or unpin, from the star beside the menu. */
  onFavorite?: () => void;
  /** A right click (or the context-menu key, or a long press) on the card. */
  onContextMenu?: (event: MouseEvent<HTMLDivElement>) => void;
  /** The overflow menu (edit, reveal, delete, pin). */
  menu?: ReactNode;
  className?: string;
}

/** The card's measures (DESIGN.md, 验证码视图 › 卡片; boards A6, B1 and B2). */
const SIZE = {
  md: {
    tile: "min-h-[200px] p-5 pb-3.5",
    avatar: 36,
    gap: "gap-3",
    issuer: "text-[15px] leading-5",
    account: "text-[12px] leading-4",
    buttons: "-mt-1 -mr-1.5",
    codeLine: "mt-6 h-14",
    code: [46, 40],
    nextLine: "mt-1 h-4 text-[12px]",
    footer: "mt-1 h-[30px] text-[12px]",
    bar: "h-1",
    check: 16,
    pill: "h-8 text-[13px]",
  },
  sm: {
    tile: "min-h-[156px] p-3.5 pb-3",
    avatar: 28,
    gap: "gap-2",
    issuer: "text-[13.5px] leading-[18px]",
    account: "text-[11px] leading-4",
    buttons: "-mt-2.5 -mr-2.5",
    codeLine: "mt-5 h-9",
    code: [28, 23],
    nextLine: "mt-0.5 h-3.5 text-[10.5px]",
    footer: "mt-0.5 h-7 text-[10.5px]",
    bar: "h-[3px]",
    check: 14,
    pill: "h-7 text-[12px]",
  },
} as const;

/** Clicks inside the card's own controls do not copy (their keys: `keepKey`). */
const stop = (event: SyntheticEvent) => event.stopPropagation();

/** One account as a card: avatar, names, the star and the menu above a large code; below it a bar
 *  that empties over the code's window, with the next code in the last five seconds, or the
 *  counter and "next code" for HOTP. The whole card copies on click, Enter or Space; just copied,
 *  it says so in place of the bar, in the theme's copied colours (`tile-copied`: ink in 墨白). */
export function EntryTile({
  entry,
  code,
  nowMs,
  masked = false,
  still = false,
  size = "md",
  copied,
  onCopy,
  onNext,
  onFavorite,
  onContextMenu,
  menu,
  className,
}: EntryTileProps) {
  const t = useT();
  const m = SIZE[size];
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
  const fontSize = (current ?? "").length > 6 ? m.code[1] : m.code[0];
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
      data-testid="entry-tile"
      data-entry={entry.id}
      data-copied={copied !== undefined || undefined}
      title={t("codes.copyHint")}
      onClick={onCopy}
      onKeyDown={onKeyDown}
      onContextMenu={onContextMenu}
      className={cx(
        // Hovered, it lifts; in 墨白 its edge turns to ink too (--tile-hover-edge).
        "group flex min-w-0 flex-col rounded-10 bg-surface text-left hairline panel transition-[box-shadow,background-color,border-color] hover:border-[color:var(--tile-hover-edge)] hover:shadow-pop",
        m.tile,
        copied !== undefined && "tile-copied",
        className,
      )}>
      <div className={cx("flex min-w-0 items-start", m.gap)}>
        <EntryAvatar
          issuer={entry.issuer}
          account={entry.account}
          color={entry.color}
          mark={entry.mark}
          size={m.avatar}
        />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-1.5">
            {/* The star button says it where there is one; the name says it where there is none. */}
            {entry.favorite && !onFavorite && (
              <Icon name="star" size={12} className="shrink-0 text-accent-text" />
            )}
            <span className={cx("truncate font-semibold text-fg", m.issuer)} title={entry.issuer}>
              {entry.issuer || entry.account}
            </span>
          </div>
          {entry.issuer !== "" && (
            <div className={cx("truncate text-fg-muted", m.account)} title={entry.account}>
              {entry.account}
            </div>
          )}
        </div>
        {(onFavorite || menu) && (
          <div
            className={cx("flex shrink-0 items-center", m.buttons)}
            onClick={stop}
            onKeyDown={keepKey}>
            {onFavorite && (
              <IconButton
                icon="star"
                label={t("codes.favorite")}
                size={28}
                pressed={entry.favorite}
                onClick={onFavorite}
                // A pinned card shows its star; the others show it on hover or focus.
                className={
                  entry.favorite
                    ? undefined
                    : "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"
                }
                data-testid="tile-favorite"
              />
            )}
            {menu}
          </div>
        )}
      </div>
      <div className={cx("flex min-w-0 items-center", m.codeLine)}>
        {current === undefined ? (
          <span className="mono text-fg-subtle" style={{ fontSize }}>
            — — —
          </span>
        ) : (
          <>
            <OtpCode
              code={current}
              masked={masked}
              size={fontSize}
              tone={warning ? "warning" : "normal"}
              className={masked ? "group-hover:hidden group-focus-visible:hidden" : undefined}
            />
            {masked && (
              <OtpCode
                code={current}
                size={fontSize}
                tone={warning ? "warning" : "normal"}
                className="hidden group-hover:inline group-focus-visible:inline"
              />
            )}
          </>
        )}
      </div>
      <div className={cx("flex min-w-0 items-center gap-2", m.nextLine)}>
        {warning && code?.next_code && !masked && (
          <span data-testid="next-code" className="mono truncate text-fg-subtle">
            {t("codes.nextCode", { code: groupCode(code.next_code) })}
          </span>
        )}
        {size === "md" && copied === undefined && (
          <span
            aria-hidden
            data-testid="tile-hint"
            className="ml-auto whitespace-nowrap text-fg-subtle opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
            {t("codes.copyHint")} ↵
          </span>
        )}
      </div>
      <div className={cx("mt-auto flex min-w-0 items-center gap-3", m.footer)}>
        {copied !== undefined ? (
          <span
            data-testid="tile-copied"
            className={cx(
              "inline-flex min-w-0 items-center gap-2 rounded-pill bg-[var(--tile-pill-bg)] px-[var(--tile-pill-pad)] font-medium text-[var(--tile-pill-fg)]",
              m.pill,
            )}>
            <Icon name="check" size={m.check} className="shrink-0" />
            <span className="truncate">{copied}</span>
          </span>
        ) : entry.kind.type === "hotp" ? (
          <>
            {/* The phone's card has room for the button alone (board B2). */}
            {size === "md" && (
              <span className="mono truncate text-fg-muted">
                {t("codes.kind.hotp", { counter: entry.kind.counter })}
              </span>
            )}
            {onNext && (
              <span
                className={size === "md" ? "ml-auto" : undefined}
                onClick={stop}
                onKeyDown={keepKey}>
                <Button icon="refresh" onClick={onNext}>
                  {t("codes.hotpNext")}
                </Button>
              </span>
            )}
          </>
        ) : (
          left !== undefined && (
            <>
              <div
                aria-hidden
                data-testid="countdown-bar"
                data-warning={warning || undefined}
                className={cx(
                  "relative min-w-0 flex-1 overflow-hidden rounded-pill bg-track",
                  m.bar,
                )}>
                {/* Keyed on the window: a new one starts full without running backwards. */}
                <span
                  key={code?.valid_from_ms ?? 0}
                  className={cx(
                    "absolute inset-y-0 left-0 rounded-pill",
                    // The theme's meter: the accent, or ink in 墨白.
                    warning ? "bg-warning" : "bg-[var(--meter)]",
                    !still && "transition-[width] duration-1000 ease-linear",
                  )}
                  style={{ width: `${left.fraction * 100}%` }}
                />
              </div>
              <span data-testid="countdown-seconds" className="mono shrink-0 text-fg-muted">
                {t("common.seconds", { n: left.seconds })}
              </span>
            </>
          )
        )}
      </div>
    </div>
  );
}

export interface TileBox {
  left: number;
  top: number;
  width: number;
}

/** Where an arrow key takes the focus among the cards: left and right through their order, up and
 *  down to the nearest card of the row above or below (a section heading between them does not
 *  matter); -1 above the first row, for the search field; `undefined` for any other key. */
export function tileStep(boxes: readonly TileBox[], at: number, key: string): number | undefined {
  const here = boxes[at];
  if (here === undefined) return undefined;
  const last = boxes.length - 1;
  switch (key) {
    case "ArrowRight":
      return Math.min(at + 1, last);
    case "ArrowLeft":
      return Math.max(at - 1, 0);
    case "Home":
      return 0;
    case "End":
      return last;
    case "ArrowDown":
    case "ArrowUp": {
      const down = key === "ArrowDown";
      // A row is the cards sharing a top, give or take a pixel of rounding.
      const tops = boxes
        .map((box) => box.top)
        .filter((top) => (down ? top > here.top + 1 : top < here.top - 1));
      if (tops.length === 0) return down ? at : -1;
      const row = down ? Math.min(...tops) : Math.max(...tops);
      const centre = here.left + here.width / 2;
      let best = at;
      let distance = Number.POSITIVE_INFINITY;
      boxes.forEach((box, i) => {
        if (Math.abs(box.top - row) > 1) return;
        const d = Math.abs(box.left + box.width / 2 - centre);
        if (d < distance) {
          best = i;
          distance = d;
        }
      });
      return best;
    }
    default:
      return undefined;
  }
}
