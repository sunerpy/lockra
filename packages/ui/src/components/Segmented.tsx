import { cx } from "../cx";
import { Icon, type IconName } from "./Icon";

const HEIGHT = { sm: "h-7", md: "h-8", lg: "h-11" } as const;
const TEXT = { sm: "text-[11px]", md: "text-[12px]", lg: "text-[14px]" } as const;

export interface SegmentedOption<V extends string> {
  value: V;
  label: string;
  disabled?: boolean;
  /** Tooltip explaining why a segment is unavailable. */
  reason?: string;
  icon?: IconName;
}

export interface SegmentedProps<V extends string> {
  options: readonly SegmentedOption<V>[];
  value: V;
  onChange: (value: V) => void;
  /** `soft`: selected = surface + hairline (本地 | 云端). `ink`: selected = ink fill (全部 · 已生效). */
  variant?: "soft" | "ink";
  /** `lg` is the phone's: a 44 px touch target. */
  size?: "sm" | "md" | "lg";
  mono?: boolean;
  /** Each option shows its icon alone; its label is still its name and its tooltip. */
  iconOnly?: boolean;
  label?: string;
  className?: string;
  "data-testid"?: string;
}

export function Segmented<V extends string>({
  options,
  value,
  onChange,
  variant = "soft",
  size = "md",
  mono = false,
  iconOnly = false,
  label,
  className,
  "data-testid": testId,
}: SegmentedProps<V>) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      data-testid={testId}
      className={cx(
        "inline-flex items-center rounded-6 bg-inset p-0.5 hairline",
        HEIGHT[size],
        className,
      )}>
      {options.map((opt) => {
        const selected = opt.value === value;
        return (
          <button
            key={opt.value}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={iconOnly ? opt.label : undefined}
            disabled={opt.disabled}
            title={opt.reason ?? (iconOnly ? opt.label : undefined)}
            onClick={() => {
              onChange(opt.value);
            }}
            className={cx(
              // One pixel inside the frame's corner, whatever the theme makes of it.
              "inline-flex h-full items-center gap-1.5 rounded-[calc(var(--radius-control)_-_1px)] whitespace-nowrap transition-colors",
              iconOnly ? "px-2" : "px-3",
              TEXT[size],
              mono && "mono",
              selected &&
                variant === "soft" &&
                "bg-surface text-fg shadow-[0_0_0_1px_var(--border)]",
              selected && variant === "ink" && "bg-primary text-primary-fg",
              !selected && "text-fg-muted hover:text-fg",
              opt.disabled && "cursor-not-allowed opacity-40 hover:text-fg-muted",
            )}>
            {opt.icon && <Icon name={opt.icon} size={size === "lg" ? 18 : 16} />}
            {!iconOnly && opt.label}
          </button>
        );
      })}
    </div>
  );
}
