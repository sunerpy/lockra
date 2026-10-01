import { THEME_IDS, themeName } from "@lockra/shared";
import {
  ACCENT_IDS,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  IconButton,
  Segmented,
  SettingsPane,
  SettingsRows,
  SettingsSection,
  StatusRow,
  ThemeTile,
  Toggle,
  resolveTheme,
  systemPrefersDark,
  systemPrefersReducedMotion,
  useI18n,
  useUiState,
} from "@lockra/ui";
import { useUpdateSettings } from "../../app/settings";

/** Settings › Appearance: the four themes (or the system's), the accent, density, font size and
 *  motion. Every choice applies at once. */
export function Appearance() {
  const { t, locale } = useI18n();
  const { settings } = useUiState();
  const update = useUpdateSettings();
  const resolved = resolveTheme(settings, systemPrefersDark());
  const systemReduced = systemPrefersReducedMotion();
  return (
    <SettingsPane title={t("settings.section.appearance")} lede={t("settings.appearance.lede")}>
      <SettingsSection
        title={t("settings.appearance.theme")}
        aside={
          settings.follow_system_theme ? (
            <span className="mono text-fg-subtle">
              {t("theme.followSystem")} · {themeName(resolved, locale)}
            </span>
          ) : undefined
        }>
        <div
          role="radiogroup"
          aria-label={t("settings.appearance.theme")}
          className="flex flex-wrap gap-4">
          {THEME_IDS.map((id) => (
            <ThemeTile
              key={id}
              theme={id}
              selected={resolved === id}
              disabled={settings.follow_system_theme}
              caption={id}
              onSelect={(theme) => update({ theme, follow_system_theme: false })}
            />
          ))}
        </div>
      </SettingsSection>
      <SettingsRows>
        <StatusRow label={t("theme.followSystem")} help={t("settings.appearance.followSystemHint")}>
          <Toggle
            checked={settings.follow_system_theme}
            onChange={(follow) =>
              update({ follow_system_theme: follow, theme: follow ? settings.theme : resolved })
            }
            ariaLabel={t("theme.followSystem")}
          />
        </StatusRow>
        <StatusRow
          label={t("settings.appearance.accent")}
          help={t("settings.appearance.accentHint")}
          data-testid="accent-row">
          {/* Each swatch paints the accent that choice gives the current theme, through the tokens. */}
          <div
            role="radiogroup"
            aria-label={t("settings.appearance.accent")}
            className="flex flex-wrap justify-end gap-1.5">
            {ACCENT_IDS.map((id) => {
              const chosen = settings.accent === id;
              const name = t(`accent.${id}`);
              return (
                <button
                  key={id}
                  type="button"
                  role="radio"
                  aria-checked={chosen}
                  aria-label={name}
                  title={name}
                  data-accent={id}
                  data-theme={resolved}
                  onClick={() => update({ accent: id })}
                  className={`flex h-7 w-7 items-center justify-center rounded-full bg-transparent ${chosen ? "hairline border-fg" : ""}`}>
                  <span className="h-[18px] w-[18px] rounded-full bg-accent" aria-hidden />
                </button>
              );
            })}
          </div>
        </StatusRow>
        <StatusRow
          label={t("settings.appearance.density")}
          help={t("settings.appearance.densityHint")}>
          <Segmented
            label={t("settings.appearance.density")}
            value={settings.density}
            onChange={(density) => update({ density })}
            options={[
              { value: "compact", label: t("settings.appearance.densityCompact") },
              { value: "default", label: t("settings.appearance.densityDefault") },
            ]}
          />
        </StatusRow>
        <StatusRow
          label={t("settings.appearance.fontSize")}
          help={t("settings.appearance.fontSizeHint")}>
          <div className="flex items-center gap-1 rounded-6 bg-surface p-0.5 hairline">
            <IconButton
              icon="minus"
              label={t("settings.appearance.fontSmaller")}
              disabled={settings.font_size_px <= FONT_SIZE_MIN}
              onClick={() => update({ font_size_px: settings.font_size_px - 1 })}
            />
            <span className="mono w-14 text-center text-[13px] text-fg" data-testid="font-size">
              {settings.font_size_px} px
            </span>
            <IconButton
              icon="plus"
              label={t("settings.appearance.fontLarger")}
              disabled={settings.font_size_px >= FONT_SIZE_MAX}
              onClick={() => update({ font_size_px: settings.font_size_px + 1 })}
            />
          </div>
        </StatusRow>
        <StatusRow
          label={t("settings.appearance.reduceMotion")}
          help={t("settings.appearance.reduceMotionHint")}
          note={systemReduced ? t("settings.appearance.systemReduced") : undefined}>
          <Toggle
            checked={settings.reduce_motion || systemReduced}
            disabled={systemReduced}
            onChange={(reduce_motion) => update({ reduce_motion })}
            ariaLabel={t("settings.appearance.reduceMotion")}
          />
        </StatusRow>
      </SettingsRows>
    </SettingsPane>
  );
}
