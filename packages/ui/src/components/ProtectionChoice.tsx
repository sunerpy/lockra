import type { Protection } from "@lockra/shared";
import { type KeyboardEvent, useRef } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Icon, type IconName } from "./Icon";

const ICONS: Record<Protection, IconName> = {
  fill: "shieldOff",
  confirm: "shieldCheck",
  no_extension: "ban",
};

export const PROTECTION_ORDER: readonly Protection[] = ["fill", "confirm", "no_extension"];

export interface ProtectionChoiceProps {
  value: Protection;
  onChange: (value: Protection) => void;
  disabled?: boolean;
  "data-testid"?: string;
}

/** A login's protection level as three cards, one of them chosen (a radio group: ← → ↑ ↓ move and
 *  choose): fill directly, verify before showing, copying or filling, never hand it to the
 *  browser extension. */
export function ProtectionChoice({
  value,
  onChange,
  disabled = false,
  "data-testid": testId,
}: ProtectionChoiceProps) {
  const t = useT();
  const cards = useRef<(HTMLButtonElement | null)[]>([]);
  const at = Math.max(0, PROTECTION_ORDER.indexOf(value));
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step =
      event.key === "ArrowDown" || event.key === "ArrowRight"
        ? 1
        : event.key === "ArrowUp" || event.key === "ArrowLeft"
          ? -1
          : 0;
    if (step === 0) return;
    event.preventDefault();
    const next = (at + step + PROTECTION_ORDER.length) % PROTECTION_ORDER.length;
    const choice = PROTECTION_ORDER[next];
    if (choice === undefined) return;
    onChange(choice);
    cards.current[next]?.focus();
  };
  return (
    <div
      role="radiogroup"
      aria-label={t("item.protection.label")}
      onKeyDown={disabled ? undefined : onKeyDown}
      className="flex flex-col gap-2"
      data-testid={testId}>
      {PROTECTION_ORDER.map((level, i) => {
        const on = level === value;
        return (
          <button
            key={level}
            ref={(el) => {
              cards.current[i] = el;
            }}
            type="button"
            role="radio"
            aria-checked={on}
            tabIndex={on ? 0 : -1}
            disabled={disabled}
            onClick={() => onChange(level)}
            data-testid="protection-option"
            data-level={level}
            className={cx(
              "flex min-h-13 items-center gap-3 rounded-10 px-3.5 py-2 text-left transition-colors disabled:opacity-50",
              on
                ? "bg-accent-soft/40 shadow-[inset_0_0_0_1.5px_var(--accent)]"
                : "bg-surface hairline hover:bg-inset",
            )}>
            <Icon
              name={ICONS[level]}
              size={18}
              className={on ? "shrink-0 text-accent-text" : "shrink-0 text-fg-muted"}
            />
            <span className="flex min-w-0 flex-col">
              <span className="text-[13px] font-medium text-fg">
                {t(`item.protection.${level}`)}
              </span>
              <span className="text-[12px] text-fg-muted">{t(`item.protection.${level}Help`)}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}
