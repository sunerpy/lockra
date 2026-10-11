// Restore the backup picked: merged account by account through the import preview, or in place of
// every account (the core keeps the current ones in a pre-restore backup first), with the
// backup's password. Leaving the page (its button or the back gesture) ends the restore (App.tsx).
import { type RestoreMode, type RestoreView, errorText, formatDateTime } from "@lockra/shared";
import { Button, PasswordField, Segmented, useBackend, useI18n, useSubmit } from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { Page } from "../components/Page";

export function Restore({ restore }: { restore: RestoreView }) {
  const { t, locale } = useI18n();
  const nav = useNav();
  const { backend } = useBackend();
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
    <Page title={t("backup.restore.title")} testId="page-restore">
      <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
        <p className="text-[14px] text-fg-muted">{t("backup.restore.body")}</p>
        <p className="mono text-[12px] break-all text-fg" data-testid="restore-file">
          {t("backup.restore.file", {
            name: restore.file_name,
            date: formatDateTime(locale, restore.created_at_ms, {
              dateStyle: "medium",
              timeStyle: "short",
            }),
          })}
        </p>
        <Segmented
          size="lg"
          label={t("backup.restore.mode")}
          value={mode}
          onChange={setMode}
          options={[
            { value: "merge", label: t("backup.restore.merge") },
            { value: "replace", label: t("backup.restore.replace") },
          ]}
          className="self-start"
        />
        <PasswordField
          size="lg"
          label={t("backup.restore.password")}
          value={password}
          onChange={setPassword}
          autoComplete="off"
          error={submit.error === undefined ? undefined : errorText(t, submit.error)}
        />
        <Button
          variant={mode === "replace" ? "danger" : "primary"}
          size="lg"
          type="submit"
          loading={submit.busy}
          disabled={password === ""}>
          {t("backup.restore.submit")}
        </Button>
        <Button variant="ghost" size="lg" onClick={nav.back}>
          {t("common.cancel")}
        </Button>
      </form>
    </Page>
  );
}
