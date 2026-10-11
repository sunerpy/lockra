import { LOCALE_SETTINGS, SORT_ORDERS } from "@lockra/shared";
import {
  PinnedGroupSelect,
  Segmented,
  SettingsPane,
  SettingsRows,
  StatusRow,
  Toggle,
  useT,
  useUiState,
} from "@lockra/ui";
import { useDispatch } from "../../app/dispatch";
import { useUpdateSettings } from "../../app/settings";
import { UpdateControls } from "../../features/update/UpdateControls";

/** Settings › General: the interface language, the order of the codes and the group pinned at the
 *  top (this device's, kept in the vault), and automatic updates with the updater's status and its
 *  one action (as Voltip's 通用 pane has them). */
export function General() {
  const t = useT();
  const { settings, update: updater, entries, pinned_group: pinned } = useUiState();
  const update = useUpdateSettings();
  const dispatch = useDispatch();
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
        <StatusRow
          label={t("codes.pin.label")}
          help={t("codes.pin.hint")}
          data-testid="settings-pinned-group">
          <PinnedGroupSelect
            entries={entries}
            pinned={pinned}
            onChange={(next) => void dispatch({ command: "view_pin_group", pinned: next })}
          />
        </StatusRow>
        <StatusRow
          label={t("settings.general.autoUpdate")}
          help={t("settings.general.autoUpdateHelp")}
          data-testid="update-auto">
          <Toggle
            checked={settings.auto_update}
            disabled={updater.method === null}
            onChange={(auto_update) => update({ auto_update })}
            ariaLabel={t("settings.general.autoUpdate")}
          />
        </StatusRow>
        <div className="py-4" data-testid="update-section">
          <UpdateControls />
        </div>
      </SettingsRows>
    </SettingsPane>
  );
}
