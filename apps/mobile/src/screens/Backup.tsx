// Save a backup where the user picks: the whole vault encrypted into a .lockrabackup file, under
// the master password or a backup password of its own. Saved, the settings again (the core's
// notice says where).
import { errorText, passwordLongEnough, relativeTime } from "@lockra/shared";
import {
  Button,
  Card,
  PasswordField,
  useBackend,
  useI18n,
  useNow,
  useSubmit,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { overPhoneScreen } from "../app/phone-screen";
import { Page } from "../components/Page";
import { SwitchRow } from "../components/Rows";

export function Backup() {
  const { t } = useI18n();
  const nav = useNav();
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
    const name = await submit.run(() =>
      overPhoneScreen(() => backend.saveBackup(separate ? password : undefined)),
    );
    if (typeof name === "string") nav.back();
  };
  return (
    <Page title={t("backup.manual.title")} testId="page-backup">
      <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
        <p className="text-[14px] text-fg-muted">{t("backup.manual.body")}</p>
        {backup.last_backup_ms !== null && (
          <p className="mono text-[12px] text-fg-subtle" data-testid="last-backup">
            {t("backup.manual.last", { when: relativeTime(t, backup.last_backup_ms, now) })}
          </p>
        )}
        <Card padding="none">
          <SwitchRow
            label={t("backup.manual.separate")}
            checked={separate}
            onChange={setSeparate}
            testId="backup-separate"
          />
        </Card>
        {separate && (
          <>
            <PasswordField
              size="lg"
              label={t("backup.manual.password")}
              value={password}
              onChange={setPassword}
              strength
              autoComplete="new-password"
            />
            <PasswordField
              size="lg"
              label={t("backup.manual.repeat")}
              value={repeat}
              onChange={setRepeat}
              autoComplete="new-password"
              error={mismatch ? t("welcome.create.mismatch") : undefined}
            />
          </>
        )}
        {submit.error !== undefined && (
          <p role="alert" className="text-[13px] text-danger">
            {errorText(t, submit.error)}
          </p>
        )}
        <Button
          variant="primary"
          size="lg"
          type="submit"
          icon="archive"
          loading={submit.busy}
          disabled={!ready}
          data-testid="backup-save">
          {t("backup.manual.save")}
        </Button>
      </form>
    </Page>
  );
}
