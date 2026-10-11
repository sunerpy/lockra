// The phone's settings: the language, the order of the codes and their groups; the theme (a pair
// of a light and a dark one to a row, as the desktop's columns), the code view and motion; locking, the fingerprint, the clipboard, hidden codes and the master password; sync;
// backups and the export; the version and a check for a newer one.
// Every choice applies at once, as on the desktop.
import {
  AUTO_LOCK_CHOICES,
  CLIPBOARD_CHOICES,
  CODES_VIEWS,
  LOCALE_SETTINGS,
  SORT_ORDERS,
  storageSummary,
  syncStatusLine,
} from "@lockra/shared";
import {
  Icon,
  Segmented,
  Select,
  THEME_COLUMNS,
  ThemeTile,
  pickTheme,
  resolveTheme,
  systemPrefersDark,
  systemPrefersReducedMotion,
  themePair,
  useBackend,
  useGuarded,
  useI18n,
  useNow,
  useT,
  useUiState,
  useUpdateSettings,
} from "@lockra/ui";
import { useNav } from "../app/nav";
import { overPhoneScreen } from "../app/phone-screen";
import { ActionRow } from "../components/ActionRow";
import { FingerprintSetting } from "../components/FingerprintSetting";
import { Page } from "../components/Page";
import { Field, InfoRow, Section, SwitchRow } from "../components/Rows";
import { UpdateRow } from "../components/UpdateRow";

export function Settings() {
  const { t } = useI18n();
  const nav = useNav();
  const { settings, app_version: version } = useUiState();
  const update = useUpdateSettings();
  const { backend } = useBackend();
  const guarded = useGuarded();
  const pickRestore = async () => {
    if (await guarded(() => overPhoneScreen(() => backend.pickRestoreFile())))
      nav.open({ name: "restore" });
  };
  const systemDark = systemPrefersDark();
  const resolved = resolveTheme(settings, systemDark);
  const systemReduced = systemPrefersReducedMotion();
  const never = t("common.never");
  return (
    <Page title={t("settings.title")} testId="page-settings">
      <div className="flex flex-col gap-5">
        <Section title={t("settings.section.general")}>
          <Field label={t("settings.general.locale.label")}>
            <Segmented
              size="lg"
              label={t("settings.general.locale.label")}
              value={settings.locale}
              onChange={(value) => update({ locale: value })}
              options={LOCALE_SETTINGS.map((value) => ({
                value,
                label: t(`settings.general.locale.${value}`),
              }))}
              className="self-start"
            />
          </Field>
          <Field label={t("settings.general.sort")}>
            <Segmented
              size="lg"
              label={t("settings.general.sort")}
              value={settings.sort}
              onChange={(sort) => update({ sort })}
              options={SORT_ORDERS.map((value) => ({ value, label: t(`codes.sort.${value}`) }))}
              className="self-start"
            />
          </Field>
          <SwitchRow
            label={t("codes.groupView")}
            checked={settings.group_codes}
            onChange={(group_codes) => update({ group_codes })}
            testId="settings-groups"
          />
        </Section>
        <Section title={t("settings.section.appearance")}>
          <SwitchRow
            label={t("theme.followSystem")}
            checked={settings.follow_system_theme}
            onChange={(follow) =>
              update({ follow_system_theme: follow, theme: follow ? settings.theme : resolved })
            }
            testId="settings-follow-system"
          />
          <div className="flex flex-col gap-2 px-4 py-3">
            <div
              role="radiogroup"
              aria-label={t("settings.appearance.theme")}
              className="grid grid-cols-[148px_1fr_148px] items-start justify-center gap-y-3">
              <span className="text-[13px] text-fg-muted">{t("settings.appearance.light")}</span>
              <span />
              <span className="text-[13px] text-fg-muted">{t("settings.appearance.dark")}</span>
              {THEME_COLUMNS.map(([light, dark]) => {
                // Following the system: the pair it switches within, linked across the row.
                const paired = settings.follow_system_theme && themePair(resolved)[0] === light;
                return [
                  <ThemeTile
                    key={light}
                    theme={light}
                    selected={resolved === light}
                    caption={null}
                    onSelect={(theme) => update(pickTheme(settings, theme, systemDark))}
                  />,
                  <span
                    key={`${light}-pair`}
                    className="flex h-[104px] items-center justify-center">
                    {paired && (
                      <span
                        aria-hidden
                        data-testid="theme-pair"
                        data-column={light}
                        className="text-accent-text">
                        <Icon name="link" size={16} />
                      </span>
                    )}
                  </span>,
                  <ThemeTile
                    key={dark}
                    theme={dark}
                    selected={resolved === dark}
                    caption={null}
                    onSelect={(theme) => update(pickTheme(settings, theme, systemDark))}
                  />,
                ];
              })}
            </div>
            <span className="text-[13px] text-fg-muted">{t("settings.appearance.themeHint")}</span>
          </div>
          <Field label={t("codes.view.label")} hint={t("settings.appearance.codesViewHint")}>
            <Segmented
              size="lg"
              label={t("codes.view.label")}
              value={settings.codes_view}
              onChange={(codes_view) => update({ codes_view })}
              options={CODES_VIEWS.map((value) => ({ value, label: t(`codes.view.${value}`) }))}
              className="self-start"
            />
          </Field>
          <SwitchRow
            label={t("settings.appearance.reduceMotion")}
            checked={settings.reduce_motion || systemReduced}
            disabled={systemReduced}
            onChange={(reduce_motion) => update({ reduce_motion })}
          />
        </Section>
        <Section title={t("settings.section.security")}>
          <Field label={t("settings.security.autoLock")} hint={t("settings.security.autoLockHint")}>
            <Select
              size="lg"
              aria-label={t("settings.security.autoLock")}
              value={String(settings.auto_lock_minutes)}
              onChange={(value) => update({ auto_lock_minutes: Number(value) })}
              options={AUTO_LOCK_CHOICES.map((n) => ({
                value: String(n),
                label: n === 0 ? never : t("common.minutes", { n }),
              }))}
            />
          </Field>
          <Field
            label={t("settings.security.clipboard")}
            hint={t("settings.security.clipboardHint")}>
            <Select
              size="lg"
              aria-label={t("settings.security.clipboard")}
              value={String(settings.clipboard_clear_seconds)}
              onChange={(value) => update({ clipboard_clear_seconds: Number(value) })}
              options={CLIPBOARD_CHOICES.map((n) => ({
                value: String(n),
                label: n === 0 ? never : t("common.seconds", { n }),
              }))}
            />
          </Field>
          <FingerprintSetting />
          <SwitchRow
            label={t("settings.security.hideCodes")}
            hint={t("settings.security.hideCodesHint")}
            checked={settings.hide_codes}
            onChange={(hide_codes) => update({ hide_codes })}
            testId="settings-hide-codes"
          />
          <div className="p-1">
            <ActionRow
              icon="key"
              label={t("settings.security.changePassword")}
              opensPage
              onClick={() => nav.open({ name: "password" })}
              testId="settings-password"
            />
          </div>
        </Section>
        <Section title={t("settings.section.sync")}>
          <div className="p-1">
            <SyncRow />
          </div>
        </Section>
        <Section title={t("backup.title")}>
          <div className="p-1">
            <ActionRow
              icon="archive"
              label={t("backup.manual.title")}
              hint={t("backup.manual.save")}
              opensPage
              onClick={() => nav.open({ name: "backup" })}
              testId="settings-backup"
            />
            <ActionRow
              icon="download"
              label={t("backup.restore.title")}
              hint={t("backup.restore.pick")}
              onClick={() => void pickRestore()}
              testId="settings-restore"
            />
            <ActionRow
              icon="qr"
              label={t("export.title")}
              hint={t("export.subtitle")}
              opensPage
              onClick={() => nav.open({ name: "export" })}
              testId="settings-export"
            />
          </div>
        </Section>
        <Section title={t("settings.section.about")}>
          <InfoRow
            label={t("settings.about.version")}
            value={`Lockra ${version}`}
            testId="about-version"
          />
          <UpdateRow />
          <InfoRow label={t("settings.about.license")} value="Apache-2.0" />
        </Section>
      </div>
    </Page>
  );
}

/** Sync: off, the way to set it up; on, how the last run went and where the space is. */
function SyncRow() {
  const t = useT();
  const nav = useNav();
  const now = useNow();
  const { space } = useUiState().sync;
  return (
    <ActionRow
      icon="cloud"
      label={space === null ? t("mobile.sync.rowOff") : syncStatusLine(space.status, t, now).text}
      hint={
        space === null
          ? t("mobile.sync.settingsHint")
          : `${t(`sync.storage.${space.storage.kind}`)} · ${storageSummary(space.storage)}`
      }
      opensPage
      onClick={() => nav.open({ name: "sync" })}
      testId="settings-sync"
    />
  );
}
