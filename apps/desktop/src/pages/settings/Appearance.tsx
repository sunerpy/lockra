import { CODES_VIEWS, themeName } from "@lockra/shared";
import {
  ACCENT_IDS,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  Icon,
  IconButton,
  Segmented,
  SettingsPane,
  SettingsRows,
  SettingsSection,
  StatusRow,
  THEME_COLUMNS,
  ThemeTile,
  Toggle,
  pickTheme,
  resolveTheme,
  systemPrefersDark,
  systemPrefersReducedMotion,
  themePair,
  useI18n,
  useUiState,
} from "@lockra/ui";
import { useUpdateSettings } from "../../app/settings";

/** Settings › Appearance: the eight themes, light above dark with a column per pair (following the
 *  system switches within the column, 暖纸 and 石墨 stand alone), the code view, the accent,
 *  density, font size and motion. Every choice applies at once. */
export function Appearance() {
  const { t, locale } = useI18n();
  const { settings } = useUiState();
  const update = useUpdateSettings();
  const systemDark = systemPrefersDark();
  const resolved = resolveTheme(settings, systemDark);
  const systemReduced = systemPrefersReducedMotion();
  // Following the system: the column it switches within, linked between its two tiles.
  const pair = settings.follow_system_theme
    ? THEME_COLUMNS.findIndex(([light]) => light === themePair(resolved)[0])
    : -1;
  return (
    <SettingsPane title={t("settings.section.appearance")} lede={t("settings.appearance.lede")}>
      <SettingsSection
        title={t("settings.appearance.theme")}
        aside={
          settings.follow_system_theme ? (
            <span className="mono text-fg-subtle">
              {t("settings.appearance.following", { theme: themeName(resolved, locale) })}
            </span>
          ) : undefined
        }>
        <div
          role="radiogroup"
          aria-label={t("settings.appearance.theme")}
          className="grid grid-cols-[repeat(4,148px)] gap-x-4"
          data-testid="theme-grid">
          <span className="col-span-4 row-start-1 pb-1.5 text-[11px] text-fg-subtle">
            {t("settings.appearance.light")}
          </span>
          <span className="col-start-1 row-start-3 flex h-8 items-end pb-1.5 text-[11px] text-fg-subtle">
            {t("settings.appearance.dark")}
          </span>
          {pair >= 0 && (
            <span
              aria-hidden
              data-testid="theme-pair"
              data-column={THEME_COLUMNS[pair]?.[0]}
              className="row-start-3 flex h-8 items-center justify-self-center text-accent-text"
              style={{ gridColumnStart: pair + 1 }}>
              <Icon name="link" size={14} />
            </span>
          )}
          {([0, 1] as const).map((row) =>
            THEME_COLUMNS.map((column, at) => {
              const id = column[row];
              return (
                <div key={id} style={{ gridColumnStart: at + 1, gridRowStart: row === 0 ? 2 : 4 }}>
                  <ThemeTile
                    theme={id}
                    selected={resolved === id}
                    caption={null}
                    onSelect={(theme) => update(pickTheme(settings, theme, systemDark))}
                  />
                </div>
              );
            }),
          )}
        </div>
        <p className="mt-3 text-[12px] leading-5 text-fg-muted">
          {t("settings.appearance.themeHint")}
        </p>
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
          label={t("codes.view.label")}
          help={t("settings.appearance.codesViewHint")}
          data-testid="settings-codes-view">
          <Segmented
            label={t("codes.view.label")}
            value={settings.codes_view}
            onChange={(codes_view) => update({ codes_view })}
            options={CODES_VIEWS.map((value) => ({ value, label: t(`codes.view.${value}`) }))}
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
