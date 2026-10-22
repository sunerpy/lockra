// The password generator (boards 09, 10), shared by the generator page and the dialog a login's form
// opens: a random password or a passphrase from the EFF's list, its strength, the site's rules when
// it is made for one, and the options, kept as the next default. The core makes the password and
// keeps it: copying it (`generator_copy`) puts it in the history, and a login saved with it takes
// it without the webview sending it back. While a generator is on the screen, its password is a
// secret view.
import {
  type ErrorCode,
  type GeneratedView,
  type GeneratorOptions,
  errorText,
  isLockraError,
} from "@lockra/shared";
import {
  Banner,
  Button,
  Icon,
  IconButton,
  Input,
  ScoreMeter,
  Segmented,
  Slider,
  Toggle,
  cx,
  useBackend,
  useT,
  useUiState,
} from "@lockra/ui";
import { useEffect, useState } from "react";
import { useDispatch } from "../../app/dispatch";
import { useSecretView } from "./secret-view";

type PasswordOptions = Extract<GeneratorOptions, { kind: "password" }>;
type PassphraseOptions = Extract<GeneratorOptions, { kind: "passphrase" }>;

/** A generator on the screen: what it is asked for, what it made, and what can be done with it. */
export interface Generator {
  /** The site it makes passwords for: its rules apply when Lockra knows them. */
  domain: string | undefined;
  options: GeneratorOptions;
  setOptions: (change: (current: GeneratorOptions) => GeneratorOptions) => void;
  /** The password made for the options, once the core has answered. */
  made: GeneratedView | undefined;
  score: number | undefined;
  failure: ErrorCode | undefined;
  /** The same options again: another password (「重新生成」). */
  regenerate: () => void;
  /** Passwords without the site's rules, which these options cannot meet. */
  ignoreRules: () => void;
  /** The password shown is the one on the clipboard. */
  copied: boolean;
  copy: () => Promise<void>;
  /** The options become the next default: done once the password is used or copied. */
  keepOptions: () => void;
}

/** The generator's state for `domain` (none: passwords for no site), from the last options used. */
export function useGenerator(domain?: string): Generator {
  const { backend } = useBackend();
  const { settings } = useUiState();
  const dispatch = useDispatch();
  // What is asked of the generator: new options, or the same ones again (「重新生成」), each a new
  // request.
  const [request, setRequest] = useState<{ options: GeneratorOptions }>(() => ({
    options: settings.generator,
  }));
  const [made, setMade] = useState<GeneratedView | undefined>(undefined);
  const [score, setScore] = useState<number | undefined>(undefined);
  const [failure, setFailure] = useState<ErrorCode | undefined>(undefined);
  const [rulesIgnored, setRulesIgnored] = useState(false);
  const [copied, setCopied] = useState(false);
  const site = rulesIgnored ? undefined : domain;
  // However the generator goes, the passwords it showed leave the screen and the secret view ends.
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
  const { options } = request;
  const keepOptions = () =>
    void dispatch({ command: "settings_set", settings: { ...settings, generator: options } });
  return {
    domain,
    options,
    setOptions: (change) => setRequest((r) => ({ options: change(r.options) })),
    made,
    score,
    failure,
    regenerate: () => setRequest((r) => ({ ...r })),
    ignoreRules: () => setRulesIgnored(true),
    copied,
    copy: async () => {
      if ((await dispatch({ command: "generator_copy" })) === undefined) return;
      setCopied(true);
      keepOptions();
    },
    keepOptions,
  };
}

export interface GeneratorPanelProps {
  generator: Generator;
  /** How it copies the password, beside 重新生成: an icon where 「使用此密码」 is the main action (a
   *  login's form), the main button where nothing else takes it (the page). */
  copy: "icon" | "button";
}

/** The generator's controls: the kind, the password with its strength, then the options. */
export function GeneratorPanel({ generator, copy }: GeneratorPanelProps) {
  const t = useT();
  const { options, made, score, failure, domain, copied, setOptions } = generator;
  const password = options.kind === "password" ? options : undefined;
  const passphrase = options.kind === "passphrase" ? options : undefined;
  const setPassword = (change: Partial<PasswordOptions>) =>
    setOptions((o) => (o.kind === "password" ? { ...o, ...change } : o));
  const setPassphrase = (change: Partial<PassphraseOptions>) =>
    setOptions((o) => (o.kind === "passphrase" ? { ...o, ...change } : o));
  const mode = (kind: GeneratorOptions["kind"]) =>
    setOptions(() =>
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
  const copyIt = () => void generator.copy();
  return (
    <div className="flex flex-col gap-3.5">
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
          onClick={generator.regenerate}
          data-testid="generator-regenerate"
        />
        {copy === "icon" ? (
          <IconButton
            icon={copied ? "check" : "copy"}
            label={t("generator.copy")}
            size={28}
            disabled={made === undefined}
            onClick={copyIt}
            data-testid="generator-copy-icon"
          />
        ) : (
          <Button
            variant="primary"
            icon={copied ? "check" : "copy"}
            disabled={made === undefined}
            onClick={copyIt}
            data-testid="generator-copy">
            {copied ? t("notice.copiedKept") : t("generator.copy")}
          </Button>
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
                onClick={generator.ignoreRules}
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
