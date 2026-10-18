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
  useBackend,
  useT,
  useUiState,
} from "@lockra/ui";
import { useEffect, useState } from "react";
import { useDispatch } from "../../app/dispatch";
import { useUpdateSettings } from "../../app/settings";
import { UpdateControls } from "../../features/update/UpdateControls";

/** Settings › General: the interface language, the order of the codes and the group pinned at the
 *  top (this device's, kept in the vault), the quick search's shortcut where password management is
 *  offered, starting at login, and automatic updates with the updater's status and its one action
 *  (as Voltip's 通用 pane has them). */
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
        <LaunchAtLoginRow />
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

/** 登录时启动 Lockra: the system's own login entry (Windows' Run value, a macOS LaunchAgent, an XDG
 *  autostart entry), read from the shell as the pane opens, so what the user turned off in the
 *  system's own list shows as off; no row where the build keeps none. Such a start stays in the
 *  tray while Lockra runs in the background (src-tauri/src/autostart.rs). */
function LaunchAtLoginRow() {
  const t = useT();
  const { backend } = useBackend();
  const { features } = useUiState();
  const [on, setOn] = useState<boolean | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let live = true;
    backend.launchAtLogin().then(
      (value) => live && setOn(value),
      () => live && setOn(null),
    );
    return () => {
      live = false;
    };
  }, [backend]);
  if (on === null) return null;
  const turn = async (next: boolean) => {
    setFailed(false);
    try {
      setOn(await backend.setLaunchAtLogin(next));
    } catch {
      setFailed(true);
    }
  };
  const label = t("settings.general.launchAtLogin");
  return (
    <StatusRow
      label={label}
      help={
        <>
          {t(
            features.passwords
              ? "settings.general.launchAtLoginHelp"
              : "settings.general.launchAtLoginHelpPlain",
          )}
          {failed && (
            <span className="mt-2 block text-danger" data-testid="launch-at-login-error">
              {t("settings.general.launchAtLoginFailed")}
            </span>
          )}
        </>
      }
      data-testid="launch-at-login">
      <Toggle checked={on} onChange={(next) => void turn(next)} ariaLabel={label} />
    </StatusRow>
  );
}

/** 快捷搜索: the global shortcut that brings the command palette up from anywhere (none until one is
 *  chosen; the suggestion is a click away). Where the shell could not register it (another app has
 *  it, or the desktop gives none to apps, as on Wayland) the row says so; on Linux it also gives the
 *  command that does the same, to bind in the desktop's own keyboard settings. */
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
              {platform === "linux" && (
                <>
                  <span>{t("settings.general.quickAccess.bind")}</span>
                  <code className="mono rounded-6 bg-inset px-2 py-0.5 text-[12px] text-fg select-all">
                    lockra-desktop --palette
                  </code>
                </>
              )}
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
