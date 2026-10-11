import type { ReactNode } from "react";
import { cx } from "../cx";

export interface PageHeaderProps {
  title: string;
  /** A line of facts under the title (accounts, groups, sync). */
  subtitle?: ReactNode;
  /** At the end of the row: the page's search and its main action. */
  actions?: ReactNode;
  className?: string;
}

/** The top of a desktop page (board A1): the page's own 26 px title and a line of facts, its main
 *  actions at the end of the row. The title bar above keeps only the drag strip and the window. */
export function PageHeader({ title, subtitle, actions, className }: PageHeaderProps) {
  return (
    // A plain block, not a <header>: the title bar is the window's one banner.
    <div
      data-testid="page-header"
      // A little more room under it than between the page's cards (board A1: 32 px to the hero).
      className={cx("mb-3 flex items-center gap-6", className)}>
      <div className="min-w-0 flex-1">
        <h1 className="truncate text-[26px] leading-tight font-bold text-fg">{title}</h1>
        {subtitle !== undefined && (
          <p className="mt-1.5 text-[13px] text-fg-muted" data-testid="page-subtitle">
            {subtitle}
          </p>
        )}
      </div>
      {actions !== undefined && <div className="flex shrink-0 items-center gap-3">{actions}</div>}
    </div>
  );
}
