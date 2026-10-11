import { groupCode } from "@lockra/shared";
import { cx } from "../cx";

export interface OtpCodeProps {
  code: string;
  /** Dots instead of digits (Settings › Security › hide codes, until hovered or focused). */
  masked?: boolean;
  /** The last seconds of the window. */
  tone?: "normal" | "warning" | "muted";
  /** A step of the scale, or pixels for a card's large code. */
  size?: "sm" | "md" | "lg" | number;
  /** 600 by default; a card's pixel-sized code is drawn at 500 unless it says otherwise. */
  weight?: "medium" | "semibold";
  className?: string;
}

const SIZE = { sm: "text-[13px]", md: "text-[22px]", lg: "text-[32px]" } as const;
const TONE = { normal: "text-fg", warning: "text-warning", muted: "text-fg-muted" } as const;

/** A code in mono, split for reading (`123 456`, `1234 5678`), announced digit by digit. */
export function OtpCode({
  code,
  masked = false,
  tone = "normal",
  size = "md",
  weight,
  className,
}: OtpCodeProps) {
  const heavy = (weight ?? (typeof size === "number" ? "medium" : "semibold")) === "semibold";
  const shown = masked ? groupCode("•".repeat(code.length)) : groupCode(code);
  return (
    <span
      data-testid="otp-code"
      data-masked={masked || undefined}
      aria-label={masked ? undefined : code.split("").join(" ")}
      style={typeof size === "number" ? { fontSize: size } : undefined}
      className={cx(
        "mono tracking-[0.04em] whitespace-nowrap tabular-nums",
        heavy ? "font-semibold" : "font-medium",
        typeof size === "number" ? undefined : SIZE[size],
        TONE[tone],
        className,
      )}>
      {shown}
    </span>
  );
}
