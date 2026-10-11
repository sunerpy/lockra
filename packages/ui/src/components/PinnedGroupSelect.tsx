import {
  ALL_GROUPS,
  type EntryView,
  FAVORITES,
  type PinnedGroup,
  UNGROUPED,
  entryGroups,
  pinnedFilter,
  pinnedFromFilter,
} from "@lockra/shared";
import { useT } from "../i18n/I18nProvider";
import { Select } from "./Select";

export interface PinnedGroupSelectProps {
  entries: readonly EntryView[];
  pinned: PinnedGroup | null;
  onChange: (pinned: PinnedGroup | null) => void;
  /** `lg` is the phone's. */
  size?: "md" | "lg";
  "data-testid"?: string;
}

/** Settings' 「置顶分组」, desktop and phone alike: none, 收藏, each group in use, or 未分组. A pin of a
 *  group no account has any more reads as none. */
export function PinnedGroupSelect({
  entries,
  pinned,
  onChange,
  size = "md",
  "data-testid": testId,
}: PinnedGroupSelectProps) {
  const t = useT();
  const options = [
    { value: ALL_GROUPS, label: t("codes.pin.none") },
    { value: FAVORITES, label: t("codes.chips.favorites") },
    ...entryGroups(entries).map((group) => ({ value: group, label: group })),
    { value: UNGROUPED, label: t("codes.groupNone") },
  ];
  const current = pinnedFilter(pinned);
  return (
    <Select
      aria-label={t("codes.pin.label")}
      size={size}
      value={options.some((o) => o.value === current) ? current : ALL_GROUPS}
      onChange={(value) => onChange(pinnedFromFilter(value))}
      options={options}
      data-testid={testId}
    />
  );
}
