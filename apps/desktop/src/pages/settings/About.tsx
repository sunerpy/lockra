import { SettingsPane, SettingsRows, StatusRow, useT, useUiState } from "@lockra/ui";

/** Settings › About: version, where the data lives, licences and credits. */
export function About() {
  const t = useT();
  const { app_version: version, data_dir: dataDir } = useUiState();
  return (
    <SettingsPane title={t("settings.section.about")} lede={t("settings.about.privacy")}>
      <SettingsRows>
        <StatusRow label={t("settings.about.version")}>
          <span className="mono text-[13px] text-fg" data-testid="about-version">
            Lockra {version}
          </span>
        </StatusRow>
        <StatusRow
          label={t("settings.about.dataDir")}
          note={
            <span className="mono break-all select-all" data-testid="about-data-dir">
              {dataDir}
            </span>
          }
        />
        <StatusRow label={t("settings.about.license")}>
          <span className="mono text-[13px] text-fg">Apache-2.0</span>
        </StatusRow>
        <StatusRow label={t("settings.about.fonts")} help={t("settings.about.fontsValue")} />
        <StatusRow label={t("settings.about.credits")} help={t("settings.about.creditsValue")} />
      </SettingsRows>
    </SettingsPane>
  );
}
