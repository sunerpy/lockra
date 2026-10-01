import type { ReactNode } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";

export interface StepListProps {
  steps: readonly ReactNode[];
  className?: string;
}

/** Numbered instructions (the import sources): mono numerals in small inset circles. */
export function StepList({ steps, className }: StepListProps) {
  const t = useT();
  return (
    <ol aria-label={t("ui.a11y.steps")} className={cx("flex flex-col gap-2", className)}>
      {steps.map((step, index) => (
        <li key={index} className="flex gap-2.5 text-[12px] leading-[18px] text-fg-muted">
          <span className="mono inline-flex size-[18px] shrink-0 items-center justify-center rounded-pill bg-inset text-[11px] text-fg">
            {index + 1}
          </span>
          <span className="min-w-0">{step}</span>
        </li>
      ))}
    </ol>
  );
}
