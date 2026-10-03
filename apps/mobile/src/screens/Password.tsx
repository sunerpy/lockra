// Change the master password: the current one, then the new one twice (the desktop's Settings ›
// Security form); done, the settings again with a toast.
import { errorText, passwordLongEnough } from "@lockra/shared";
import { Button, PasswordField, useBackend, useSubmit, useT, useToaster } from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { Page } from "../components/Page";

export function Password() {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const toaster = useToaster();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [repeat, setRepeat] = useState("");
  const submit = useSubmit();
  const mismatch = repeat !== "" && repeat !== next;
  const ready = current !== "" && passwordLongEnough(next) && repeat === next;
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const done = await submit.run(() =>
      backend.dispatch({ command: "vault_change_password", current, new: next }),
    );
    setCurrent("");
    if (done === undefined) return;
    toaster.info(t("settings.security.changed"));
    nav.back();
  };
  const error = submit.error === undefined ? undefined : errorText(t, submit.error);
  return (
    <Page title={t("settings.security.changePassword")} testId="page-password">
      <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
        <p className="text-[14px] text-fg-muted">{t("settings.security.changePasswordHint")}</p>
        <PasswordField
          size="lg"
          label={t("settings.security.current")}
          value={current}
          onChange={setCurrent}
          autoComplete="current-password"
          error={submit.error === "wrong_password" ? error : undefined}
        />
        <PasswordField
          size="lg"
          label={t("settings.security.newPassword")}
          value={next}
          onChange={setNext}
          autoComplete="new-password"
          strength
          help={t("welcome.create.hint")}
        />
        <PasswordField
          size="lg"
          label={t("welcome.create.repeat")}
          value={repeat}
          onChange={setRepeat}
          autoComplete="new-password"
          error={mismatch ? t("welcome.create.mismatch") : undefined}
        />
        {submit.error !== undefined && submit.error !== "wrong_password" && (
          <p role="alert" className="text-[13px] text-danger">
            {error}
          </p>
        )}
        <Button variant="primary" size="lg" type="submit" loading={submit.busy} disabled={!ready}>
          {t("settings.security.changePassword")}
        </Button>
      </form>
    </Page>
  );
}
