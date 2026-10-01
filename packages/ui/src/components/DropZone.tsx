import type { ReactNode } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Icon, type IconName } from "./Icon";

export interface DropZoneProps {
  /** A drag is hovering over the window (the shell reports it, without paths). */
  active?: boolean;
  icon?: IconName;
  title: string;
  hint?: string;
  /** Click or Enter: open the file picker instead. */
  onActivate: () => void;
  children?: ReactNode;
  className?: string;
}

export function DropZone({
  active = false,
  icon = "upload",
  title,
  hint,
  onActivate,
  children,
  className,
}: DropZoneProps) {
  const t = useT();
  return (
    <button
      type="button"
      data-testid="drop-zone"
      data-active={active || undefined}
      aria-label={t("ui.a11y.dropZone")}
      onClick={onActivate}
      className={cx(
        "flex w-full flex-col items-center justify-center gap-2 rounded-10 border border-dashed px-4 py-6 text-center transition-colors",
        active
          ? "border-accent bg-accent-soft text-accent-text"
          : "border-border-strong text-fg-muted hover:bg-inset",
        className,
      )}>
      <Icon name={icon} size={20} />
      <span className="text-[13px] font-medium text-fg">{title}</span>
      {hint && <span className="text-[12px]">{hint}</span>}
      {children}
    </button>
  );
}
