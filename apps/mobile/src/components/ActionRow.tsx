// One action of a list on the phone: a full-width row, an icon, its name and a hint, the arrow
// when it opens a page.
import { Icon, type IconName, cx } from "@lockra/ui";

export function ActionRow({
  icon,
  label,
  hint,
  opensPage = false,
  danger = false,
  onClick,
  testId,
}: {
  icon: IconName;
  label: string;
  hint?: string;
  /** It leads to a page of its own: an arrow at the end. */
  opensPage?: boolean;
  danger?: boolean;
  onClick: () => void;
  testId?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      data-testid={testId}
      className={cx(
        "flex min-h-14 w-full items-center gap-3 rounded-10 px-3 py-2 text-left transition-colors active:bg-inset",
        danger ? "text-danger" : "text-fg",
      )}>
      <Icon name={icon} size={20} className={danger ? undefined : "text-fg-muted"} />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="text-[15px]">{label}</span>
        {hint !== undefined && <span className="text-[13px] text-fg-muted">{hint}</span>}
      </span>
      {opensPage && <Icon name="chevronRight" size={16} className="text-fg-subtle" />}
    </button>
  );
}
