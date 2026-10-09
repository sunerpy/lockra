// The space's recovery key (its sync key), once the user proved to be here (the fingerprint that
// unlocks this vault, or the master password): it hides after REVEAL_SECONDS like any secret
// (app/secret-page.ts). "Save to a file" uses the password typed to show it, or the fingerprint
// again; "I have kept it" says the key is written down, which ends Settings › Sync's reminder.
import { errorText } from "@lockra/shared";
import {
  Banner,
  Button,
  PasswordField,
  unlockBiometric,
  useBackend,
  useSaveSyncKey,
  useSubmit,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { useSecretAnswer, useSecretPage } from "../app/secret-page";
import { Page } from "../components/Page";
import { SyncKeyText } from "../components/SyncKeyText";

export function SyncKey() {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const { lock } = useUiState();
  const biometric = unlockBiometric(lock);
  const [password, setPassword] = useState("");
  const [shown, setShown] = useState<
    { syncKey: string; password: string | undefined; at: number } | undefined
  >(undefined);
  const submit = useSubmit();
  const deliver = useSecretAnswer();
  const left = useSecretPage(shown?.at);
  const saving = useSaveSyncKey();
  const [saved, setSaved] = useState(false);
  const ask = async (typed?: string) => {
    const answer = await submit.run(() =>
      backend.dispatch(
        typed === undefined
          ? { command: "sync_key_reveal", reason: t("sync.recoveryKey.reason") }
          : { command: "sync_key_reveal", password: typed },
      ),
    );
    setPassword("");
    if (answer !== undefined)
      deliver(() => setShown({ syncKey: answer.sync_key, password: typed, at: Date.now() }));
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    await ask(password);
  };
  if (shown === undefined) {
    return (
      <Page title={t("sync.recoveryKey.title")} testId="page-sync-key">
        <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
          <p className="text-[14px] text-fg">
            {t(biometric === null ? "sync.recoveryKey.prompt" : "sync.recoveryKey.promptBiometric")}
          </p>
          {biometric !== null && (
            <Button
              variant="outline"
              size="lg"
              icon="fingerprint"
              loading={submit.busy}
              onClick={() => void ask()}>
              {t(`sync.invite.verifyWith.${biometric}`)}
            </Button>
          )}
          <PasswordField
            size="lg"
            label={t("sync.masterPassword")}
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            error={submit.error === undefined ? undefined : errorText(t, submit.error)}
          />
          <Button
            variant="primary"
            size="lg"
            type="submit"
            icon="key"
            loading={submit.busy}
            disabled={password === ""}>
            {t("sync.recoveryKey.submit")}
          </Button>
        </form>
      </Page>
    );
  }
  const save = async () => {
    if ((await saving.save(shown.password)) === true) setSaved(true);
  };
  const kept = async () => {
    await backend.dispatch({ command: "sync_key_acknowledge" }).catch(() => undefined);
    nav.back();
  };
  return (
    <Page title={t("sync.recoveryKey.title")} testId="page-sync-key">
      <div className="flex flex-col gap-4">
        <Banner tone="warn" marker="icon">
          {t("sync.recoveryKey.body")}
        </Banner>
        <div className="flex flex-col gap-1.5">
          <span className="text-[13px] text-fg-muted">{t("sync.recoveryKey.key")}</span>
          <SyncKeyText value={shown.syncKey} />
        </div>
        <p className="text-[13px] text-fg-subtle" data-testid="sync-key-countdown">
          {t("sync.recoveryKey.hideIn", { s: left })}
        </p>
        {saved && (
          <p className="text-[13px] text-ok" role="status" data-testid="sync-key-saved">
            {t("sync.recoveryKey.savedTo")}
          </p>
        )}
        {saving.error !== undefined && (
          <p className="text-[13px] text-danger" role="alert">
            {errorText(t, saving.error)}
          </p>
        )}
        <Button
          variant="outline"
          size="lg"
          icon="download"
          loading={saving.busy}
          onClick={() => void save()}>
          {t("sync.recoveryKey.save")}
        </Button>
        <Button variant="primary" size="lg" onClick={() => void kept()}>
          {t("sync.recoveryKey.done")}
        </Button>
      </div>
    </Page>
  );
}
