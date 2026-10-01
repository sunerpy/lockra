import {
  errorText,
  formatBytes,
  formatDateTime,
  relativeTime,
  releaseNotesLines,
} from "@lockra/shared";
import {
  Button,
  Progress,
  SettingsPane,
  SettingsRows,
  StatusRow,
  Toggle,
  useBackend,
  useLocale,
  useNow,
  useT,
  useUiState,
} from "@lockra/ui";
import type { ReactNode } from "react";
import { useSubmit } from "../../app/dispatch";
import { useUpdateSettings } from "../../app/settings";

/** Settings › About: version, the in-app update, where the data lives, licences and credits. */
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
        <Update />
        <AutoCheck />
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

/** The in-app update: what the updater is doing, and the one action that fits it. Lockra goes
 *  online here only when asked (or when automatic checks are on). */
function Update() {
  const t = useT();
  const locale = useLocale();
  const now = useNow();
  const { backend } = useBackend();
  const { update } = useUiState();
  const action = useSubmit();
  const { method, status } = update;
  const label = t("settings.about.update.label");
  if (method === null) {
    return (
      <StatusRow label={label} help={t("settings.about.update.unavailable")} data-testid="update" />
    );
  }
  const check = () => void action.run(() => backend.dispatch({ command: "update_check" }));
  const install = () => void action.run(() => backend.dispatch({ command: "update_install" }));
  const checkButton = (text: string) => (
    <Button
      size="sm"
      variant="outline"
      onClick={check}
      loading={action.busy}
      data-testid="update-check">
      {text}
    </Button>
  );
  const rowView = (): RowView => {
    switch (status.state) {
      case "idle":
        return {
          help: t("settings.about.update.idle"),
          control: checkButton(t("settings.about.update.check")),
        };
      case "checking":
        return {
          help: t("settings.about.update.checking"),
          control: (
            <Button size="sm" variant="outline" disabled loading data-testid="update-check">
              {t("settings.about.update.check")}
            </Button>
          ),
        };
      case "up_to_date":
        return {
          help: t("settings.about.update.upToDate"),
          control: checkButton(t("settings.about.update.check")),
          note: t("settings.about.update.checkedAt", {
            when: relativeTime(t, status.checked_at_ms, now),
          }),
        };
      case "available": {
        const available = t("settings.about.update.available", { version: status.version });
        const published = status.date === null ? Number.NaN : Date.parse(status.date);
        return {
          help: Number.isNaN(published)
            ? available
            : `${available} · ${t("settings.about.update.publishedOn", {
                date: formatDateTime(locale, published, { dateStyle: "medium" }),
              })}`,
          control: (
            <Button
              size="sm"
              variant="primary"
              onClick={install}
              loading={action.busy}
              data-testid="update-install">
              {t("settings.about.update.install")}
            </Button>
          ),
          note: t(`settings.about.update.method.${method}`),
        };
      }
      case "downloading": {
        const known = status.total !== null && status.total > 0;
        return {
          help: known
            ? t("settings.about.update.downloading", {
                version: status.version,
                percent: Math.min(100, Math.floor((status.received / (status.total ?? 1)) * 100)),
              })
            : t("settings.about.update.downloadingSize", {
                version: status.version,
                received: formatBytes(status.received),
              }),
          control: (
            <Progress
              value={known ? status.received / (status.total ?? 1) : undefined}
              indeterminate={!known}
              className="w-40"
              label={label}
            />
          ),
        };
      }
      case "installing":
        return {
          help: t("settings.about.update.installing", { version: status.version }),
          control: <Progress indeterminate className="w-40" label={label} />,
        };
      case "failed":
        return {
          help: (
            <span className="text-danger">
              {t("settings.about.update.failed", { error: errorText(t, status.code) })}
            </span>
          ),
          control: checkButton(t("settings.about.update.retry")),
        };
    }
  };
  const rejected =
    action.error === undefined ? undefined : (
      <span className="text-danger">{errorText(t, action.error)}</span>
    );
  const view = rowView();
  return (
    <>
      <StatusRow label={label} help={view.help} note={rejected ?? view.note} data-testid="update">
        {view.control}
      </StatusRow>
      {status.state === "available" && status.notes !== null && (
        <ReleaseNotes notes={status.notes} />
      )}
    </>
  );
}

interface RowView {
  help: ReactNode;
  control?: ReactNode;
  note?: ReactNode;
}

/** The release's notes, as text (never HTML). */
function ReleaseNotes({ notes }: { notes: string }) {
  const t = useT();
  const lines = releaseNotesLines(notes);
  if (lines.length === 0) return null;
  return (
    <div className="border-b border-border py-3" data-testid="update-notes">
      <p className="text-[12px] font-medium text-fg">{t("settings.about.update.notes")}</p>
      <div className="mt-1 flex flex-col text-[12px] leading-5 text-fg-muted">
        {lines.map((line, index) =>
          line === "" ? (
            <span key={index} className="h-2" aria-hidden="true" />
          ) : (
            <span key={index}>{line}</span>
          ),
        )}
      </div>
    </div>
  );
}

/** "Check for updates automatically": off unless turned on; only for a copy that can update. */
function AutoCheck() {
  const t = useT();
  const { settings, update } = useUiState();
  const updateSettings = useUpdateSettings();
  if (update.method === null) return null;
  return (
    <StatusRow
      label={t("settings.about.update.auto")}
      help={
        <>
          {t("settings.about.update.autoHint")} {t("settings.about.update.network")}
        </>
      }
      data-testid="update-auto">
      <Toggle
        checked={settings.auto_check_updates}
        onChange={(auto_check_updates) => updateSettings({ auto_check_updates })}
        ariaLabel={t("settings.about.update.auto")}
      />
    </StatusRow>
  );
}
