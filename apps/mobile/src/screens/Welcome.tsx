// No vault yet: create one with a master password, or restore a backup made elsewhere (the
// desktop, another phone), whose password becomes the master password. Joining a sync space comes
// with the phone's sync.
import { errorText, formatDateTime, passwordLongEnough } from "@lockra/shared";
import {
  Button,
  Logo,
  PasswordField,
  useBackend,
  useDispatch,
  useGuarded,
  useI18n,
  useSubmit,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { overPhoneScreen } from "../app/phone-screen";

export function Welcome() {
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
    <main
      className="flex min-h-full flex-col gap-8 px-5 pt-[max(env(safe-area-inset-top),3rem)] pb-[max(env(safe-area-inset-bottom),1.5rem)]"
      data-testid="page-welcome">
      <header className="flex flex-col items-center gap-3 text-center">
        <Logo size={56} />
        <h1 className="text-[22px] font-semibold text-fg">{t("welcome.title")}</h1>
        <p className="text-[14px] text-fg-muted">{t("welcome.subtitle")}</p>
      </header>
      <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
        <h2 className="text-[16px] font-medium text-fg">{t("welcome.create.title")}</h2>
        <p className="text-[14px] text-fg-muted">{t("welcome.create.body")}</p>
        <PasswordField
          label={t("welcome.create.password")}
          value={password}
          onChange={setPassword}
          size="lg"
          strength
          autoComplete="new-password"
          help={t("welcome.create.hint")}
          error={submit.error === undefined ? undefined : errorText(t, submit.error)}
        />
        <PasswordField
          label={t("welcome.create.repeat")}
          value={repeat}
          onChange={setRepeat}
          size="lg"
          autoComplete="new-password"
          error={mismatch ? t("welcome.create.mismatch") : undefined}
        />
        <Button
          variant="primary"
          type="submit"
          size="lg"
          icon="lock"
          loading={submit.busy}
          disabled={!ready}>
          {t("welcome.create.submit")}
        </Button>
      </form>
      <RestoreBackup />
    </main>
  );
}

function RestoreBackup() {
  const { t, locale } = useI18n();
  const { backend, state } = useBackend();
  const guarded = useGuarded();
  const dispatch = useDispatch();
  const restore = state?.restore ?? null;
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
    <section className="flex flex-col gap-4" data-testid="welcome-restore">
      <h2 className="text-[16px] font-medium text-fg">{t("welcome.restore.title")}</h2>
      <p className="text-[14px] text-fg-muted">{t("welcome.restore.body")}</p>
      {restore === null ? (
        <Button
          size="lg"
          icon="folder"
          onClick={() => void guarded(() => overPhoneScreen(() => backend.pickRestoreFile()))}>
          {t("welcome.restore.pick")}
        </Button>
      ) : (
        <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
          <p className="mono text-[12px] break-all text-fg" data-testid="restore-file">
            {t("welcome.restore.file", {
              name: restore.file_name,
              date: formatDateTime(locale, restore.created_at_ms, {
                dateStyle: "medium",
                timeStyle: "short",
              }),
            })}
          </p>
          <PasswordField
            size="lg"
            label={t("welcome.restore.password")}
            value={password}
            onChange={setPassword}
            autoComplete="off"
            error={submit.error === undefined ? undefined : errorText(t, submit.error)}
          />
          <Button
            variant="primary"
            size="lg"
            type="submit"
            loading={submit.busy}
            disabled={password === ""}>
            {t("welcome.restore.submit")}
          </Button>
          <Button
            variant="ghost"
            size="lg"
            onClick={() => void dispatch({ command: "restore_cancel" })}>
            {t("common.cancel")}
          </Button>
        </form>
      )}
    </section>
  );
}
