import type { ReactNode } from "react";
import { cx } from "../cx";
import { Icon, type IconName } from "./Icon";

export interface TileRowProps {
  /** The 16 px icon in the 32 px tile on the left. */
  icon: IconName;
  title: ReactNode;
  /** The line under the title: a status lamp and its words, a date. */
  subtitle?: ReactNode;
  /** Right side: the row's actions. */
  actions?: ReactNode;
  /** A line under the row (a warning that belongs to it). */
  note?: ReactNode;
  className?: string;
  "data-testid"?: string;
}

/** A settings list row with an icon tile (Settings › 浏览器's browsers and paired extensions): the
 *  tile, the name and its status line, the actions on the right, hairline below. */
export function TileRow({
  icon,
  title,
  subtitle,
  actions,
  note,
  className,
  ...rest
}: TileRowProps) {
  return (
    <div
      data-testid={rest["data-testid"]}
      className={cx(
        "flex flex-col gap-1.5 border-b border-border py-3 last:border-b-0",
        className,
      )}>
      <div className="flex min-h-9 items-center gap-3">
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-6 bg-inset text-fg-muted">
          <Icon name={icon} size={16} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[14px] font-medium text-fg">{title}</div>
          {subtitle !== undefined && (
            <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[12px] text-fg-muted">
              {subtitle}
            </div>
          )}
        </div>
        {actions !== undefined && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
      </div>
      {note !== undefined && <div className="pl-11 text-[12px] leading-4">{note}</div>}
    </div>
  );
}
