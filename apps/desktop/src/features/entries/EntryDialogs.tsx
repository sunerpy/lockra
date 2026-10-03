// The dialogs of one account, opened through the shell's overlay: add by hand, add from an
// otpauth link, edit, delete, and reveal (the secret and its QR code, behind the master password).
// A secret typed here passes through React state once and is gone when its dialog closes.
import {
  ALGORITHMS,
  type Algorithm,
  type EntryView,
  type ErrorCode,
  type OtpKind,
  REVEAL_SECONDS,
  type Revealed,
  entryGroups,
  entryLabel,
  errorText,
  originText,
  parametersText,
  parseKind,
} from "@lockra/shared";
import {
  AccountAppearance,
  Banner,
  Button,
  Dialog,
  Icon,
  Input,
  PasswordField,
  QrView,
  Segmented,
  Textarea,
  Toggle,
  useBackend,
  useClock,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useSubmit } from "../../app/dispatch";
import { useShell } from "../../app/shell-state";

const DIGITS = ["6", "7", "8"] as const;
type DigitsText = (typeof DIGITS)[number];

/** The overlay's dialog, if it is one of an account's. */
export function EntryDialogs() {
  const shell = useShell();
  const { entries } = useUiState();
  const overlay = shell.overlay;
  const id = overlay !== null && "id" in overlay ? overlay.id : undefined;
  const entry = id === undefined ? undefined : entries.find((e) => e.id === id);
  // The account went away meanwhile (an import replaced it, the vault was restored): close.
  const missing = id !== undefined && entry === undefined;
  const { close } = shell;
  useEffect(() => {
    if (missing) close();
  }, [missing, close]);
  if (overlay === null) return null;
  switch (overlay.type) {
    case "add_manual":
      return <AddManualDialog onClose={close} />;
    case "add_uri":
      return <AddUriDialog onClose={close} />;
    case "edit":
      return entry ? <EditDialog entry={entry} onClose={close} /> : null;
    case "delete":
      return entry ? <DeleteDialog entry={entry} onClose={close} /> : null;
    case "reveal":
      return entry ? <RevealDialog entry={entry} onClose={close} /> : null;
    case "export":
    case "settings":
      return null;
  }
}

/** A labelled control that is not a text field (a segmented choice). */
function Field({
  label,
  children,
  className,
}: {
  label: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={className}>
      <div className="mb-1 text-[12px] text-fg-muted">{label}</div>
      {children}
    </div>
  );
}

function FormError({ code }: { code: ErrorCode | undefined }) {
  const t = useT();
  if (code === undefined) return null;
  return (
    <p role="alert" className="text-[12px] text-danger">
      {errorText(t, code)}
    </p>
  );
}

/** A group's name, suggesting the groups the accounts are in. */
export function GroupField({
  value,
  onChange,
  autoFocus = false,
}: {
  value: string;
  onChange: (value: string) => void;
  /** The dialog's first field. */
  autoFocus?: boolean;
}) {
  const t = useT();
  const { entries } = useUiState();
  const listId = useId();
  return (
    <>
      <Input
        label={t("entry.group")}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={t("entry.groupPlaceholder")}
        list={listId}
        {...(autoFocus ? { "data-autofocus": true } : {})}
      />
      <datalist id={listId}>
        {entryGroups(entries).map((group) => (
          <option key={group} value={group} />
        ))}
      </datalist>
    </>
  );
}

function AddManualDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const formId = useId();
  const [issuer, setIssuer] = useState("");
  const [account, setAccount] = useState("");
  const [secret, setSecret] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [type, setType] = useState<OtpKind["type"]>("totp");
  const [period, setPeriod] = useState("30");
  const [counter, setCounter] = useState("0");
  const [digits, setDigits] = useState<DigitsText>("6");
  const [algorithm, setAlgorithm] = useState<Algorithm>("sha1");
  const [group, setGroup] = useState("");
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
    if (added !== undefined) onClose();
  };
  const fieldError =
    submit.error === "invalid_secret" || submit.error === "invalid_parameters"
      ? submit.error
      : undefined;
  // Parameters out of range open the section that holds them.
  const showAdvanced = advanced || fieldError === "invalid_parameters";
  return (
    <Dialog
      open
      title={t("entry.manualTitle")}
      onClose={onClose}
      width={480}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            type="submit"
            form={formId}
            loading={submit.busy}
            disabled={secret.trim() === ""}>
            {t("entry.add")}
          </Button>
        </>
      }>
      <form
        id={formId}
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-3"
        data-testid="add-manual-form">
        {submit.error !== undefined && fieldError === undefined && (
          <Banner tone="danger" marker="bar">
            {errorText(t, submit.error)}
          </Banner>
        )}
        <Input
          label={t("entry.issuer")}
          value={issuer}
          onChange={(e) => setIssuer(e.target.value)}
          placeholder="GitHub"
          data-autofocus
        />
        <Input
          label={t("entry.account")}
          value={account}
          onChange={(e) => setAccount(e.target.value)}
          placeholder="name@example.com"
        />
        <Input
          label={t("entry.secret")}
          mono
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
          help={t("entry.secretHint")}
          error={submit.error === "invalid_secret" ? errorText(t, "invalid_secret") : undefined}
          autoComplete="off"
          spellCheck={false}
        />
        <GroupField value={group} onChange={setGroup} />
        <button
          type="button"
          aria-expanded={showAdvanced}
          onClick={() => setAdvanced(!showAdvanced)}
          className="flex items-center gap-1 self-start rounded-4 text-[12px] text-fg-muted hover:text-fg">
          <Icon name={showAdvanced ? "chevronDown" : "chevronRight"} size={12} />
          {t("entry.advanced")}
        </button>
        {showAdvanced && (
          <div className="grid grid-cols-2 gap-3" data-testid="advanced">
            <Field label={t("entry.type")} className="col-span-2">
              <Segmented
                label={t("entry.type")}
                value={type}
                onChange={setType}
                options={[
                  { value: "totp", label: t("entry.totp") },
                  { value: "hotp", label: t("entry.hotp") },
                ]}
              />
            </Field>
            {type === "totp" ? (
              <Input
                label={t("entry.period")}
                type="number"
                inputMode="numeric"
                min={1}
                max={3600}
                value={period}
                onChange={(e) => setPeriod(e.target.value)}
                mono
              />
            ) : (
              <Input
                label={t("entry.counter")}
                type="number"
                inputMode="numeric"
                min={0}
                value={counter}
                onChange={(e) => setCounter(e.target.value)}
                mono
              />
            )}
            <Field label={t("entry.digits")}>
              <Segmented
                label={t("entry.digits")}
                value={digits}
                onChange={setDigits}
                mono
                options={DIGITS.map((d) => ({ value: d, label: d }))}
              />
            </Field>
            <Field label={t("entry.algorithm")} className="col-span-2">
              <Segmented
                label={t("entry.algorithm")}
                value={algorithm}
                onChange={setAlgorithm}
                mono
                options={ALGORITHMS.map((a) => ({ value: a, label: a.toUpperCase() }))}
              />
            </Field>
            {fieldError === "invalid_parameters" && (
              <div className="col-span-2">
                <FormError code={fieldError} />
              </div>
            )}
          </div>
        )}
      </form>
    </Dialog>
  );
}

function AddUriDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const formId = useId();
  const form = useRef<HTMLFormElement>(null);
  const [uri, setUri] = useState("");
  const submit = useSubmit();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    const added = await submit.run(() =>
      backend.dispatch({ command: "entry_add_uri", uri: uri.trim() }),
    );
    if (added !== undefined) onClose();
  };
  return (
    <Dialog
      open
      title={t("entry.uriTitle")}
      onClose={onClose}
      width={520}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            type="submit"
            form={formId}
            loading={submit.busy}
            disabled={uri.trim() === ""}>
            {t("entry.add")}
          </Button>
        </>
      }>
      <form
        id={formId}
        ref={form}
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-2">
        <Textarea
          label={t("entry.uri")}
          mono
          rows={4}
          value={uri}
          onChange={(e) => setUri(e.target.value)}
          // A link is one line: Enter adds it, Shift Enter is left to the field.
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              form.current?.requestSubmit();
            }
          }}
          placeholder={t("entry.uriPlaceholder")}
          spellCheck={false}
          data-autofocus
        />
        <FormError code={submit.error} />
      </form>
    </Dialog>
  );
}

function EditDialog({ entry, onClose }: { entry: EntryView; onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const formId = useId();
  const [issuer, setIssuer] = useState(entry.issuer);
  const [account, setAccount] = useState(entry.account);
  const [group, setGroup] = useState(entry.group ?? "");
  const [favorite, setFavorite] = useState(entry.favorite);
  const [color, setColor] = useState(entry.color);
  const [mark, setMark] = useState(entry.mark ?? "");
  const submit = useSubmit();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    const patch = { issuer, account, group, favorite, color, mark: mark.trim() };
    const saved = await submit.run(() =>
      backend.dispatch({ command: "entry_update", id: entry.id, patch }),
    );
    if (saved !== undefined) onClose();
  };
  return (
    <Dialog
      open
      title={t("entry.editTitle")}
      onClose={onClose}
      width={480}
      facts={`${parametersText(t, entry.kind, entry.algorithm, entry.digits)} · ${originText(t, entry.origin)}`}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" type="submit" form={formId} loading={submit.busy}>
            {t("entry.save")}
          </Button>
        </>
      }>
      <form id={formId} onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
        <Input
          label={t("entry.issuer")}
          value={issuer}
          onChange={(e) => setIssuer(e.target.value)}
          data-autofocus
        />
        <Input
          label={t("entry.account")}
          value={account}
          onChange={(e) => setAccount(e.target.value)}
        />
        <GroupField value={group} onChange={setGroup} />
        <Toggle checked={favorite} onChange={setFavorite} label={t("codes.favorite")} />
        <AccountAppearance
          issuer={issuer}
          account={account}
          color={color}
          mark={mark}
          onColor={setColor}
          onMark={setMark}
        />
        <FormError code={submit.error} />
      </form>
    </Dialog>
  );
}

function DeleteDialog({ entry, onClose }: { entry: EntryView; onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const submit = useSubmit();
  const remove = async () => {
    const done = await submit.run(() =>
      backend.dispatch({ command: "entry_delete", id: entry.id }),
    );
    if (done !== undefined) onClose();
  };
  return (
    <Dialog
      open
      title={t("entry.deleteTitle", { name: entryLabel(entry.issuer, entry.account) })}
      onClose={onClose}
      actions={
        <>
          <Button variant="ghost" onClick={onClose} data-autofocus>
            {t("common.cancel")}
          </Button>
          <Button variant="danger" icon="trash" loading={submit.busy} onClick={() => void remove()}>
            {t("entry.deleteConfirm")}
          </Button>
        </>
      }>
      <p>{t("entry.deleteBody")}</p>
      <div className="mt-2">
        <FormError code={submit.error} />
      </div>
    </Dialog>
  );
}

function RevealDialog({ entry, onClose }: { entry: EntryView; onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const { platform } = useUiState();
  const formId = useId();
  const now = useClock();
  const [password, setPassword] = useState("");
  const [revealed, setRevealed] = useState<{ answer: Revealed; at: number } | undefined>(undefined);
  const submit = useSubmit();
  const shown = revealed !== undefined;
  // However the dialog goes (the button, Esc, the scrim, the vault locking), the secret view ends
  // and the window may be captured again.
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
  useEffect(() => {
    if (shown && left === 0) onClose();
  }, [shown, left, onClose]);
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
      <Dialog
        open
        title={t("entry.revealTitle")}
        onClose={onClose}
        width={440}
        actions={
          <>
            <Button variant="ghost" onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              type="submit"
              form={formId}
              loading={submit.busy}
              disabled={password === ""}>
              {t("entry.revealSubmit")}
            </Button>
          </>
        }>
        <form id={formId} onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
          <p>{t("entry.revealPrompt", { name })}</p>
          <PasswordField
            label={t("unlock.password")}
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            error={submit.error === undefined ? undefined : errorText(t, submit.error)}
            data-autofocus
          />
        </form>
      </Dialog>
    );
  }
  const { answer } = revealed;
  return (
    <Dialog
      open
      title={name}
      onClose={onClose}
      width={600}
      hint={<span data-testid="reveal-countdown">{t("export.hideIn", { s: left })}</span>}
      actions={
        <Button variant="primary" onClick={onClose}>
          {t("common.done")}
        </Button>
      }>
      <div className="flex flex-col gap-4" data-testid="revealed">
        <Banner tone="warn" marker="icon">
          {t("entry.revealWarning")}
        </Banner>
        <div className="flex gap-5">
          <QrView svg={answer.svg} label={t("ui.a11y.qr")} size={200} />
          <div className="flex min-w-0 flex-1 flex-col gap-4">
            <div>
              <div className="text-[12px] text-fg-muted">{t("entry.revealSecret")}</div>
              <div
                className="mono text-[15px] break-all text-fg select-all"
                data-testid="revealed-secret">
                {answer.secret}
              </div>
            </div>
            <div>
              <div className="text-[12px] text-fg-muted">{t("entry.revealUri")}</div>
              <div className="mono text-[11px] break-all text-fg-muted select-all">
                {answer.uri}
              </div>
            </div>
            {platform === "linux" && (
              <p className="text-[12px] text-fg-subtle">{t("entry.linuxCapture")}</p>
            )}
          </div>
        </div>
      </div>
    </Dialog>
  );
}
