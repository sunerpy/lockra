import type { ReactNode } from "react";
import { cx } from "../cx";
import { Lamp, type LampTone } from "./Lamp";

export interface LampTextProps {
  tone: LampTone;
  children: ReactNode;
  /** Mono readout appended after the label. */
  readout?: ReactNode;
  mono?: boolean;
  pulse?: boolean;
  size?: "sm" | "md";
  className?: string;
}

/** `● 已是最新 · 0.3.0` — the design's standard status readout (ported from Voltip). */
export function LampText({
  tone,
  children,
  readout,
  mono = false,
  pulse = false,
  size = "md",
  className,
}: LampTextProps) {
  return (
    <span
      className={cx(
        "inline-flex min-w-0 items-center gap-1.5",
        size === "sm" ? "text-[11px]" : "text-[12px]",
        mono && "mono",
        className,
      )}>
      <Lamp tone={tone} size={size === "sm" ? 6 : 8} pulse={pulse} />
      <span className="min-w-0 text-fg">{children}</span>
      {readout !== undefined && <span className="mono text-fg-muted">{readout}</span>}
    </span>
  );
}
