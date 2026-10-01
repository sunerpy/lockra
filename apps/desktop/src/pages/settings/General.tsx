import { LOCALE_SETTINGS, SORT_ORDERS } from "@lockra/shared";
import { Segmented, SettingsPane, SettingsRows, StatusRow, useT, useUiState } from "@lockra/ui";
import { useUpdateSettings } from "../../app/settings";

/** Settings › General: the interface language and the order of the codes. */
export function General() {
  const t = useT();
  const { settings } = useUiState();
  const update = useUpdateSettings();
  return (
    <SettingsPane title={t("settings.section.general")} lede={t("settings.general.lede")}>
      <SettingsRows>
        <StatusRow
          label={t("settings.general.locale.label")}
          help={t("settings.general.localeHint")}>
          <Segmented
            label={t("settings.general.locale.label")}
            value={settings.locale}
            onChange={(locale) => update({ locale })}
            options={LOCALE_SETTINGS.map((value) => ({
              value,
              label: t(`settings.general.locale.${value}`),
            }))}
          />
        </StatusRow>
        <StatusRow label={t("settings.general.sort")} help={t("settings.general.sortHint")}>
          <Segmented
            label={t("settings.general.sort")}
            value={settings.sort}
            onChange={(sort) => update({ sort })}
            options={SORT_ORDERS.map((value) => ({ value, label: t(`codes.sort.${value}`) }))}
          />
        </StatusRow>
      </SettingsRows>
    </SettingsPane>
  );
}
