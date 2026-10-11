import { type CodeView, type EntryView, groupCode } from "@lockra/shared";
import type { KeyboardEvent, SyntheticEvent } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Button } from "./Button";
import { CountdownRing, WARNING_SECONDS, remaining } from "./CountdownRing";
import { EntryAvatar } from "./EntryAvatar";
import { OtpCode } from "./OtpCode";

export interface RecentCardProps {
  entry: EntryView;
  /** The entry's code from the last frame; absent until the first frame arrives. */
  code?: CodeView;
  nowMs: number;
  /** Dots until hovered or focused. */
  masked?: boolean;
  still?: boolean;
  /** `sm` is the phone's card. */
  size?: "md" | "sm";
  onCopy: () => void;
  onNext?: () => void;
  className?: string;
}

/** The card's measures (boards A1 and A2). */
const SIZE = {
  md: {
    pad: "p-6",
    avatar: 44,
    issuer: "text-[18px]",
    account: "text-[13px]",
    code: 54,
    ring: 80,
    stroke: 6,
    seconds: "text-[24px]",
    hint: "copyHint",
  },
  sm: {
    pad: "p-5",
    avatar: 40,
    issuer: "text-[17px]",
    account: "text-[12.5px]",
    code: 44,
    ring: 64,
    stroke: 5,
    seconds: "text-[20px]",
    hint: "tapHint",
  },
} as const;

const stop = (event: SyntheticEvent) => event.stopPropagation();

/** 「最近使用」: the account copied last, at the top of the codes page (boards A1, A2). Its code is
 *  large, its seconds sit in a ring with the next code under it, and the whole card copies on a
 *  click, Enter or Space; a counter-based account offers its next code instead of the ring. */
export function RecentCard({
  entry,
  code,
  nowMs,
  masked = false,
  still = false,
  size = "md",
  onCopy,
  onNext,
  className,
}: RecentCardProps) {
  const t = useT();
  const m = SIZE[size];
  const totp = code !== undefined && code.valid_from_ms !== null && code.valid_until_ms !== null;
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
  const next = totp && !expired ? code.next_code : null;
  const fontSize = (current ?? "").length > 6 ? Math.round(m.code * 0.86) : m.code;
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
      data-testid="recent-card"
      data-entry={entry.id}
      title={t("codes.copyHint")}
      onClick={onCopy}
      onKeyDown={onKeyDown}
      className={cx(
        "group flex min-w-0 items-stretch justify-between gap-4 rounded-14 bg-surface text-left hairline panel transition-shadow hover:shadow-pop",
        m.pad,
        className,
      )}>
      <div className="flex min-w-0 flex-col">
        <span className="text-[12px] font-medium text-fg-muted">{t("codes.hero.recent")}</span>
        <div className="mt-3 flex min-w-0 items-center gap-3">
          <EntryAvatar
            issuer={entry.issuer}
            account={entry.account}
            color={entry.color}
            mark={entry.mark}
            size={m.avatar}
          />
          <div className="min-w-0">
            <div className={cx("truncate font-semibold text-fg", m.issuer)} title={entry.issuer}>
              {entry.issuer || entry.account}
            </div>
            {entry.issuer !== "" && (
              <div className={cx("truncate text-fg-muted", m.account)} title={entry.account}>
                {entry.account}
              </div>
            )}
          </div>
        </div>
        <div className="mt-2 flex min-w-0 items-center">
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
                weight="semibold"
                tone={warning ? "warning" : "normal"}
                className={masked ? "group-hover:hidden group-focus-visible:hidden" : undefined}
              />
              {masked && (
                <OtpCode
                  code={current}
                  size={fontSize}
                  weight="semibold"
                  tone={warning ? "warning" : "normal"}
                  className="hidden group-hover:inline group-focus-visible:inline"
                />
              )}
            </>
          )}
        </div>
        <span className="mt-auto pt-1 text-[12px] text-fg-subtle">{t(`codes.hero.${m.hint}`)}</span>
      </div>
      <div className="flex shrink-0 flex-col items-center justify-center gap-2">
        {left !== undefined && code?.valid_from_ms != null && code.valid_until_ms !== null ? (
          <>
            <div className="relative" style={{ width: m.ring, height: m.ring }}>
              <CountdownRing
                validFromMs={code.valid_from_ms}
                validUntilMs={code.valid_until_ms}
                nowMs={nowMs}
                size={m.ring}
                stroke={m.stroke}
                still={still}
              />
              <div
                aria-hidden
                className="absolute inset-0 flex flex-col items-center justify-center leading-none">
                <span
                  data-testid="recent-seconds"
                  className={cx(
                    "mono font-semibold",
                    m.seconds,
                    warning ? "text-warning" : "text-fg",
                  )}>
                  {left.seconds}
                </span>
                <span className="mt-0.5 text-[10px] text-fg-subtle">
                  {t("codes.hero.secondsUnit")}
                </span>
              </div>
            </div>
            {next && !masked && (
              <span
                data-testid="recent-next"
                className="mono text-[12px] whitespace-nowrap text-fg-subtle">
                {t("codes.nextCode", { code: groupCode(next) })}
              </span>
            )}
          </>
        ) : (
          entry.kind.type === "hotp" &&
          onNext && (
            <span onClick={stop} onKeyDown={stop}>
              <Button icon="refresh" onClick={onNext}>
                {t("codes.hotpNext")}
              </Button>
            </span>
          )
        )}
      </div>
    </div>
  );
}
