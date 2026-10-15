import {
  type EntryView,
  type ItemView,
  type PinnedGroup,
  entryFilters,
  itemFilters,
} from "@lockra/shared";
import { type KeyboardEvent, useMemo, useRef } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Icon, type IconName } from "./Icon";

export interface FilterChip {
  value: string;
  label: string;
  count: number;
  icon?: IconName;
  /** The label is the user's own text (a group's name), not interface copy. */
  userText?: boolean;
  /** The group pinned on this device (first of the chips, the one the page opens on). */
  pinned?: boolean;
}

/** The codes page's chips, desktop and phone alike: 全部, 收藏 with its star, each group and 未分组,
 *  with their counts; the pinned one first, a pin on a pinned group. */
export function useFilterChips(
  entries: readonly EntryView[],
  pinned: PinnedGroup | null,
): FilterChip[] {
  const t = useT();
  return useMemo(
    () =>
      entryFilters(entries, pinned).map((f) => ({
        value: f.value,
        count: f.count,
        label:
          f.kind === "all"
            ? t("codes.chips.all")
            : f.kind === "favorites"
              ? t("codes.chips.favorites")
              : f.kind === "ungrouped"
                ? t("codes.groupNone")
                : f.value,
        pinned: f.pinned,
        ...(f.kind === "favorites"
          ? { icon: "star" as const }
          : f.pinned
            ? { icon: "pin" as const }
            : {}),
        ...(f.kind === "group" ? { userText: true } : {}),
      })),
    [entries, pinned, t],
  );
}

/** The password page's filters as chips: 全部, 收藏, each group, 未分组 (no pin there yet). */
export function useItemFilterChips(items: readonly ItemView[]): FilterChip[] {
  const t = useT();
  return useMemo(
    () =>
      itemFilters(items).map((f) => ({
        value: f.value,
        count: f.count,
        label:
          f.kind === "all"
            ? t("codes.chips.all")
            : f.kind === "favorites"
              ? t("passwords.favorites")
              : f.kind === "ungrouped"
                ? t("codes.groupNone")
                : f.value,
        pinned: false,
        ...(f.kind === "favorites" ? { icon: "star" as const } : {}),
        ...(f.kind === "group" ? { userText: true } : {}),
      })),
    [items, t],
  );
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
            data-pinned={option.pinned || undefined}
            className={cx(
              "inline-flex shrink-0 items-center gap-1.5 rounded-pill px-4 text-[13px] font-medium whitespace-nowrap hairline transition-colors disabled:opacity-50",
              size === "lg" ? "h-11" : "h-10",
              on
                ? "bg-chip-on text-chip-on-fg shadow-[var(--card-shadow)]"
                : "bg-surface text-fg-muted hover:text-fg",
            )}>
            {option.icon && (
              <Icon
                name={option.icon}
                size={14}
                className={option.icon === "star" ? "text-warning" : "text-accent-text"}
              />
            )}
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
