// The password generator (boards 09, 10): a random password or a passphrase from the EFF's list,
// its strength, the site's rules when it is made for one, and the options, kept as the next
// default. The core makes the password and keeps it: copying it (`generator_copy`) puts it in the
// history, and a login saved with it takes it without the webview sending it back. The history
// opens after the master password or the system check, and hides itself after two minutes.
import {
  type ErrorCode,
  type GeneratedPastView,
  type GeneratedView,
  type GeneratorOptions,
  REVEAL_SECONDS,
  errorText,
  isLockraError,
  relativeTime,
} from "@lockra/shared";
import {
  Banner,
  Button,
  Dialog,
  Icon,
  IconButton,
  Input,
  ScoreMeter,
  Segmented,
  Slider,
  Toggle,
  cx,
  useBackend,
  useClock,
  useT,
  useUiState,
} from "@lockra/ui";
import { useEffect, useState } from "react";
import { useDispatch } from "../../app/dispatch";
import { useSecretView } from "./secret-view";
import { useVerify } from "./verify";

type PasswordOptions = Extract<GeneratorOptions, { kind: "password" }>;
type PassphraseOptions = Extract<GeneratorOptions, { kind: "passphrase" }>;

export interface GeneratorDialogProps {
  onClose: () => void;
  /** Opened from a login's form: 「使用此密码」 hands the password over (the login then saves it as
   *  the generator's). Without it the dialog's button copies it. */
  onUse?: (password: string) => void;
  /** The site it is made for: its rules apply when Lockra knows them. */
  domain?: string;
}

export function GeneratorDialog({ onClose, onUse, domain }: GeneratorDialogProps) {
  const t = useT();
  const { backend } = useBackend();
  const { settings, generator } = useUiState();
  const dispatch = useDispatch();
  const verify = useVerify();
  const now = useClock();
  // What is asked of the generator: new options, or the same ones again (「重新生成」), each a new
  // request.
  const [request, setRequest] = useState<{ options: GeneratorOptions }>(() => ({
    options: settings.generator,
  }));
  const { options } = request;
  const setOptions = (next: GeneratorOptions | ((current: GeneratorOptions) => GeneratorOptions)) =>
    setRequest((r) => ({ options: typeof next === "function" ? next(r.options) : next }));
  const [made, setMade] = useState<GeneratedView | undefined>(undefined);
  const [score, setScore] = useState<number | undefined>(undefined);
  const [failure, setFailure] = useState<ErrorCode | undefined>(undefined);
  const [ignoreRules, setIgnoreRules] = useState(false);
  const [copied, setCopied] = useState(false);
  const [history, setHistory] = useState<{ list: GeneratedPastView[]; at: number } | undefined>(
    undefined,
  );
  const site = ignoreRules ? undefined : domain;
  // However the dialog goes, the passwords it showed leave the screen and the secret view ends.
  useSecretView(true);
  // A password for the options (and the site's rules), made again whenever they change; an answer
  // that comes after a newer request is dropped.
  useEffect(() => {
    let live = true;
    backend
      .dispatch({
        command: "generator_generate",
        options: request.options,
        ...(site === undefined ? {} : { domain: site }),
      })
      .then((answer) => {
        if (!live) return;
        // A new password: nothing of it is copied yet.
        setCopied(false);
        setFailure(undefined);
        setMade(answer);
        backend.dispatch({ command: "password_score", password: answer.password }).then(
          (strength) => {
            if (live) setScore(strength.score);
          },
          () => undefined,
        );
      })
      .catch((error: unknown) => {
        if (!live) return;
        setMade(undefined);
        setScore(undefined);
        setFailure(isLockraError(error) ? error.code : "internal");
      });
    return () => {
      live = false;
    };
  }, [backend, request, site]);
  const historyLeft =
    history === undefined
      ? REVEAL_SECONDS
      : Math.max(0, REVEAL_SECONDS - Math.floor((now - history.at) / 1000));
  // The history hides itself two minutes after it opened.
  useEffect(() => {
    if (history === undefined) return undefined;
    const timer = setTimeout(
      () => setHistory(undefined),
      Math.max(0, history.at + REVEAL_SECONDS * 1000 - Date.now()),
    );
    return () => clearTimeout(timer);
  }, [history]);
  // The options become the next default once the password is used or copied.
  const keepOptions = () =>
    void dispatch({ command: "settings_set", settings: { ...settings, generator: options } });
  const copy = async () => {
    if ((await dispatch({ command: "generator_copy" })) === undefined) return;
    setCopied(true);
    keepOptions();
  };
  const use = () => {
    if (made === undefined || onUse === undefined) return;
    keepOptions();
    onUse(made.password);
  };
  const openHistory = async () => {
    const list = await verify("view", t("generator.history.title"), (proof) =>
      backend.dispatch({ command: "generator_history_reveal", ...proof }),
    );
    if (list !== undefined) setHistory({ list, at: Date.now() });
  };
  const clearHistory = async () => {
    if ((await dispatch({ command: "generator_history_clear" })) !== undefined)
      setHistory(undefined);
  };
  const password = options.kind === "password" ? options : undefined;
  const passphrase = options.kind === "passphrase" ? options : undefined;
  const setPassword = (change: Partial<PasswordOptions>) =>
    setOptions((o) => (o.kind === "password" ? { ...o, ...change } : o));
  const setPassphrase = (change: Partial<PassphraseOptions>) =>
    setOptions((o) => (o.kind === "passphrase" ? { ...o, ...change } : o));
  const mode = (kind: GeneratorOptions["kind"]) =>
    setOptions(
      kind === "password"
        ? {
            kind: "password",
            length: 20,
            lower: true,
            upper: true,
            digits: true,
            symbols: true,
            avoid_ambiguous: false,
            exclude: "",
          }
        : { kind: "passphrase", words: 6, separator: "-", capitalize: false, number: false },
    );
  if (history !== undefined) {
    return (
      <Dialog
        open
        title={t("generator.history.title")}
        onClose={() => setHistory(undefined)}
        width={520}
        hint={
          <span data-testid="generator-history-countdown">
            {t("passwords.detail.hideIn", { s: historyLeft })}
          </span>
        }
        actions={
          <>
            <Button
              variant="text-danger"
              onClick={() => void clearHistory()}
              disabled={history.list.length === 0}
              data-testid="generator-history-clear">
              {t("generator.history.clear")}
            </Button>
            <Button variant="primary" onClick={() => setHistory(undefined)}>
              {t("common.done")}
            </Button>
          </>
        }>
        {history.list.length === 0 ? (
          <p>{t("generator.history.empty")}</p>
        ) : (
          <ul className="flex flex-col divide-y divide-border" data-testid="generator-history">
            {history.list.map((entry) => (
              <li
                key={`${entry.at_ms}:${entry.password}`}
                className="flex min-h-13 flex-col justify-center py-1.5">
                <span className="mono text-[13px] font-medium break-all text-fg select-all">
                  {entry.password}
                </span>
                <span className="text-[11.5px] text-fg-subtle">
                  {[
                    relativeTime(t, entry.at_ms, now),
                    entry.site === null ? "" : t("generator.history.site", { site: entry.site }),
                  ]
                    .filter((s) => s !== "")
                    .join(" · ")}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Dialog>
    );
  }
  return (
    <Dialog
      open
      title={t("generator.title")}
      onClose={onClose}
      width={520}
      hint={
        <Button
          variant="text"
          size="sm"
          icon="history"
          className="font-sans"
          onClick={() => void openHistory()}
          data-testid="generator-history-open">
          {t("generator.history.open", { n: generator.history_count })}
        </Button>
      }
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          {onUse === undefined ? (
            <Button
              variant="primary"
              icon={copied ? "check" : "copy"}
              disabled={made === undefined}
              onClick={() => void copy()}
              data-testid="generator-copy">
              {copied ? t("notice.copiedKept") : t("generator.copy")}
            </Button>
          ) : (
            <Button
              variant="primary"
              disabled={made === undefined}
              onClick={use}
              data-testid="generator-use">
              {t("generator.use")}
            </Button>
          )}
        </>
      }>
      <div className="flex flex-col gap-3.5" data-testid="generator-dialog">
        <Segmented
          label={t("generator.mode.label")}
          value={options.kind}
          onChange={mode}
          options={(["password", "passphrase"] as const).map((value) => ({
            value,
            label: t(`generator.mode.${value}`),
          }))}
          className="w-[220px]"
        />
        <div className="flex h-16 items-center gap-2 rounded-10 bg-inset px-3.5">
          <span
            className={cx(
              "mono min-w-0 flex-1 truncate font-semibold tracking-[1px] text-fg select-all",
              passphrase ? "text-[18px]" : "text-[22px]",
            )}
            title={made?.password}
            data-testid="generator-password">
            {made?.password ?? "—"}
          </span>
          <IconButton
            icon="refresh"
            label={t("generator.regenerate")}
            size={28}
            onClick={() => setRequest((r) => ({ ...r }))}
            data-testid="generator-regenerate"
          />
          {onUse !== undefined && (
            <IconButton
              icon={copied ? "check" : "copy"}
              label={t("generator.copy")}
              size={28}
              disabled={made === undefined}
              onClick={() => void copy()}
              data-testid="generator-copy-icon"
            />
          )}
        </div>
        {made !== undefined && <ScoreMeter score={score} bits={made.bits} />}
        {made !== undefined && score !== undefined && score < 2 && (
          <p className="flex items-center gap-1.5 text-[12px] text-fg-muted">
            <Icon name="alert" size={14} className="text-warning-text" />
            {t("generator.weak")}
          </p>
        )}
        {made?.rules_met && domain !== undefined && (
          <p
            className="flex items-center gap-1.5 text-[12px] text-fg-muted"
            data-testid="generator-rules">
            <Icon name="info" size={14} className="text-accent-text" />
            {t("generator.rules", { site: domain })}
          </p>
        )}
        {failure !== undefined && (
          <Banner
            tone="danger"
            marker="bar"
            title={
              failure === "generator_rules_unsatisfiable"
                ? t("generator.unsatisfiable")
                : errorText(t, failure)
            }
            actions={
              failure === "generator_rules_unsatisfiable" && domain !== undefined ? (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setIgnoreRules(true)}
                  data-testid="generator-ignore-rules">
                  {t("generator.ignoreRules")}
                </Button>
              ) : undefined
            }
          />
        )}
        {password && (
          <div className="flex flex-col gap-2.5">
            <Row label={t("generator.length")}>
              <Slider
                label={t("generator.length")}
                value={password.length}
                min={4}
                max={128}
                onChange={(length) => setPassword({ length })}
                data-testid="generator-length"
              />
            </Row>
            <Row label={t("generator.characters")}>
              <div className="flex flex-wrap items-center gap-1.5">
                {(["lower", "upper", "digits", "symbols"] as const).map((group) => (
                  <button
                    key={group}
                    type="button"
                    aria-pressed={password[group]}
                    onClick={() => setPassword({ [group]: !password[group] })}
                    data-testid={`generator-${group}`}
                    className={cx(
                      "mono inline-flex h-7 items-center gap-1 rounded-6 px-2 text-[12px] transition-colors",
                      password[group]
                        ? "bg-accent-soft text-accent-text shadow-[inset_0_0_0_1px_var(--accent)]"
                        : "bg-surface text-fg-muted hairline",
                    )}>
                    {password[group] && <Icon name="check" size={12} />}
                    {t(`generator.${group}`)}
                  </button>
                ))}
                <span className="text-[12px] text-fg-subtle">{t("generator.eachGroup")}</span>
              </div>
            </Row>
            <Row label="">
              <Toggle
                checked={password.avoid_ambiguous}
                onChange={(avoid_ambiguous) => setPassword({ avoid_ambiguous })}
                label={t("generator.avoidAmbiguous")}
              />
            </Row>
            <Row label={t("generator.exclude")}>
              <Input
                mono
                size="sm"
                value={password.exclude}
                onChange={(e) => setPassword({ exclude: e.target.value })}
                aria-label={t("generator.exclude")}
                className="w-36"
                data-testid="generator-exclude"
              />
            </Row>
          </div>
        )}
        {passphrase && (
          <div className="flex flex-col gap-2.5">
            <Row label={t("generator.words")}>
              <Slider
                label={t("generator.words")}
                value={passphrase.words}
                min={3}
                max={20}
                onChange={(words) => setPassphrase({ words })}
                data-testid="generator-words"
              />
            </Row>
            <Row label={t("generator.separator")}>
              <Segmented
                label={t("generator.separator")}
                size="sm"
                value={passphrase.separator}
                onChange={(separator) => setPassphrase({ separator })}
                options={["-", ".", "_", " "].map((value) => ({
                  value,
                  label: value === " " ? t("generator.space") : value,
                }))}
              />
            </Row>
            <Row label="">
              <div className="flex flex-wrap gap-4">
                <Toggle
                  checked={passphrase.capitalize}
                  onChange={(capitalize) => setPassphrase({ capitalize })}
                  label={t("generator.capitalize")}
                />
                <Toggle
                  checked={passphrase.number}
                  onChange={(number) => setPassphrase({ number })}
                  label={t("generator.number")}
                />
              </div>
            </Row>
            <p className="text-[12px] text-fg-subtle">{t("generator.wordlist")}</p>
          </div>
        )}
      </div>
    </Dialog>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex min-h-9 items-center gap-3">
      <span className="w-24 shrink-0 text-[13px] text-fg-muted">{label}</span>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}
