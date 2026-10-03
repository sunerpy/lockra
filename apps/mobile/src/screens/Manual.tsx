// Add an account by hand: the service, the account, the secret and a group; the advanced section
// holds the type, the period or counter, the digits and the algorithm. The core checks the secret.
// A secret typed here passes through React state once and is gone when the page closes.
import { ALGORITHMS, type Algorithm, type OtpKind, errorText, parseKind } from "@lockra/shared";
import { Banner, Button, Icon, Input, Segmented, useBackend, useSubmit, useT } from "@lockra/ui";
import { type ReactNode, type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { GroupInput } from "../components/GroupInput";
import { Page } from "../components/Page";

const DIGITS = ["6", "7", "8"] as const;
type DigitsText = (typeof DIGITS)[number];

export function Manual() {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const [issuer, setIssuer] = useState("");
  const [account, setAccount] = useState("");
  const [secret, setSecret] = useState("");
  const [group, setGroup] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [type, setType] = useState<OtpKind["type"]>("totp");
  const [period, setPeriod] = useState("30");
  const [counter, setCounter] = useState("0");
  const [digits, setDigits] = useState<DigitsText>("6");
  const [algorithm, setAlgorithm] = useState<Algorithm>("sha1");
  const submit = useSubmit();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    const kind = parseKind(type, period, counter);
    if (kind === undefined) {
      submit.setError("invalid_parameters");
      return;
    }
    const draft = {
      issuer,
      account,
      secret,
      kind,
      algorithm,
      digits: Number(digits),
      group: group.trim() === "" ? null : group.trim(),
    };
    const added = await submit.run(() => backend.dispatch({ command: "entry_add_manual", draft }));
    if (added !== undefined) nav.home();
  };
  const fieldError =
    submit.error === "invalid_secret" || submit.error === "invalid_parameters"
      ? submit.error
      : undefined;
  // Parameters out of range open the section that holds them.
  const showAdvanced = advanced || fieldError === "invalid_parameters";
  return (
    <Page title={t("entry.manualTitle")} testId="page-manual">
      <form
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-4"
        data-testid="manual-form">
        {submit.error !== undefined && fieldError === undefined && (
          <Banner tone="danger" marker="bar">
            {errorText(t, submit.error)}
          </Banner>
        )}
        <Input
          size="lg"
          label={t("entry.issuer")}
          value={issuer}
          onChange={(e) => setIssuer(e.target.value)}
          placeholder="GitHub"
          autoComplete="off"
        />
        <Input
          size="lg"
          label={t("entry.account")}
          value={account}
          onChange={(e) => setAccount(e.target.value)}
          placeholder="name@example.com"
          autoComplete="off"
        />
        <Input
          size="lg"
          mono
          label={t("entry.secret")}
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
          help={t("entry.secretHint")}
          error={submit.error === "invalid_secret" ? errorText(t, "invalid_secret") : undefined}
          autoComplete="off"
          autoCapitalize="characters"
          spellCheck={false}
        />
        <GroupInput value={group} onChange={setGroup} />
        <button
          type="button"
          aria-expanded={showAdvanced}
          onClick={() => setAdvanced(!showAdvanced)}
          className="flex h-11 items-center gap-1.5 self-start text-[14px] text-fg-muted">
          <Icon name={showAdvanced ? "chevronDown" : "chevronRight"} size={16} />
          {t("entry.advanced")}
        </button>
        {showAdvanced && (
          <div className="flex flex-col gap-4" data-testid="advanced">
            <Choice label={t("entry.type")}>
              <Segmented
                size="lg"
                label={t("entry.type")}
                value={type}
                onChange={setType}
                options={[
                  { value: "totp", label: t("entry.totp") },
                  { value: "hotp", label: t("entry.hotp") },
                ]}
              />
            </Choice>
            {type === "totp" ? (
              <Input
                size="lg"
                mono
                label={t("entry.period")}
                type="number"
                inputMode="numeric"
                min={1}
                max={3600}
                value={period}
                onChange={(e) => setPeriod(e.target.value)}
              />
            ) : (
              <Input
                size="lg"
                mono
                label={t("entry.counter")}
                type="number"
                inputMode="numeric"
                min={0}
                value={counter}
                onChange={(e) => setCounter(e.target.value)}
              />
            )}
            <Choice label={t("entry.digits")}>
              <Segmented
                size="lg"
                mono
                label={t("entry.digits")}
                value={digits}
                onChange={setDigits}
                options={DIGITS.map((d) => ({ value: d, label: d }))}
              />
            </Choice>
            <Choice label={t("entry.algorithm")}>
              <Segmented
                size="lg"
                mono
                label={t("entry.algorithm")}
                value={algorithm}
                onChange={setAlgorithm}
                options={ALGORITHMS.map((a) => ({ value: a, label: a.toUpperCase() }))}
              />
            </Choice>
            {fieldError === "invalid_parameters" && (
              <p role="alert" className="text-[13px] text-danger">
                {errorText(t, fieldError)}
              </p>
            )}
          </div>
        )}
        <Button
          variant="primary"
          size="lg"
          type="submit"
          loading={submit.busy}
          disabled={secret.trim() === ""}>
          {t("entry.add")}
        </Button>
      </form>
    </Page>
  );
}

/** A labelled choice that is not a text field (a segmented control). */
function Choice({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-[12px] text-fg-muted">{label}</span>
      {children}
    </div>
  );
}
