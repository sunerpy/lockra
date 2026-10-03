// An account's secret and QR code, behind the master password entered again; they hide after
// REVEAL_SECONDS like any secret (app/secret-page.ts).
import { type EntryView, type Revealed, entryLabel, errorText } from "@lockra/shared";
import { Banner, Button, PasswordField, QrView, useBackend, useSubmit, useT } from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { useSecretPage } from "../app/secret-page";
import { Page } from "../components/Page";

export function Reveal({ entry }: { entry: EntryView }) {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const [password, setPassword] = useState("");
  const [revealed, setRevealed] = useState<{ answer: Revealed; at: number } | undefined>(undefined);
  const submit = useSubmit();
  const left = useSecretPage(revealed?.at);
  const { back } = nav;
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    const answer = await submit.run(() =>
      backend.dispatch({ command: "entry_reveal", id: entry.id, password }),
    );
    setPassword("");
    if (answer !== undefined) setRevealed({ answer, at: Date.now() });
  };
  const name = entryLabel(entry.issuer, entry.account);
  if (revealed === undefined) {
    return (
      <Page title={t("entry.revealTitle")} testId="page-reveal">
        <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
          <p className="text-[14px] text-fg">{t("entry.revealPrompt", { name })}</p>
          <PasswordField
            size="lg"
            label={t("unlock.password")}
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            error={submit.error === undefined ? undefined : errorText(t, submit.error)}
          />
          <Button
            variant="primary"
            size="lg"
            type="submit"
            loading={submit.busy}
            disabled={password === ""}>
            {t("entry.revealSubmit")}
          </Button>
        </form>
      </Page>
    );
  }
  const { answer } = revealed;
  return (
    <Page title={name} testId="page-reveal">
      <div className="flex flex-col gap-4" data-testid="revealed">
        <Banner tone="warn" marker="icon">
          {t("entry.revealWarning")}
        </Banner>
        <div className="self-center">
          <QrView svg={answer.svg} label={t("ui.a11y.qr")} size={240} />
        </div>
        <div>
          <div className="text-[12px] text-fg-muted">{t("entry.revealSecret")}</div>
          <div
            className="mono text-[16px] break-all text-fg select-all"
            data-testid="revealed-secret">
            {answer.secret}
          </div>
        </div>
        <div>
          <div className="text-[12px] text-fg-muted">{t("entry.revealUri")}</div>
          <div className="mono text-[12px] break-all text-fg-muted select-all">{answer.uri}</div>
        </div>
        <p className="text-[13px] text-fg-subtle" data-testid="reveal-countdown">
          {t("export.hideIn", { s: left })}
        </p>
        <Button variant="primary" size="lg" onClick={back}>
          {t("common.done")}
        </Button>
      </div>
    </Page>
  );
}
