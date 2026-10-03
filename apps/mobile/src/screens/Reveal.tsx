// An account's secret and QR code, behind the master password entered again; they hide after
// REVEAL_SECONDS. The window never shows in screenshots (FLAG_SECURE, MainActivity.kt); the core
// still hears when the secret view ends, as on the desktop.
import {
  type EntryView,
  REVEAL_SECONDS,
  type Revealed,
  entryLabel,
  errorText,
} from "@lockra/shared";
import {
  Banner,
  Button,
  PasswordField,
  QrView,
  useBackend,
  useClock,
  useSubmit,
  useT,
} from "@lockra/ui";
import { type SubmitEvent, useEffect, useState } from "react";
import { useNav } from "../app/nav";
import { Page } from "../components/Page";

export function Reveal({ entry }: { entry: EntryView }) {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const now = useClock();
  const [password, setPassword] = useState("");
  const [revealed, setRevealed] = useState<{ answer: Revealed; at: number } | undefined>(undefined);
  const submit = useSubmit();
  const shown = revealed !== undefined;
  // However the page goes (its button, the back gesture, the vault locking), the secret view ends.
  useEffect(() => {
    if (!shown) return undefined;
    return () => {
      void backend.dispatch({ command: "secret_view_closed" }).catch(() => undefined);
    };
  }, [shown, backend]);
  // The shared clock ticks on whole seconds, so it can read just before the moment of the reveal.
  const left =
    revealed === undefined
      ? REVEAL_SECONDS
      : Math.max(0, REVEAL_SECONDS - Math.max(0, Math.floor((now - revealed.at) / 1000)));
  const { back } = nav;
  useEffect(() => {
    if (shown && left === 0) back();
  }, [shown, left, back]);
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
