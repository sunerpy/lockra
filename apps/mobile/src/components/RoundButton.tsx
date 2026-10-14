// A round 44 px button on the canvas (boards A2, 21): add, generate and lock in a screen's header,
// reorder by the filters.
import { Icon, type IconName } from "@lockra/ui";

export function RoundButton({
  icon,
  label,
  onClick,
  testId,
}: {
  icon: IconName;
  label: string;
  onClick: () => void;
  testId: string;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      data-testid={testId}
      className="inline-flex size-11 shrink-0 items-center justify-center rounded-full bg-surface text-fg-muted hairline panel active:text-fg">
      <Icon name={icon} size={18} />
    </button>
  );
}
