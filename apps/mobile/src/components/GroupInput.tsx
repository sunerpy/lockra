// A group's name on the phone: typed, or one of the groups in use tapped from the chips below.
import { entryGroups } from "@lockra/shared";
import { Chip, Input, useT, useUiState } from "@lockra/ui";

export function GroupInput({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const t = useT();
  const { entries } = useUiState();
  const groups = entryGroups(entries);
  return (
    <div className="flex flex-col gap-2">
      <Input
        size="lg"
        label={t("entry.group")}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={t("entry.groupPlaceholder")}
        autoComplete="off"
      />
      {groups.length > 0 && (
        <div
          role="group"
          aria-label={t("mobile.groupSuggestions")}
          className="flex flex-wrap gap-2">
          {groups.map((group) => (
            <Chip
              key={group}
              size="lg"
              round
              active={group === value.trim()}
              onClick={() => onChange(group)}>
              {group}
            </Chip>
          ))}
        </div>
      )}
    </div>
  );
}
