import { type ChangeEvent, useId } from "react";
import { cx } from "../cx";
import { Input } from "./Input";

export interface SliderProps {
  /** Its name (the row's label). */
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  /** A number field beside the track, for typing the value. */
  withInput?: boolean;
  disabled?: boolean;
  className?: string;
  "data-testid"?: string;
}

/** A range on the theme's track: the generator's length and number of words. The number field
 *  beside it takes what is typed, kept within `min` and `max`. */
export function Slider({
  label,
  value,
  min,
  max,
  step = 1,
  onChange,
  withInput = true,
  disabled = false,
  className,
  "data-testid": testId,
}: SliderProps) {
  const id = useId();
  const clamp = (n: number) => Math.min(max, Math.max(min, Math.round(n / step) * step));
  const fromRange = (event: ChangeEvent<HTMLInputElement>) =>
    onChange(clamp(Number(event.target.value)));
  const fromInput = (event: ChangeEvent<HTMLInputElement>) => {
    const n = Number(event.target.value);
    if (event.target.value !== "" && Number.isFinite(n)) onChange(clamp(n));
  };
  return (
    <div className={cx("flex items-center gap-3", className)} data-testid={testId}>
      <input
        id={id}
        type="range"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        onChange={fromRange}
        className="h-1.5 w-60 cursor-pointer accent-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-50"
        data-testid={testId === undefined ? undefined : `${testId}-range`}
      />
      {withInput && (
        <Input
          type="number"
          size="sm"
          mono
          aria-label={label}
          min={min}
          max={max}
          step={step}
          value={value}
          disabled={disabled}
          onChange={fromInput}
          className="w-16"
          data-testid={testId === undefined ? undefined : `${testId}-input`}
        />
      )}
    </div>
  );
}
