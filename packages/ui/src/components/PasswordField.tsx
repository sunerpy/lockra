import { type InputHTMLAttributes, useId, useState } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { IconButton } from "./IconButton";

export type Strength = "tooShort" | "weak" | "fair" | "strong";

/** A rough strength: length first (the core requires 8 characters), then variety. */
export function passwordStrength(password: string): Strength {
  const length = Array.from(password).length;
  if (length < 8) return "tooShort";
  // Letters outside ASCII (a Chinese pass phrase) count as one more class of their own.
  const beyondAscii = Array.from(password).some((c) => (c.codePointAt(0) ?? 0) > 0x7f);
  const classes =
    [/[a-z]/, /[A-Z]/, /\d/, /[^A-Za-z\d]/].filter((r) => r.test(password)).length +
    (beyondAscii ? 1 : 0);
  if (length >= 16 || (length >= 12 && classes >= 3)) return "strong";
  if (length >= 12 || classes >= 3) return "fair";
  return "weak";
}

const STRENGTH_STEP: Record<Strength, number> = { tooShort: 1, weak: 2, fair: 3, strong: 4 };
const STRENGTH_TONE: Record<Strength, string> = {
  tooShort: "bg-danger",
  weak: "bg-warning",
  fair: "bg-accent",
  strong: "bg-ok",
};

export interface PasswordFieldProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "type" | "size" | "onChange" | "value"
> {
  label: string;
  value: string;
  onChange: (value: string) => void;
  /** Show the four-step strength meter (new passwords only). */
  strength?: boolean;
  error?: string;
  help?: string;
  /** `lg` is the phone's: a 44 px field with larger text. */
  size?: "md" | "lg";
}

export function PasswordField({
  label,
  value,
  onChange,
  strength = false,
  error,
  help,
  size = "md",
  className,
  id,
  ...rest
}: PasswordFieldProps) {
  const t = useT();
  const autoId = useId();
  const inputId = id ?? autoId;
  const [visible, setVisible] = useState(false);
  const level = passwordStrength(value);
  return (
    <div className={cx("flex flex-col gap-1", className)}>
      <label htmlFor={inputId} className="text-[12px] text-fg-muted">
        {label}
      </label>
      <div
        className={cx(
          "flex items-center gap-1 rounded-6 bg-surface pr-1 pl-2.5 hairline transition-colors",
          size === "lg" ? "h-11" : "h-9",
          "focus-within:border-fg focus-within:shadow-[0_0_0_1px_var(--fg)]",
          error &&
            "border-danger focus-within:border-danger focus-within:shadow-[0_0_0_1px_var(--danger)]",
        )}>
        <input
          {...rest}
          id={inputId}
          type={visible ? "text" : "password"}
          value={value}
          autoComplete={rest.autoComplete ?? "off"}
          spellCheck={false}
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? `${inputId}-error` : undefined}
          onChange={(e) => onChange(e.target.value)}
          className={cx(
            "mono min-w-0 flex-1 bg-transparent outline-none placeholder:text-fg-subtle",
            size === "lg" ? "text-[15px]" : "text-[13px]",
          )}
        />
        <IconButton
          type="button"
          icon={visible ? "eyeOff" : "eye"}
          label={visible ? t("ui.a11y.hidePassword") : t("ui.a11y.showPassword")}
          onClick={() => setVisible((v) => !v)}
        />
      </div>
      {strength && value !== "" && (
        <div
          className="flex items-center gap-2"
          aria-label={t("ui.a11y.passwordStrength")}
          data-testid="password-strength"
          data-strength={level}>
          <div className="flex flex-1 gap-[3px]">
            {[1, 2, 3, 4].map((step) => (
              <span
                key={step}
                className={cx(
                  "h-1 flex-1 rounded-pill",
                  step <= STRENGTH_STEP[level] ? STRENGTH_TONE[level] : "bg-track",
                )}
              />
            ))}
          </div>
          <span className="w-10 text-right text-[11px] text-fg-muted">
            {t(`strength.${level}`)}
          </span>
        </div>
      )}
      {error ? (
        <p id={`${inputId}-error`} className="text-[12px] text-danger">
          {error}
        </p>
      ) : (
        help && <p className="text-[12px] text-fg-subtle">{help}</p>
      )}
    </div>
  );
}
