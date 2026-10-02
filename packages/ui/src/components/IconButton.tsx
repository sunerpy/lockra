import type { ButtonHTMLAttributes } from "react";
import { cx } from "../cx";
import { Icon, type IconName } from "./Icon";

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: IconName;
  /** Required: the icon alone is not a name. */
  label: string;
  size?: 24 | 28;
  tone?: "default" | "danger";
  bordered?: boolean;
  /** A toggle: `aria-pressed`, and the icon filled in the accent colour while on. */
  pressed?: boolean;
}

export function IconButton({
  icon,
  label,
  size = 24,
  tone = "default",
  bordered = false,
  pressed,
  className,
  type = "button",
  ...rest
}: IconButtonProps) {
  return (
    <button
      type={type}
      aria-label={label}
      aria-pressed={pressed}
      title={label}
      className={cx(
        "inline-flex shrink-0 items-center justify-center rounded-6 transition-colors",
        "hover:bg-inset disabled:cursor-not-allowed disabled:opacity-50",
        pressed ? "text-accent-text hover:text-accent-text-hover" : "text-fg-muted hover:text-fg",
        tone === "danger" && "hover:text-danger",
        bordered && "bg-surface hairline",
        className,
      )}
      style={{ width: size, height: size }}
      {...rest}>
      <Icon name={icon} size={size === 24 ? 14 : 16} fill={pressed ? "currentColor" : "none"} />
    </button>
  );
}
