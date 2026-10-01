import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";

/** `v` pulled into 0..1; NaN reads as 0. */
export function clamp01(v: number): number {
  return Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0;
}

export interface ProgressProps {
  /** 0..1; ignored when `indeterminate`. */
  value?: number;
  indeterminate?: boolean;
  size?: 2 | 4 | 6 | 8;
  tone?: "accent" | "ink" | "ok" | "danger";
  /** Render as N discrete segments (download rows) instead of a continuous bar. */
  segments?: number;
  label?: string;
  className?: string;
}

const TONE_CLASS = {
  accent: "bg-accent",
  ink: "bg-primary",
  ok: "bg-ok",
  danger: "bg-danger",
} as const;

export function Progress({
  value = 0,
  indeterminate = false,
  size = 4,
  tone = "accent",
  segments,
  label,
  className,
}: ProgressProps) {
  const t = useT();
  const pct = clamp01(value) * 100;
  const aria = {
    role: "progressbar" as const,
    "aria-label": label ?? t("ui.a11y.progress"),
    "aria-valuemin": 0,
    "aria-valuemax": 100,
    "aria-valuenow": indeterminate ? undefined : Math.round(pct),
  };
  if (segments !== undefined) {
    const lit = Math.round(clamp01(value) * segments);
    return (
      <div {...aria} className={cx("flex gap-[2px]", className)} style={{ height: size }}>
        {Array.from({ length: segments }, (_, i) => (
          <span
            key={i}
            className={cx("flex-1 rounded-[1px]", i < lit ? TONE_CLASS[tone] : "bg-track")}
          />
        ))}
      </div>
    );
  }
  return (
    <div
      {...aria}
      className={cx("relative overflow-hidden rounded-pill bg-track", className ?? "w-full")}
      style={{ height: size }}>
      <span
        className={cx(
          "absolute inset-y-0 left-0 rounded-pill",
          TONE_CLASS[tone],
          indeterminate && "w-1/3 [animation:lk-indeterminate_1.2s_ease-in-out_infinite]",
        )}
        style={indeterminate ? undefined : { width: `${pct}%` }}
      />
    </div>
  );
}
