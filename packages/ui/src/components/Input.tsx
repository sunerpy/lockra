import { type InputHTMLAttributes, type TextareaHTMLAttributes, useId } from "react";
import { cx } from "../cx";
import { Icon, type IconName } from "./Icon";
import { Keycaps } from "./Keycap";

export interface InputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "size"> {
  icon?: IconName;
  /** Keycap hint at the right edge (e.g. `Ctrl F`). */
  keys?: string;
  mono?: boolean;
  error?: string;
  label?: string;
  help?: string;
  /** `xl` is the codes page's search (boards A1, A2). */
  size?: "sm" | "md" | "lg" | "xl";
  /** A round-ended field on the theme's card surface: the codes page's search. */
  pill?: boolean;
}

const SIZE_CLASS = {
  sm: "h-7 text-[12px]",
  md: "h-8 text-[13px]",
  lg: "h-11 text-[15px]",
  xl: "h-13.5 text-[14px]",
} as const;

export function Input({
  icon,
  keys,
  mono = false,
  error,
  label,
  help,
  size = "md",
  pill = false,
  className,
  id,
  ...rest
}: InputProps) {
  const autoId = useId();
  const inputId = id ?? autoId;
  const errorId = `${inputId}-error`;
  return (
    <div className={cx("flex flex-col gap-1", className)}>
      {label && (
        <label htmlFor={inputId} className="text-[12px] text-fg-muted">
          {label}
        </label>
      )}
      <div
        className={cx(
          "flex items-center gap-2 bg-surface hairline transition-colors",
          pill ? "rounded-pill px-4 panel" : "rounded-6 px-2.5",
          "focus-within:border-fg focus-within:shadow-[0_0_0_1px_var(--fg)]",
          error &&
            "border-danger focus-within:border-danger focus-within:shadow-[0_0_0_1px_var(--danger)]",
          SIZE_CLASS[size],
        )}>
        {icon && (
          <Icon
            name={icon}
            size={size === "xl" ? 18 : 14}
            className={cx("shrink-0", size === "xl" ? "text-fg-muted" : "text-fg-subtle")}
          />
        )}
        <input
          id={inputId}
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? errorId : undefined}
          className={cx(
            "min-w-0 flex-1 bg-transparent outline-none placeholder:text-fg-subtle",
            mono && "mono",
          )}
          {...rest}
        />
        {keys && <Keycaps keys={keys} />}
      </div>
      {error ? (
        <p id={errorId} className="text-[12px] text-danger">
          {error}
        </p>
      ) : (
        help && <p className="text-[12px] text-fg-subtle">{help}</p>
      )}
    </div>
  );
}

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  mono?: boolean;
  label?: string;
}

export function Textarea({ mono = false, label, className, id, ...rest }: TextareaProps) {
  const autoId = useId();
  const inputId = id ?? autoId;
  return (
    <div className={cx("flex flex-col gap-1", className)}>
      {label && (
        <label htmlFor={inputId} className="text-[12px] text-fg-muted">
          {label}
        </label>
      )}
      <textarea
        id={inputId}
        className={cx(
          "w-full resize-none rounded-10 bg-surface p-3 text-[13px] leading-5 hairline outline-none",
          "placeholder:text-fg-subtle focus:border-fg focus:shadow-[0_0_0_1px_var(--fg)]",
          mono && "mono",
        )}
        {...rest}
      />
    </div>
  );
}
