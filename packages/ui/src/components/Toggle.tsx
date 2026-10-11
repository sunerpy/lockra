import type { ReactNode } from "react";
import { cx } from "../cx";

export interface ToggleProps {
  checked: boolean;
  onChange: (next: boolean) => void;
  label?: ReactNode;
  /** Mono readout after the label (e.g. `PORT 8756`). */
  readout?: ReactNode;
  disabled?: boolean;
  id?: string;
  className?: string;
  /** Accessible name when there is no visible label (table cells). */
  ariaLabel?: string;
  /** `lg` is the phone's: a 52 × 32 switch, a touch target of its own. */
  size?: "md" | "lg";
}

/** 32×18 switch (the phone's 52×32). On = ink track, off = hairline track; the knob is the surface
 *  colour. */
export function Toggle({
  checked,
  onChange,
  label,
  readout,
  disabled = false,
  id,
  className,
  ariaLabel,
  size = "md",
}: ToggleProps) {
  const large = size === "lg";
  return (
    <label
      className={cx(
        "inline-flex items-center gap-2 text-[13px]",
        disabled && "opacity-50",
        className,
      )}>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => {
          onChange(!checked);
        }}
        // Codex's switch (user request 2026-09-29): 32 × 19, the accent when on, a faint ink
        // track when off, the same white thumb in both.
        className={cx(
          "relative inline-flex shrink-0 items-center rounded-pill transition-colors",
          large ? "h-8 w-[52px]" : "h-[19px] w-8",
          checked ? "bg-accent" : "bg-fg/10",
          disabled
            ? "cursor-not-allowed"
            : checked
              ? "cursor-pointer hover:opacity-90"
              : "cursor-pointer hover:bg-fg/15",
        )}>
        <span
          className={cx(
            "absolute rounded-full bg-thumb shadow-thumb transition-transform",
            large ? "top-1 size-6" : "top-[3px] size-[13px]",
            large
              ? checked
                ? "translate-x-6"
                : "translate-x-1"
              : checked
                ? "translate-x-4"
                : "translate-x-[3px]",
          )}
        />
      </button>
      {label !== undefined && <span>{label}</span>}
      {readout !== undefined && <span className="mono text-[11px] text-fg-muted">{readout}</span>}
    </label>
  );
}
