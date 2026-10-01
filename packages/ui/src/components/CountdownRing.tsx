import { useT } from "../i18n/I18nProvider";
import { cx } from "../cx";

export interface CountdownRingProps {
  validFromMs: number;
  validUntilMs: number;
  nowMs: number;
  size?: number;
  /** No transition (Settings › Appearance › reduce motion). */
  still?: boolean;
  className?: string;
}

/** Seconds at which the ring and the code turn to the warning colour. */
export const WARNING_SECONDS = 5;

/** How much of the window is left, 0..1, and the whole seconds remaining. */
export function remaining(
  validFromMs: number,
  validUntilMs: number,
  nowMs: number,
): { fraction: number; seconds: number } {
  const span = Math.max(1, validUntilMs - validFromMs);
  const left = Math.min(span, Math.max(0, validUntilMs - nowMs));
  return { fraction: left / span, seconds: Math.ceil(left / 1000) };
}

/** A ring that empties over a code's window: the accent colour, warning in the last five seconds.
 *  The circle is keyed on the window so a new window starts full without animating backwards. */
export function CountdownRing({
  validFromMs,
  validUntilMs,
  nowMs,
  size = 20,
  still = false,
  className,
}: CountdownRingProps) {
  const t = useT();
  const { fraction, seconds } = remaining(validFromMs, validUntilMs, nowMs);
  const stroke = 2.5;
  const r = (size - stroke) / 2;
  const circumference = 2 * Math.PI * r;
  const warning = seconds <= WARNING_SECONDS;
  return (
    <svg
      role="img"
      aria-label={`${t("ui.a11y.countdown")} ${seconds}`}
      data-testid="countdown-ring"
      data-warning={warning || undefined}
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      className={cx("shrink-0 -rotate-90", className)}>
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        strokeWidth={stroke}
        className="stroke-track"
      />
      <circle
        key={validFromMs}
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        strokeWidth={stroke}
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={circumference * (1 - fraction)}
        className={cx(
          warning ? "stroke-warning" : "stroke-accent",
          !still && "transition-[stroke-dashoffset] duration-1000 ease-linear",
        )}
      />
    </svg>
  );
}
