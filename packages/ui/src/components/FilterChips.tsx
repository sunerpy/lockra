import { type KeyboardEvent, useRef } from "react";
import { cx } from "../cx";
import { Icon, type IconName } from "./Icon";

export interface FilterChip {
  value: string;
  label: string;
  count: number;
  icon?: IconName;
  /** The label is the user's own text (a group's name), not interface copy. */
  userText?: boolean;
}

export interface FilterChipsProps {
  /** The group's accessible name. */
  label: string;
  options: readonly FilterChip[];
  value: string;
  onChange: (value: string) => void;
  /** The phone's row scrolls sideways instead of wrapping. */
  scroll?: boolean;
  /** `lg` is the phone's: a taller touch target. */
  size?: "md" | "lg";
  disabled?: boolean;
  className?: string;
  "data-testid"?: string;
}

/** The codes page's filters (boards A1, A2): 全部, 收藏, each group, 未分组, each a pill with its
 *  count; one of them is on. A radio group: ← → Home End move and choose, Tab leaves. */
export function FilterChips({
  label,
  options,
  value,
  onChange,
  scroll = false,
  size = "md",
  disabled = false,
  className,
  "data-testid": testId,
}: FilterChipsProps) {
  const chips = useRef<(HTMLButtonElement | null)[]>([]);
  const at = Math.max(
    0,
    options.findIndex((o) => o.value === value),
  );
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const last = options.length - 1;
    const next =
      event.key === "ArrowRight" || event.key === "ArrowDown"
        ? (at + 1) % options.length
        : event.key === "ArrowLeft" || event.key === "ArrowUp"
          ? (at + last) % options.length
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? last
              : undefined;
    const option = next === undefined ? undefined : options[next];
    if (next === undefined || option === undefined) return;
    event.preventDefault();
    onChange(option.value);
    chips.current[next]?.focus();
  };
  return (
    <div
      role="radiogroup"
      aria-label={label}
      aria-disabled={disabled || undefined}
      onKeyDown={disabled ? undefined : onKeyDown}
      data-testid={testId}
      className={cx(
        "flex min-w-0 gap-2",
        // The row scrolls from the screen's left edge on the phone (to the button after it); its
        // scrollbar is not drawn.
        scroll ? "-ml-4 overflow-x-auto pl-4 [scrollbar-width:none]" : "flex-wrap",
        className,
      )}>
      {options.map((option, i) => {
        const on = i === at;
        return (
          <button
            key={option.value}
            ref={(el) => {
              chips.current[i] = el;
            }}
            type="button"
            role="radio"
            aria-checked={on}
            aria-label={`${option.label} ${option.count}`}
            tabIndex={on ? 0 : -1}
            disabled={disabled}
            onClick={() => {
              onChange(option.value);
            }}
            data-testid="filter-chip"
            className={cx(
              "inline-flex shrink-0 items-center gap-1.5 rounded-pill px-4 text-[13px] font-medium whitespace-nowrap hairline transition-colors disabled:opacity-50",
              size === "lg" ? "h-11" : "h-10",
              on
                ? "bg-chip-on text-chip-on-fg shadow-[var(--card-shadow)]"
                : "bg-surface text-fg-muted hover:text-fg",
            )}>
            {option.icon && <Icon name={option.icon} size={14} className="text-warning" />}
            <span
              className="max-w-40 truncate"
              {...(option.userText ? { "data-user-text": "" } : {})}>
              {option.label}
            </span>
            <span className={cx("tabular-nums", on ? "opacity-70" : "text-fg-subtle")}>
              {option.count}
            </span>
          </button>
        );
      })}
    </div>
  );
}
