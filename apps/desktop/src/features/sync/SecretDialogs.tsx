// The two sync views that show a secret: the new space's sync key, and an invitation for another
// device. Both hide themselves after two minutes like a revealed secret, and however they close,
// the shell lifts screen-capture protection (`secret_view_closed`).
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

/** The sync key of a space just created: shown once (an invitation shows it again). "Save to a
 *  file" uses `password`, the one the space was just made with; "I have kept it" says the key is
 *  written down. Closed otherwise, Settings › Sync goes on reminding of it. */
export function SyncKeyDialog({
  syncKey,
  password,
  onClose,
}: {
  syncKey: string;
  password: string;
  onClose: () => void;
}) {
  const t = useT();
  const { backend } = useBackend();
  const now = useClock();
  const [at] = useState(() => Date.now());
  const left = secondsLeft(now, at);
  const saving = useSaveSyncKey();
  const [saved, setSaved] = useState(false);
  useSecretView(true, left, onClose);
  const save = async () => {
    if ((await saving.save(password)) === true) setSaved(true);
  };
  const kept = async () => {
    await backend.dispatch({ command: "sync_key_acknowledge" }).catch(() => undefined);
    onClose();
  };
  return (
    <Dialog
      open
      title={t("sync.created.title")}
      onClose={onClose}
      width={560}
      hint={<span data-testid="sync-key-countdown">{t("sync.created.hideIn", { s: left })}</span>}
      actions={
        <>
          <Button variant="ghost" icon="download" onClick={() => void save()} loading={saving.busy}>
            {t("sync.created.save")}
          </Button>
          <Button variant="primary" onClick={() => void kept()} data-autofocus>
            {t("sync.created.done")}
          </Button>
        </>
      }>
      <div className="flex flex-col gap-3" data-testid="sync-created">
        <Banner tone="warn" marker="icon">
          {t("sync.created.body")}
        </Banner>
        <div className="text-[12px] text-fg-muted">{t("sync.created.key")}</div>
        <SyncKeyText value={syncKey} />
        <p className="text-[12px] text-fg-subtle">{t("sync.created.again")}</p>
        {saved && (
          <p className="text-[12px] text-ok" role="status" data-testid="sync-key-saved">
            {t("sync.created.savedTo")}
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
 *  unlocks this vault, or the master password. */
export function InviteDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const { platform, lock } = useUiState();
  const biometric = unlockBiometric(lock);
  const formId = useId();
  const now = useClock();
  const [password, setPassword] = useState("");
  const [invite, setInvite] = useState<{ answer: SyncInvite; at: number } | undefined>(undefined);
  const submit = useSubmit();
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
  return (
    <Dialog
      open
      title={t("sync.invite.open")}
      onClose={onClose}
      width={640}
      hint={<span data-testid="invite-countdown">{t("sync.invite.hideIn", { s: left })}</span>}
      actions={
        <Button variant="primary" onClick={onClose}>
          {t("common.done")}
        </Button>
      }>
      <div className="flex flex-col gap-4" data-testid="sync-invite">
        <Banner tone="warn" marker="icon">
          {t("sync.invite.warning")}
        </Banner>
        <div className="flex gap-5">
          <QrView svg={answer.svg} label={t("ui.a11y.qr")} size={220} />
          <div className="flex min-w-0 flex-1 flex-col gap-3">
            <p className="text-[13px] text-fg">{t("sync.invite.body")}</p>
            <div>
              <div className="text-[12px] text-fg-muted">{t("sync.invite.text")}</div>
              <div
                className="mono max-h-28 overflow-auto text-[11px] break-all text-fg-muted select-all"
                data-testid="invite-text">
                {answer.shared_text}
              </div>
            </div>
            <div>
              <div className="text-[12px] text-fg-muted">{t("sync.invite.code")}</div>
              <div
                className="mono text-[18px] tracking-wide text-fg select-all"
                data-testid="invite-code">
                {answer.code}
              </div>
              <p className="text-[12px] text-fg-subtle">{t("sync.invite.codeHint")}</p>
            </div>
            <div>
              <div className="text-[12px] text-fg-muted">{t("sync.created.key")}</div>
              <SyncKeyText value={answer.sync_key} />
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
