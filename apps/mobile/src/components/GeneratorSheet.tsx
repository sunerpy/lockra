// The password generator on the phone, in a sheet (the passwords screen's 生成密码, the form's
// 生成密码…): a random password or a passphrase from the EFF's list, its strength, the site's rules
// when it is made for one, and the options, kept as the next default once the password is used or
// copied. The core makes the password and keeps it: copying it puts it in the history, and a login
// saved with it takes it without the webview sending it back. The history is the desktop's.
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
  cx,
  useBackend,
  useDispatch,
  useT,
  useUiState,
} from "@lockra/ui";
import { useEffect, useState } from "react";
import { BottomSheet } from "./BottomSheet";
import { SwitchRow } from "./Rows";

type PasswordOptions = Extract<GeneratorOptions, { kind: "password" }>;
type PassphraseOptions = Extract<GeneratorOptions, { kind: "passphrase" }>;

export function GeneratorSheet({
  onClose,
  onUse,
  domain,
}: {
  onClose: () => void;
  /** From a login's form: 「使用此密码」 hands it over. Without it the sheet's button copies it. */
  onUse?: (password: string) => void;
  /** The site it is made for: its rules apply when Lockra knows them. */
  domain?: string;
}) {
  const t = useT();
  const { backend } = useBackend();
  const { settings } = useUiState();
  const dispatch = useDispatch();
  // A new request for each change of the options, and for 「重新生成」.
  const [request, setRequest] = useState<{ options: GeneratorOptions }>(() => ({
    options: settings.generator,
  }));
  const { options } = request;
  const [made, setMade] = useState<GeneratedView | undefined>(undefined);
  const [score, setScore] = useState<number | undefined>(undefined);
  const [failure, setFailure] = useState<ErrorCode | undefined>(undefined);
  const [ignoreRules, setIgnoreRules] = useState(false);
  const [copied, setCopied] = useState(false);
  const site = ignoreRules ? undefined : domain;
  // However the sheet goes, the passwords it showed leave the screen and the core hears it (the
  // window never shows in screenshots, MainActivity.kt).
  useEffect(
    () => () => {
      void backend.dispatch({ command: "secret_view_closed" }).catch(() => undefined);
    },
    [backend],
  );
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
  const setOptions = (next: GeneratorOptions) => setRequest({ options: next });
  const setPassword = (change: Partial<PasswordOptions>) =>
    setRequest((r) =>
      r.options.kind === "password" ? { options: { ...r.options, ...change } } : r,
    );
  const setPassphrase = (change: Partial<PassphraseOptions>) =>
    setRequest((r) =>
      r.options.kind === "passphrase" ? { options: { ...r.options, ...change } } : r,
    );
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
  const password = options.kind === "password" ? options : undefined;
  const passphrase = options.kind === "passphrase" ? options : undefined;
  return (
    <BottomSheet title={t("generator.title")} onClose={onClose} data-testid="generator-sheet">
      <div className="flex flex-col gap-4 px-2 pb-2">
        <Segmented
          label={t("generator.mode.label")}
          size="lg"
          value={options.kind}
          onChange={(kind) =>
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
                : {
                    kind: "passphrase",
                    words: 6,
                    separator: "-",
                    capitalize: false,
                    number: false,
                  },
            )
          }
          options={(["password", "passphrase"] as const).map((value) => ({
            value,
            label: t(`generator.mode.${value}`),
          }))}
        />
        <div className="flex min-h-18 items-center gap-2 rounded-14 bg-inset px-4 py-3">
          <span
            className={cx(
              "mono min-w-0 flex-1 font-semibold tracking-[1px] break-all text-fg select-all",
              passphrase ? "text-[17px]" : "text-[20px]",
            )}
            data-testid="generator-password">
            {made?.password ?? "—"}
          </span>
          <IconButton
            icon="refresh"
            label={t("generator.regenerate")}
            size={40}
            onClick={() => setRequest((r) => ({ ...r }))}
            data-testid="generator-regenerate"
          />
        </div>
        {made !== undefined && <ScoreMeter score={score} bits={made.bits} />}
        {made?.rules_met && domain !== undefined && (
          <p
            className="flex items-center gap-1.5 text-[13px] text-fg-muted"
            data-testid="generator-rules">
            <Icon name="info" size={16} className="text-accent-text" />
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
                <Button size="sm" variant="outline" onClick={() => setIgnoreRules(true)}>
                  {t("generator.ignoreRules")}
                </Button>
              ) : undefined
            }
          />
        )}
        {password && (
          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1">
              <span className="text-[13px] text-fg-muted">{t("generator.length")}</span>
              <Slider
                label={t("generator.length")}
                value={password.length}
                min={4}
                max={128}
                onChange={(length) => setPassword({ length })}
                data-testid="generator-length"
              />
            </div>
            <div
              className="flex flex-wrap items-center gap-2"
              role="group"
              aria-label={t("generator.characters")}>
              {(["lower", "upper", "digits", "symbols"] as const).map((group) => (
                <button
                  key={group}
                  type="button"
                  aria-pressed={password[group]}
                  onClick={() => setPassword({ [group]: !password[group] })}
                  data-testid={`generator-${group}`}
                  className={cx(
                    "mono inline-flex h-10 items-center gap-1 rounded-10 px-3 text-[13px] transition-colors",
                    password[group]
                      ? "bg-accent-soft text-accent-text shadow-[inset_0_0_0_1px_var(--accent)]"
                      : "bg-surface text-fg-muted hairline",
                  )}>
                  {password[group] && <Icon name="check" size={14} />}
                  {t(`generator.${group}`)}
                </button>
              ))}
            </div>
            <div className="-mx-4">
              <SwitchRow
                label={t("generator.avoidAmbiguous")}
                checked={password.avoid_ambiguous}
                onChange={(avoid_ambiguous) => setPassword({ avoid_ambiguous })}
              />
            </div>
            <Input
              size="lg"
              mono
              label={t("generator.exclude")}
              value={password.exclude}
              onChange={(e) => setPassword({ exclude: e.target.value })}
              autoComplete="off"
              data-testid="generator-exclude"
            />
          </div>
        )}
        {passphrase && (
          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1">
              <span className="text-[13px] text-fg-muted">{t("generator.words")}</span>
              <Slider
                label={t("generator.words")}
                value={passphrase.words}
                min={3}
                max={20}
                onChange={(words) => setPassphrase({ words })}
                data-testid="generator-words"
              />
            </div>
            <Segmented
              label={t("generator.separator")}
              size="lg"
              value={passphrase.separator}
              onChange={(separator) => setPassphrase({ separator })}
              options={["-", ".", "_", " "].map((value) => ({
                value,
                label: value === " " ? t("generator.space") : value,
              }))}
            />
            <div className="-mx-4 flex flex-col">
              <SwitchRow
                label={t("generator.capitalize")}
                checked={passphrase.capitalize}
                onChange={(capitalize) => setPassphrase({ capitalize })}
              />
              <SwitchRow
                label={t("generator.number")}
                checked={passphrase.number}
                onChange={(number) => setPassphrase({ number })}
              />
            </div>
            <p className="text-[13px] text-fg-subtle">{t("generator.wordlist")}</p>
          </div>
        )}
        {onUse === undefined ? (
          <Button
            variant="primary"
            size="lg"
            icon={copied ? "check" : "copy"}
            disabled={made === undefined}
            onClick={() => void copy()}
            data-testid="generator-copy">
            {copied ? t("notice.copiedKept") : t("generator.copy")}
          </Button>
        ) : (
          <div className="flex gap-2">
            <Button
              size="lg"
              icon={copied ? "check" : "copy"}
              disabled={made === undefined}
              onClick={() => void copy()}
              className="flex-1"
              data-testid="generator-copy">
              {copied ? t("notice.copiedKept") : t("generator.copy")}
            </Button>
            <Button
              variant="primary"
              size="lg"
              disabled={made === undefined}
              onClick={use}
              className="flex-1"
              data-testid="generator-use">
              {t("generator.use")}
            </Button>
          </div>
        )}
      </div>
    </BottomSheet>
  );
}
