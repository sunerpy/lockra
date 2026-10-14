// A login's password on a page of its own (boards 28, 29). A 「填充前验证」 login asks first for the
// fingerprint that unlocks this vault (by itself where it is the default unlock) or the master
// password; the others show it at once. It hides after REVEAL_SECONDS, closing the page, and the
// core hears that the secret view ended however the page goes (app/secret-page.ts). 复制密码 is
// Rust's, marked sensitive on the clipboard (ClipboardPlugin.kt).
import { type ItemView, REVEAL_SECONDS, errorText, itemAccount, itemName } from "@lockra/shared";
import {
  Banner,
  Button,
  CountdownRing,
  PasswordField,
  type Proof,
  motionReduced,
  useBackend,
  useClock,
  useSubmit,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useEffect, useRef, useState } from "react";
import { useNav } from "../app/nav";
import { shown, useAutoFingerprint } from "../app/presence";
import { useSecretAnswer, useSecretPage } from "../app/secret-page";
import { useItemActions } from "../app/verify";
import { Page } from "../components/Page";

const STRENGTH = [
  "generator.strength.s0",
  "generator.strength.s1",
  "generator.strength.s2",
  "generator.strength.s3",
  "generator.strength.s4",
] as const;

export function LoginReveal({ item }: { item: ItemView }) {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const { settings } = useUiState();
  const actions = useItemActions();
  const now = useClock();
  const submit = useSubmit();
  const [revealed, setRevealed] = useState<{ password: string; at: number } | undefined>(undefined);
  const [score, setScore] = useState<number | undefined>(undefined);
  const left = useSecretPage(revealed?.at);
  const deliver = useSecretAnswer();
  const asksFirst = item.protection === "confirm";
  const ask = async (proof: Proof = {}) => {
    const answer = await submit.run(() =>
      backend.dispatch({
        command: "item_reveal",
        id: item.id,
        parts: { password: true },
        ...proof,
      }),
    );
    const password = answer?.password;
    // The fingerprint's prompt may outlast the page: a page gone shows nothing.
    if (password !== undefined && password !== null)
      deliver(() => setRevealed({ password, at: Date.now() }));
  };
  // A login that asks for nothing shows at once; what the asking needs is read when it asks.
  const asking = useRef({ ask, asksFirst });
  useEffect(() => {
    asking.current = { ask, asksFirst };
  });
  useEffect(() => {
    if (!asking.current.asksFirst) void asking.current.ask();
  }, []);
  const password = revealed?.password;
  useEffect(() => {
    if (password === undefined) return undefined;
    let live = true;
    backend.dispatch({ command: "password_score", password }).then(
      (answer) => {
        if (live) setScore(answer.score);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [backend, password]);
  if (revealed === undefined) {
    return (
      <Page title={t("mobile.logins.revealTitle")} testId="page-login-reveal">
        {asksFirst ? (
          <Prove
            name={itemName(item)}
            busy={submit.busy}
            error={shown(submit.error)}
            onProof={ask}
          />
        ) : (
          submit.error !== undefined && (
            <p role="alert" className="text-[14px] text-danger">
              {errorText(t, submit.error)}
            </p>
          )
        )}
      </Page>
    );
  }
  const account = itemAccount(item);
  const length = Array.from(revealed.password).length;
  const facts = [t("item.reveal.chars", { n: length })];
  if (score !== undefined)
    facts.push(t(STRENGTH[Math.min(4, Math.max(0, score))] ?? "generator.strength.s0"));
  return (
    <Page title={t("mobile.logins.revealTitle")} testId="page-login-reveal">
      <div className="flex flex-col gap-4" data-testid="login-revealed">
        <Banner tone="warn" marker="icon">
          {t("item.reveal.warning")}
        </Banner>
        <div className="flex flex-col gap-2 rounded-14 bg-surface px-4 py-4 hairline panel">
          <span className="truncate text-[13px] text-fg-muted">
            {account !== "" && item.title !== "" ? `${item.title} · ${account}` : itemName(item)}
          </span>
          <span
            className="mono text-[26px] leading-[1.25] font-semibold tracking-[1px] break-all text-fg select-all"
            data-testid="login-revealed-password">
            {revealed.password}
          </span>
          <span className="text-[13px] text-fg-muted">{facts.join(" · ")}</span>
        </div>
        <Button
          variant="primary"
          size="lg"
          icon="copy"
          onClick={() => void actions.copy(item, { type: "password" })}
          data-testid="login-reveal-copy">
          {t("item.copy.password")}
        </Button>
        <p
          className="flex items-center justify-center gap-1.5 text-[13px] text-fg-subtle"
          data-testid="login-reveal-countdown">
          <CountdownRing
            validFromMs={revealed.at}
            validUntilMs={revealed.at + REVEAL_SECONDS * 1000}
            nowMs={now}
            size={16}
            stroke={2}
            still={motionReduced(settings)}
          />
          {t("item.reveal.hideIn", { s: left })}
        </p>
        <Button size="lg" onClick={nav.back}>
          {t("common.done")}
        </Button>
      </div>
    </Page>
  );
}

/** The proof a 「填充前验证」 login asks for (board 29): the fingerprint, or the master password;
 *  without the password, the fingerprint. */
function Prove({
  name,
  busy,
  error,
  onProof,
}: {
  name: string;
  busy: boolean;
  error: ReturnType<typeof shown>;
  onProof: (proof: Proof) => Promise<void>;
}) {
  const t = useT();
  const [password, setPassword] = useState("");
  const action = t("item.verify.action.show");
  const reason = t("item.verify.reason", { action, name });
  const fingerprint = useAutoFingerprint(() => onProof({ reason }));
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    const typed = password;
    setPassword("");
    if (typed !== "") await onProof({ password: typed });
    else if (fingerprint !== null) await onProof({ reason });
  };
  return (
    <form
      onSubmit={(e) => void onSubmit(e)}
      className="flex flex-col gap-4"
      data-testid="login-prove">
      <p className="text-[14px] text-fg">
        {t(fingerprint === null ? "item.verify.promptPassword" : "item.verify.prompt", {
          action,
          name,
        })}
      </p>
      {fingerprint !== null && (
        <Button
          variant="outline"
          size="lg"
          icon="fingerprint"
          loading={busy}
          onClick={() => void onProof({ reason })}>
          {t("mobile.verifyWith")}
        </Button>
      )}
      <PasswordField
        size="lg"
        label={t("unlock.password")}
        value={password}
        onChange={setPassword}
        autoComplete="current-password"
        help={fingerprint === null ? undefined : t("mobile.passwordOrFingerprint")}
        error={error === undefined ? undefined : errorText(t, error)}
      />
      <Button
        variant="primary"
        size="lg"
        type="submit"
        loading={busy}
        disabled={password === "" && fingerprint === null}
        data-testid="login-prove-submit">
        {action}
      </Button>
    </form>
  );
}
