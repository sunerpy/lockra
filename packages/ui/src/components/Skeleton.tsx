import { type ReactNode, useEffect, useState } from "react";
import { cx } from "../cx";

/** How long a load may take before placeholders show: a quick answer shows no flash of them. */
export const SKELETON_DELAY_MS = 300;

/** `true` once `delay` ms passed since the component mounted: render it only while waiting. */
export function useDelayed(delay = SKELETON_DELAY_MS): boolean {
  const [late, setLate] = useState(false);
  useEffect(() => {
    const timer = setTimeout(() => setLate(true), delay);
    return () => clearTimeout(timer);
  }, [delay]);
  return late;
}

/** A still placeholder bar in `inset2` (no shimmer: nothing moves while waiting). */
export function SkeletonBar({
  width,
  height = 12,
  className,
}: {
  width: number | string;
  height?: number;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cx("block rounded-4 bg-inset2", className)}
      style={{ width, height }}
    />
  );
}

/** Placeholders for a list being loaded: `rows` rows of an avatar and two lines, shown only once
 *  the load took longer than {@link SKELETON_DELAY_MS}. Render it while loading only. */
export function SkeletonRows({ rows = 5, label }: { rows?: number; label: string }): ReactNode {
  const late = useDelayed();
  if (!late) return null;
  return (
    <div role="status" aria-label={label} data-testid="skeleton" className="flex flex-col">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex h-[calc(var(--row-h)+32px)] items-center gap-3 px-4">
          <span aria-hidden className="size-10 shrink-0 rounded-[12px] bg-inset2" />
          <div className="flex min-w-0 flex-1 flex-col gap-2">
            <SkeletonBar width={`${40 + ((i * 17) % 30)}%`} height={12} />
            <SkeletonBar width={`${25 + ((i * 11) % 20)}%`} height={10} />
          </div>
        </div>
      ))}
    </div>
  );
}
