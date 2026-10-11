import { groupCode } from "@lockra/shared";
import { cx } from "../cx";

export interface OtpCodeProps {
  code: string;
  /** Dots instead of digits (Settings › Security › hide codes, until hovered or focused). */
  masked?: boolean;
  /** The last seconds of the window. */
  tone?: "normal" | "warning" | "muted";
  /** A step of the scale, or pixels for a card's large code (drawn at weight 500, not 600). */
  size?: "sm" | "md" | "lg" | number;
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
  className,
}: OtpCodeProps) {
  const shown = masked ? groupCode("•".repeat(code.length)) : groupCode(code);
  return (
    <span
      data-testid="otp-code"
      data-masked={masked || undefined}
      aria-label={masked ? undefined : code.split("").join(" ")}
      style={typeof size === "number" ? { fontSize: size } : undefined}
      className={cx(
        "mono tracking-[0.04em] whitespace-nowrap tabular-nums",
        typeof size === "number" ? "font-medium" : cx("font-semibold", SIZE[size]),
        TONE[tone],
        className,
      )}>
      {shown}
    </span>
  );
}
