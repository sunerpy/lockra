import {
  LOCALE_SETTINGS,
  SORT_ORDERS,
  SUGGESTED_SHORTCUT,
  errorText,
  shortcutText,
} from "@lockra/shared";
import {
  Button,
  PinnedGroupSelect,
  Segmented,
  SettingsPane,
  SettingsRows,
  ShortcutRecorder,
  StatusRow,
  Toggle,
  useT,
  useUiState,
} from "@lockra/ui";
import { useDispatch } from "../../app/dispatch";
import { useUpdateSettings } from "../../app/settings";
import { UpdateControls } from "../../features/update/UpdateControls";

/** Settings › General: the interface language, the order of the codes and the group pinned at the
 *  top (this device's, kept in the vault), the quick search's shortcut where password management is
 *  offered, and automatic updates with the updater's status and its one action (as Voltip's 通用
 *  pane has them). */
export function General() {
  const t = useT();
  const { settings, update: updater, entries, pinned_group: pinned, features } = useUiState();
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
        {features.passwords && <QuickAccessRow />}
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

/** 快捷搜索: the global shortcut that brings the command palette up from anywhere (none until one is
 *  chosen; the suggestion is a click away). Where the shell could not register it (another app has
 *  it, or the desktop gives none to apps, as on Wayland) the row says so and gives the command that
 *  does the same, to bind in the desktop's own keyboard settings. */
function QuickAccessRow() {
  const t = useT();
  const { quick_access: quick, platform } = useUiState();
  const update = useUpdateSettings();
  const choose = (quick_access_shortcut: string | null) => update({ quick_access_shortcut });
  return (
    <StatusRow
      label={t("settings.general.quickAccess.label")}
      help={
        <>
          {t("settings.general.quickAccess.help")}
          {quick.error !== null && (
            <span className="mt-2 flex flex-col items-start gap-1" data-testid="quick-access-bind">
              <span className="text-danger">{errorText(t, quick.error)}</span>
              <span>{t("settings.general.quickAccess.bind")}</span>
              <code className="mono rounded-6 bg-inset px-2 py-0.5 text-[12px] text-fg select-all">
                lockra-desktop --palette
              </code>
            </span>
          )}
        </>
      }
      data-testid="quick-access">
      <div className="flex items-center gap-2">
        {quick.shortcut === null && (
          <Button variant="text" size="sm" onClick={() => choose(SUGGESTED_SHORTCUT)}>
            {t("settings.general.quickAccess.suggest", {
              keys: shortcutText(SUGGESTED_SHORTCUT, platform),
            })}
          </Button>
        )}
        <ShortcutRecorder
          value={quick.shortcut}
          onChange={choose}
          platform={platform}
          label={t("settings.general.quickAccess.field")}
          invalid={quick.error !== null}
        />
      </div>
    </StatusRow>
  );
}
