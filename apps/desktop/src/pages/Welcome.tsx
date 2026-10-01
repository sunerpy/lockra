// No vault yet: create one with a master password, or restore a backup (its password becomes the
// master password).
import { errorText, formatDateTime } from "@lockra/shared";
import {
  Button,
  CardGrid,
  Logo,
  Panel,
  PasswordField,
  useBackend,
  useI18n,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useGuarded, useSubmit } from "../app/dispatch";
import { passwordLongEnough } from "../app/password";

export function Welcome() {
  const { t } = useI18n();
  return (
    <div
      className="mx-auto flex w-full max-w-[880px] flex-col gap-6 px-6 py-10"
      data-testid="page-welcome">
      <header className="flex items-center gap-4">
        <Logo size={48} />
        <div className="min-w-0">
          <h1 className="text-[20px] font-semibold text-fg">{t("welcome.title")}</h1>
          <p className="mt-1 text-[13px] text-fg-muted">{t("welcome.subtitle")}</p>
        </div>
      </header>
      <CardGrid min={320}>
        <CreateVault />
        <RestoreVault />
      </CardGrid>
    </div>
  );
}

function CreateVault() {
  const { t } = useI18n();
  const { backend } = useBackend();
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const submit = useSubmit();
  const mismatch = repeat !== "" && repeat !== password;
  const ready = passwordLongEnough(password) && repeat === password;
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    await submit.run(() => backend.dispatch({ command: "vault_create", password }));
  };
  return (
    <Panel eyebrow={t("welcome.create.title")} data-testid="welcome-create">
      <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
        <p className="text-[13px] text-fg-muted">{t("welcome.create.body")}</p>
        <PasswordField
          label={t("welcome.create.password")}
          value={password}
          onChange={setPassword}
          strength
          autoFocus
          autoComplete="new-password"
          help={t("welcome.create.hint")}
          error={submit.error === undefined ? undefined : errorText(t, submit.error)}
        />
        <PasswordField
          label={t("welcome.create.repeat")}
          value={repeat}
          onChange={setRepeat}
          autoComplete="new-password"
          error={mismatch ? t("welcome.create.mismatch") : undefined}
        />
        <Button
          variant="primary"
          type="submit"
          icon="lock"
          loading={submit.busy}
          disabled={!ready}
          className="self-start">
          {t("welcome.create.submit")}
        </Button>
      </form>
    </Panel>
  );
}

function RestoreVault() {
  const { t, locale } = useI18n();
  const { backend } = useBackend();
  const { restore } = useUiState();
  const guarded = useGuarded();
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    await submit.run(() =>
      backend.dispatch({ command: "restore_commit", password, mode: "replace" }),
    );
    setPassword("");
  };
  return (
    <Panel eyebrow={t("welcome.restore.title")} data-testid="welcome-restore">
      <div className="flex flex-col gap-3">
        <p className="text-[13px] text-fg-muted">{t("welcome.restore.body")}</p>
        {restore === null ? (
          <Button
            icon="folder"
            className="self-start"
            onClick={() => void guarded(() => backend.pickRestoreFile())}>
            {t("welcome.restore.pick")}
          </Button>
        ) : (
          <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
            <p
              className="mono truncate text-[12px] text-fg"
              title={restore.file_name}
              data-testid="restore-file">
              {t("welcome.restore.file", {
                name: restore.file_name,
                date: formatDateTime(locale, restore.created_at_ms, {
                  dateStyle: "medium",
                  timeStyle: "short",
                }),
              })}
            </p>
            <PasswordField
              label={t("welcome.restore.password")}
              value={password}
              onChange={setPassword}
              autoFocus
              autoComplete="off"
              error={submit.error === undefined ? undefined : errorText(t, submit.error)}
            />
            <div className="flex items-center gap-2">
              <Button
                variant="primary"
                type="submit"
                loading={submit.busy}
                disabled={password === ""}>
                {t("welcome.restore.submit")}
              </Button>
              <Button
                variant="ghost"
                onClick={() =>
                  void backend.dispatch({ command: "restore_cancel" }).catch(() => undefined)
                }>
                {t("common.cancel")}
              </Button>
            </div>
          </form>
        )}
      </div>
    </Panel>
  );
}
