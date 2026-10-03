// Encrypted backups: one now (with the master password or a separate one), automatic ones into a
// folder the user picks (keeping the newest few), and restoring one (merged through the import
// preview, or replacing every account after the current vault is set aside).
import {
  errorText,
  formatDateTime,
  KEEP_CHOICES,
  passwordLongEnough,
  relativeTime,
  type RestoreMode,
} from "@lockra/shared";
import {
  Banner,
  Button,
  Lamp,
  Panel,
  PasswordField,
  Segmented,
  Select,
  SettingsRows,
  StatusRow,
  Toggle,
  useBackend,
  useI18n,
  useNow,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useDispatch, useGuarded, useSubmit } from "../app/dispatch";

export function Backup() {
  const { t } = useI18n();
  return (
    <div className="flex flex-col gap-4" data-testid="page-backup">
      <p className="text-[13px] text-fg-muted">{t("backup.subtitle")}</p>
      <ManualBackup />
      <AutoBackup />
      <RestoreBackup />
    </div>
  );
}

function ManualBackup() {
  const { t } = useI18n();
  const { backend } = useBackend();
  const { backup } = useUiState();
  const now = useNow();
  const [separate, setSeparate] = useState(false);
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const submit = useSubmit();
  const mismatch = repeat !== "" && repeat !== password;
  const ready = !separate || (passwordLongEnough(password) && repeat === password);
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const name = await submit.run(() => backend.saveBackup(separate ? password : undefined));
    if (name !== undefined && name !== null) {
      setPassword("");
      setRepeat("");
    }
  };
  return (
    <Panel
      eyebrow={t("backup.manual.title")}
      right={
        backup.last_backup_ms === null ? undefined : (
          <span className="mono text-fg-subtle" data-testid="last-backup">
            {t("backup.manual.last", { when: relativeTime(t, backup.last_backup_ms, now) })}
          </span>
        )
      }
      data-testid="backup-manual">
      <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
        <p className="text-[12px] leading-[18px] text-fg-muted">{t("backup.manual.body")}</p>
        <Toggle checked={separate} onChange={setSeparate} label={t("backup.manual.separate")} />
        {separate && (
          <div className="grid gap-3 sm:grid-cols-2">
            <PasswordField
              label={t("backup.manual.password")}
              value={password}
              onChange={setPassword}
              strength
              autoComplete="new-password"
            />
            <PasswordField
              label={t("backup.manual.repeat")}
              value={repeat}
              onChange={setRepeat}
              autoComplete="new-password"
              error={mismatch ? t("welcome.create.mismatch") : undefined}
            />
          </div>
        )}
        {submit.error !== undefined && (
          <p role="alert" className="text-[12px] text-danger">
            {errorText(t, submit.error)}
          </p>
        )}
        <Button
          variant="primary"
          type="submit"
          icon="archive"
          loading={submit.busy}
          disabled={!ready}
          className="self-start"
          data-testid="backup-save">
          {t("backup.manual.save")}
        </Button>
      </form>
    </Panel>
  );
}

function AutoBackup() {
  const { t } = useI18n();
  const { backend } = useBackend();
  const { settings, backup } = useUiState();
  const dispatch = useDispatch();
  const guarded = useGuarded();
  const now = useNow();
  const auto = settings.auto_backup;
  const save = (next: Partial<typeof auto>) =>
    dispatch({
      command: "settings_set",
      settings: { ...settings, auto_backup: { ...auto, ...next } },
    });
  const setEnabled = async (enabled: boolean) => {
    if (enabled && auto.dir === null) {
      // No folder yet: choose one first; the state may not carry it yet, so it goes in explicitly.
      const dir = await guarded(() => backend.pickBackupDir());
      if (dir !== undefined && dir !== null) await save({ enabled: true, dir });
      return;
    }
    await save({ enabled });
  };
  return (
    <Panel
      eyebrow={t("backup.auto.title")}
      right={
        <Toggle
          checked={auto.enabled}
          onChange={(on) => void setEnabled(on)}
          ariaLabel={t("backup.auto.enabled")}
        />
      }
      data-testid="backup-auto">
      <div className="flex flex-col gap-3">
        <p className="text-[12px] leading-[18px] text-fg-muted">{t("backup.auto.body")}</p>
        <SettingsRows>
          <StatusRow
            label={t("backup.auto.folder")}
            note={
              <span className="mono break-all" data-testid="backup-dir">
                {auto.dir ?? t("backup.auto.noFolder")}
              </span>
            }>
            <Button
              size="sm"
              icon="folder"
              onClick={() => void guarded(() => backend.pickBackupDir())}>
              {t("backup.auto.pick")}
            </Button>
          </StatusRow>
          <StatusRow label={t("backup.auto.keep")}>
            <Select
              size="sm"
              aria-label={t("backup.auto.keep")}
              value={String(auto.keep)}
              onChange={(keep) => void save({ keep: Number(keep) })}
              options={KEEP_CHOICES.map((n) => ({
                value: String(n),
                label: t("backup.auto.keepCount", { n }),
              }))}
            />
          </StatusRow>
        </SettingsRows>
        {backup.last_auto_error !== null ? (
          <div data-testid="auto-error">
            <Banner tone="danger" marker="bar">
              {t("backup.auto.failed", {
                when: relativeTime(t, backup.last_auto_error.at_ms, now),
                error: errorText(t, backup.last_auto_error.code),
              })}
            </Banner>
          </div>
        ) : (
          backup.last_auto_file !== null &&
          backup.last_backup_ms !== null && (
            <p
              className="flex items-center gap-2 text-[12px] text-fg-muted"
              data-testid="auto-last">
              <Lamp tone="ok" />
              <span className="min-w-0 truncate">
                {t("backup.auto.last", {
                  when: relativeTime(t, backup.last_backup_ms, now),
                  file: backup.last_auto_file,
                })}
              </span>
            </p>
          )
        )}
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="text-[12px] text-fg-subtle">{t("backup.auto.passwordNote")}</p>
          <Button
            size="sm"
            icon="refresh"
            disabled={auto.dir === null}
            onClick={() => void dispatch({ command: "backup_auto_now" })}>
            {t("backup.auto.now")}
          </Button>
        </div>
      </div>
    </Panel>
  );
}

function RestoreBackup() {
  const { t, locale } = useI18n();
  const { backend } = useBackend();
  const { restore } = useUiState();
  const guarded = useGuarded();
  const dispatch = useDispatch();
  const [password, setPassword] = useState("");
  const [mode, setMode] = useState<RestoreMode>("merge");
  const submit = useSubmit();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    await submit.run(() => backend.dispatch({ command: "restore_commit", password, mode }));
    setPassword("");
  };
  return (
    <Panel eyebrow={t("backup.restore.title")} data-testid="backup-restore">
      <div className="flex flex-col gap-3">
        <p className="text-[12px] leading-[18px] text-fg-muted">{t("backup.restore.body")}</p>
        {restore === null ? (
          <Button
            icon="folder"
            className="self-start"
            onClick={() => void guarded(() => backend.pickRestoreFile())}>
            {t("backup.restore.pick")}
          </Button>
        ) : (
          <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
            <p
              className="mono truncate text-[12px] text-fg"
              title={restore.file_name}
              data-testid="restore-file">
              {t("backup.restore.file", {
                name: restore.file_name,
                date: formatDateTime(locale, restore.created_at_ms, {
                  dateStyle: "medium",
                  timeStyle: "short",
                }),
              })}
            </p>
            <Segmented
              label={t("backup.restore.mode")}
              value={mode}
              onChange={setMode}
              options={[
                { value: "merge", label: t("backup.restore.merge") },
                { value: "replace", label: t("backup.restore.replace") },
              ]}
            />
            <PasswordField
              label={t("backup.restore.password")}
              value={password}
              onChange={setPassword}
              autoFocus
              autoComplete="off"
              className="max-w-[24rem]"
              error={submit.error === undefined ? undefined : errorText(t, submit.error)}
            />
            <div className="flex items-center gap-2">
              <Button
                variant={mode === "replace" ? "danger" : "primary"}
                type="submit"
                loading={submit.busy}
                disabled={password === ""}>
                {t("backup.restore.submit")}
              </Button>
              <Button variant="ghost" onClick={() => void dispatch({ command: "restore_cancel" })}>
                {t("common.cancel")}
              </Button>
            </div>
          </form>
        )}
      </div>
    </Panel>
  );
}
