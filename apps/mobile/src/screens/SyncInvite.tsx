// An invitation for another device, once the user proved to be here (the fingerprint that unlocks
// this vault, or the master password): the QR code the other device scans, the sealed text to
// send it instead with its code apart (copied by the core for a computer, which scans nothing),
// and the sync key alone. They hide after REVEAL_SECONDS like any secret (app/secret-page.ts).
import { type SyncInvite as Invite, errorText } from "@lockra/shared";
import {
  Banner,
  Button,
  PasswordField,
  QrView,
  unlockBiometric,
  useBackend,
  useSubmit,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { useSecretAnswer, useSecretPage } from "../app/secret-page";
import { Page } from "../components/Page";
import { SyncKeyText } from "../components/SyncKeyText";

export function SyncInvite() {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const { lock, sync } = useUiState();
  const biometric = unlockBiometric(lock);
  const [password, setPassword] = useState("");
  const [invite, setInvite] = useState<{ answer: Invite; at: number } | undefined>(undefined);
  const submit = useSubmit();
  const copying = useSubmit();
  const [copied, setCopied] = useState(false);
  const deliver = useSecretAnswer();
  const left = useSecretPage(invite?.at);
  const ask = async (typed?: string) => {
    const answer = await submit.run(() =>
      backend.dispatch(
        typed === undefined
          ? { command: "sync_invite", reason: t("sync.invite.reason") }
          : { command: "sync_invite", password: typed },
      ),
    );
    setPassword("");
    if (answer !== undefined) deliver(() => setInvite({ answer, at: Date.now() }));
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    await ask(password);
  };
  if (invite === undefined) {
    return (
      <Page title={t("sync.invite.open")} testId="page-sync-invite">
        <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
          <p className="text-[14px] text-fg">
            {t(biometric === null ? "sync.invite.prompt" : "sync.invite.promptBiometric")}
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
  const copy = async () => {
    setCopied(false);
    const done = await copying.run(() =>
      backend.dispatch({ command: "sync_invite_copy", text: answer.shared_text }),
    );
    if (done !== undefined) setCopied(true);
  };
  return (
    <Page title={t("sync.invite.open")} testId="page-sync-invite">
      <div className="flex flex-col gap-4" data-testid="sync-invite">
        <Banner tone="warn" marker="icon">
          {t(
            sync.space?.storage.kind === "relay"
              ? "sync.invite.warningRelay"
              : "sync.invite.warning",
          )}
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
            {answer.shared_text}
          </div>
          <Button
            variant="outline"
            size="lg"
            icon="copy"
            loading={copying.busy}
            onClick={() => void copy()}
            className="self-start">
            {t("sync.invite.copy")}
          </Button>
          {copied && (
            <p role="status" className="text-[13px] text-fg-muted">
              {t("sync.invite.copied")}
            </p>
          )}
          {copying.error !== undefined && (
            <p role="alert" className="text-[13px] text-danger">
              {errorText(t, copying.error)}
            </p>
          )}
        </div>
        <div className="flex flex-col gap-1.5">
          <span className="text-[13px] text-fg-muted">{t("sync.invite.code")}</span>
          <div
            className="mono text-[20px] tracking-wide text-fg select-all"
            data-testid="invite-code">
            {answer.code}
          </div>
          <p className="text-[13px] text-fg-subtle">{t("sync.invite.codeHint")}</p>
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
