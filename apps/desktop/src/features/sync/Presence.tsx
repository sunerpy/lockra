// Proving the user is at this computer before a sync step that shows a secret or changes the
// space: the biometric check that unlocks this vault when it is on, else the master password.
import { errorText } from "@lockra/shared";
import { Button, Dialog, PasswordField, unlockBiometric, useT, useUiState } from "@lockra/ui";
import { type SubmitEvent, useId, useState } from "react";
import { useSubmit } from "../../app/dispatch";

/** The proof a command takes: the password typed, or the reason the biometric prompt shows. */
export type Presence = { password: string } | { reason: string };

/** Asks for the proof; `onConfirm` dispatches with it, and what it fails with shows under the
 *  password. `promptBiometric` is the sentence when a biometric check is offered too. */
export function PresenceDialog({
  title,
  prompt,
  promptBiometric,
  reason,
  submitLabel,
  onConfirm,
  onClose,
}: {
  title: string;
  prompt: string;
  promptBiometric: string;
  reason: string;
  submitLabel: string;
  onConfirm: (presence: Presence) => Promise<unknown>;
  onClose: () => void;
}) {
  const t = useT();
  const { lock } = useUiState();
  const biometric = unlockBiometric(lock);
  const formId = useId();
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const confirm = async (presence: Presence) => {
    await submit.run(() => onConfirm(presence));
    setPassword("");
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    await confirm({ password });
  };
  return (
    <Dialog
      open
      title={title}
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
            {submitLabel}
          </Button>
        </>
      }>
      <form id={formId} onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
        <p>{biometric === null ? prompt : promptBiometric}</p>
        {biometric !== null && (
          <Button
            variant="outline"
            icon="fingerprint"
            className="self-start"
            loading={submit.busy}
            onClick={() => void confirm({ reason })}>
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
