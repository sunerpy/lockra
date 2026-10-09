// The two sync views that show a secret: the recovery key (the space's sync key), and an
// invitation for another device. Both open once the user proved to be here, hide themselves after
// two minutes like a revealed secret, and however they close, the shell lifts screen-capture
// protection (`secret_view_closed`).
import { REVEAL_SECONDS, type SyncInvite, errorText } from "@lockra/shared";
import {
  Banner,
  Button,
  Dialog,
  PasswordField,
  QrView,
  unlockBiometric,
  useBackend,
  useClock,
  useSaveSyncKey,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useCallback, useEffect, useId, useRef, useState } from "react";
import { useSubmit } from "../../app/dispatch";

/** Seconds left of a secret shown at `at`, on the shared clock (it ticks on whole seconds, so it
 *  can read just before the moment the view opened). */
function secondsLeft(now: number, at: number | undefined): number {
  return at === undefined
    ? REVEAL_SECONDS
    : Math.max(0, REVEAL_SECONDS - Math.max(0, Math.floor((now - at) / 1000)));
}

/** Ends the secret view a command opened in the shell (screen-capture protection on) when its
 *  answer arrives after the view that asked for it went (Settings closed meanwhile): nothing shows
 *  the secret, so nothing would end the view either. Returns `deliver(show)`: it shows the answer
 *  while the asker is still on screen, and otherwise ends the secret view. */
export function useSecretAnswer(): (show: () => void) => void {
  const { backend } = useBackend();
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  return useCallback(
    (show: () => void) => {
      if (mounted.current) show();
      else void backend.dispatch({ command: "secret_view_closed" }).catch(() => undefined);
    },
    [backend],
  );
}

/** While `shown`, a secret is on screen: closing (or unmounting) ends the secret view; at zero
 *  seconds left the view closes itself. */
function useSecretView(shown: boolean, left: number, onClose: () => void): void {
  const { backend } = useBackend();
  useEffect(() => {
    if (!shown) return undefined;
    return () => {
      void backend.dispatch({ command: "secret_view_closed" }).catch(() => undefined);
    };
  }, [shown, backend]);
  useEffect(() => {
    if (shown && left === 0) onClose();
  }, [shown, left, onClose]);
}

function SyncKeyText({ value }: { value: string }) {
  return (
    <div
      className="mono rounded-6 bg-inset px-3 py-2 text-[15px] break-normal text-fg select-all hairline"
      data-testid="sync-key">
      {value}
    </div>
  );
}

/** The recovery key, once the user proved to be here. "Save to a file" uses the password typed to
 *  show it (or the biometric check again); "I have kept it" says it is written down, which ends
 *  Settings › Sync's reminder. */
export function RecoveryKeyDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const { lock } = useUiState();
  const biometric = unlockBiometric(lock);
  const formId = useId();
  const now = useClock();
  const [password, setPassword] = useState("");
  const [shown, setShown] = useState<
    { syncKey: string; password: string | undefined; at: number } | undefined
  >(undefined);
  const submit = useSubmit();
  const deliver = useSecretAnswer();
  const saving = useSaveSyncKey();
  const [saved, setSaved] = useState(false);
  const left = secondsLeft(now, shown?.at);
  useSecretView(shown !== undefined, left, onClose);
  const ask = async (typed?: string) => {
    const answer = await submit.run(() =>
      backend.dispatch(
        typed === undefined
          ? { command: "sync_key_reveal", reason: t("sync.recoveryKey.reason") }
          : { command: "sync_key_reveal", password: typed },
      ),
    );
    setPassword("");
    if (answer !== undefined)
      deliver(() => setShown({ syncKey: answer.sync_key, password: typed, at: Date.now() }));
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    await ask(password);
  };
  if (shown === undefined) {
    return (
      <Dialog
        open
        title={t("sync.recoveryKey.title")}
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
              {t("sync.recoveryKey.submit")}
            </Button>
          </>
        }>
        <form id={formId} onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
          <p>
            {t(biometric === null ? "sync.recoveryKey.prompt" : "sync.recoveryKey.promptBiometric")}
          </p>
          {biometric !== null && (
            <Button
              variant="outline"
              icon="fingerprint"
              className="self-start"
              loading={submit.busy}
              onClick={() => void ask()}>
              {t(`sync.invite.verifyWith.${biometric}`)}
            </Button>
          )}
          <PasswordField
            label={t("sync.masterPassword")}
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
  const save = async () => {
    if ((await saving.save(shown.password)) === true) setSaved(true);
  };
  const kept = async () => {
    await backend.dispatch({ command: "sync_key_acknowledge" }).catch(() => undefined);
    onClose();
  };
  return (
    <Dialog
      open
      title={t("sync.recoveryKey.title")}
      onClose={onClose}
      width={560}
      hint={
        <span data-testid="sync-key-countdown">{t("sync.recoveryKey.hideIn", { s: left })}</span>
      }
      actions={
        <>
          <Button variant="ghost" icon="download" onClick={() => void save()} loading={saving.busy}>
            {t("sync.recoveryKey.save")}
          </Button>
          <Button variant="primary" onClick={() => void kept()} data-autofocus>
            {t("sync.recoveryKey.done")}
          </Button>
        </>
      }>
      <div className="flex flex-col gap-3" data-testid="sync-recovery-key">
        <Banner tone="warn" marker="icon">
          {t("sync.recoveryKey.body")}
        </Banner>
        <div className="text-[12px] text-fg-muted">{t("sync.recoveryKey.key")}</div>
        <SyncKeyText value={shown.syncKey} />
        {saved && (
          <p className="text-[12px] text-ok" role="status" data-testid="sync-key-saved">
            {t("sync.recoveryKey.savedTo")}
          </p>
        )}
        {saving.error !== undefined && (
          <p className="text-[12px] text-danger" role="alert">
            {errorText(t, saving.error)}
          </p>
        )}
      </div>
    </Dialog>
  );
}

/** An invitation for another device, once the user proved to be here: the biometric check that
 *  unlocks this vault, or the master password. A space on a relay offers its pairing link to copy
 *  beside the QR code; elsewhere the text to send is sealed, behind "Can't scan?", with its code. */
export function InviteDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const { platform, lock } = useUiState();
  const biometric = unlockBiometric(lock);
  const formId = useId();
  const now = useClock();
  const [password, setPassword] = useState("");
  const [invite, setInvite] = useState<{ answer: SyncInvite; at: number } | undefined>(undefined);
  const [cantScan, setCantScan] = useState(false);
  const [copied, setCopied] = useState(false);
  const submit = useSubmit();
  const copying = useSubmit();
  const deliver = useSecretAnswer();
  const left = secondsLeft(now, invite?.at);
  useSecretView(invite !== undefined, left, onClose);
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
      <Dialog
        open
        title={t("sync.invite.open")}
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
              {t("sync.invite.submit")}
            </Button>
          </>
        }>
        <form id={formId} onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
          <p>{t(biometric === null ? "sync.invite.prompt" : "sync.invite.promptBiometric")}</p>
          {biometric !== null && (
            <Button
              variant="outline"
              icon="fingerprint"
              className="self-start"
              loading={submit.busy}
              onClick={() => void ask()}>
              {t(`sync.invite.verifyWith.${biometric}`)}
            </Button>
          )}
          <PasswordField
            label={t("sync.masterPassword")}
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
  const { answer } = invite;
  // On a relay nothing is sealed: the invitation itself is the pairing link to copy and send.
  const shared = answer.shared;
  const copy = async () => {
    setCopied(false);
    const done = await copying.run(() =>
      backend.dispatch({ command: "sync_invite_copy", text: shared?.text ?? answer.invite }),
    );
    if (done !== undefined) setCopied(true);
  };
  const copyControls = (label: string, done: string) => (
    <>
      <Button
        variant="outline"
        icon="copy"
        loading={copying.busy}
        onClick={() => void copy()}
        className="self-start">
        {label}
      </Button>
      {copied && (
        <p role="status" className="text-[12px] text-fg-muted">
          {done}
        </p>
      )}
      {copying.error !== undefined && (
        <p role="alert" className="text-[12px] text-danger">
          {errorText(t, copying.error)}
        </p>
      )}
    </>
  );
  return (
    <Dialog
      open
      title={t("sync.invite.open")}
      onClose={onClose}
      width={600}
      hint={<span data-testid="invite-countdown">{t("sync.invite.hideIn", { s: left })}</span>}
      actions={
        <Button variant="primary" onClick={onClose}>
          {t("common.done")}
        </Button>
      }>
      <div className="flex flex-col gap-4" data-testid="sync-invite">
        <Banner tone="warn" marker="icon">
          {t(shared === null ? "sync.invite.linkWarning" : "sync.invite.warning")}
        </Banner>
        {!answer.includes_storage && (
          <p className="text-[13px] text-fg-muted" data-testid="invite-key-only">
            {t("sync.invite.keyOnly")}
          </p>
        )}
        <div className="flex gap-5">
          <QrView svg={answer.svg} label={t("ui.a11y.qr")} size={220} />
          <div className="flex min-w-0 flex-1 flex-col gap-3">
            <p className="text-[13px] text-fg">
              {t(shared === null ? "sync.invite.bodyLink" : "sync.invite.body")}
            </p>
            {shared === null ? (
              <div className="flex flex-col gap-3" data-testid="invite-link">
                {copyControls(t("sync.invite.copyLink"), t("sync.invite.copiedLink"))}
              </div>
            ) : (
              <>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={cantScan ? "chevronDown" : "chevronRight"}
                  aria-expanded={cantScan}
                  onClick={() => setCantScan((open) => !open)}
                  className="self-start">
                  {t("sync.invite.cantScan")}
                </Button>
                {cantScan && (
                  <div className="flex flex-col gap-3" data-testid="invite-send">
                    <p className="text-[12px] text-fg-muted">{t("sync.invite.cantScanBody")}</p>
                    {copyControls(t("sync.invite.copy"), t("sync.invite.copied"))}
                    <div>
                      <div className="text-[12px] text-fg-muted">{t("sync.invite.code")}</div>
                      <div
                        className="mono text-[18px] tracking-wide text-fg select-all"
                        data-testid="invite-code">
                        {shared.code}
                      </div>
                      <p className="text-[12px] text-fg-subtle">{t("sync.invite.codeHint")}</p>
                    </div>
                  </div>
                )}
              </>
            )}
            {platform === "linux" && (
              <p className="text-[12px] text-fg-subtle">{t("entry.linuxCapture")}</p>
            )}
          </div>
        </div>
      </div>
    </Dialog>
  );
}
