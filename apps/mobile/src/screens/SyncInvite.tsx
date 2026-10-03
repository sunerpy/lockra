// An invitation for another device, after the master password is entered again: the QR code the
// other device scans, the text it can paste instead, and the sync key alone. They hide after
// REVEAL_SECONDS like any secret (app/secret-page.ts).
import { type SyncInvite as Invite, errorText } from "@lockra/shared";
import { Banner, Button, PasswordField, QrView, useBackend, useSubmit, useT } from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { useSecretAnswer, useSecretPage } from "../app/secret-page";
import { Page } from "../components/Page";
import { SyncKeyText } from "../components/SyncKeyText";

export function SyncInvite() {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const [password, setPassword] = useState("");
  const [invite, setInvite] = useState<{ answer: Invite; at: number } | undefined>(undefined);
  const submit = useSubmit();
  const deliver = useSecretAnswer();
  const left = useSecretPage(invite?.at);
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    const answer = await submit.run(() => backend.dispatch({ command: "sync_invite", password }));
    setPassword("");
    if (answer !== undefined) deliver(() => setInvite({ answer, at: Date.now() }));
  };
  if (invite === undefined) {
    return (
      <Page title={t("sync.invite.open")} testId="page-sync-invite">
        <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
          <p className="text-[14px] text-fg">{t("sync.invite.prompt")}</p>
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
            icon="qr"
            loading={submit.busy}
            disabled={password === ""}>
            {t("sync.invite.submit")}
          </Button>
        </form>
      </Page>
    );
  }
  const { answer } = invite;
  return (
    <Page title={t("sync.invite.open")} testId="page-sync-invite">
      <div className="flex flex-col gap-4" data-testid="sync-invite">
        <Banner tone="warn" marker="icon">
          {t("sync.invite.warning")}
        </Banner>
        <div className="self-center">
          <QrView svg={answer.svg} label={t("ui.a11y.qr")} size={260} />
        </div>
        <p className="text-[14px] text-fg">{t("sync.invite.body")}</p>
        <div className="flex flex-col gap-1.5">
          <span className="text-[13px] text-fg-muted">{t("sync.invite.text")}</span>
          <div
            className="mono max-h-32 overflow-y-auto text-[12px] break-all text-fg-muted select-all"
            data-testid="invite-text">
            {answer.invite}
          </div>
        </div>
        <div className="flex flex-col gap-1.5">
          <span className="text-[13px] text-fg-muted">{t("sync.created.key")}</span>
          <SyncKeyText value={answer.sync_key} />
        </div>
        <p className="text-[13px] text-fg-subtle" data-testid="invite-countdown">
          {t("sync.invite.hideIn", { s: left })}
        </p>
        <Button variant="primary" size="lg" onClick={nav.back}>
          {t("common.done")}
        </Button>
      </div>
    </Page>
  );
}
