// Settings › Security › Unlock with fingerprint. The phone remembers the vault's key only behind
// the fingerprint: turning it on makes the device key and its check in one go, after one passed
// check (device_biometric_enable); turning it off, after the master password, removes the key
// (device_unlock_disable), since without the check the Keystore would not give it back anyway.
import { errorText } from "@lockra/shared";
import { Button, Dialog, PasswordField, useBackend, useSubmit, useT, useUiState } from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { SwitchRow } from "./Rows";

export function FingerprintSetting() {
  const t = useT();
  const { backend } = useBackend();
  const { lock } = useUiState();
  const { enabled, biometric } = lock.device_unlock;
  const on = enabled && biometric.enabled;
  const [disabling, setDisabling] = useState(false);
  const submit = useSubmit();
  // No fingerprint enrolled on this phone: nothing to turn on.
  if (biometric.kind !== "fingerprint" && !on) return null;
  const turnOn = () =>
    void submit.run(() =>
      backend.dispatch({
        command: "device_biometric_enable",
        reason: t("settings.security.biometricReason.fingerprint"),
      }),
    );
  return (
    <>
      <SwitchRow
        label={t("settings.security.biometric.fingerprint")}
        hint={t("settings.security.biometricHint.fingerprint")}
        checked={on}
        disabled={submit.busy}
        onChange={(next) => (next ? turnOn() : setDisabling(true))}
        testId="settings-fingerprint"
      />
      {/* A cancelled check is the user's own choice: nothing to say. */}
      {submit.error !== undefined && submit.error !== "biometric_cancelled" && (
        <p role="alert" className="px-4 pb-3 text-[13px] text-danger">
          {errorText(t, submit.error)}
        </p>
      )}
      {disabling && <TurnOff onClose={() => setDisabling(false)} />}
    </>
  );
}

function TurnOff({ onClose }: { onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    const done = await submit.run(() =>
      backend.dispatch({ command: "device_unlock_disable", password }),
    );
    setPassword("");
    if (done !== undefined) onClose();
  };
  return (
    <Dialog
      open
      title={t("mobile.fingerprint.disableTitle")}
      onClose={onClose}
      actions={
        <>
          <Button variant="ghost" size="lg" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="danger"
            size="lg"
            type="submit"
            form="fingerprint-off"
            loading={submit.busy}
            disabled={password === ""}>
            {t("common.confirm")}
          </Button>
        </>
      }>
      <form
        id="fingerprint-off"
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-3"
        data-testid="fingerprint-off">
        <p>{t("mobile.fingerprint.disablePrompt")}</p>
        <PasswordField
          size="lg"
          label={t("unlock.password")}
          value={password}
          onChange={setPassword}
          autoComplete="current-password"
          error={submit.error === undefined ? undefined : errorText(t, submit.error)}
        />
      </form>
    </Dialog>
  );
}
