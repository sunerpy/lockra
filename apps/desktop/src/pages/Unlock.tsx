// The locked vault: the master password, or the key remembered on this device. Wrong passwords
// slow down (the core's rate limit); a forgotten password leads to a reset that keeps the old file.
// Where Touch ID or Windows Hello is the default unlock (Settings › Security), its button comes
// first and the system's check asks by itself (@lockra/ui `useUnlockPrompt`).
import { type ErrorCode, errorText } from "@lockra/shared";
import {
  Button,
  Dialog,
  Input,
  Logo,
  PasswordField,
  useBackend,
  useClock,
  useI18n,
  useUiState,
  useUnlockPrompt,
  windowFocus,
} from "@lockra/ui";
import { type SubmitEvent, useCallback, useState } from "react";
import { useSubmit } from "../app/dispatch";
import { biometricName } from "../app/platform";

export function Unlock() {
  const { t } = useI18n();
  const { backend } = useBackend();
  const { lock, platform, settings } = useUiState();
  const now = useClock();
  const [password, setPassword] = useState("");
  const [resetOpen, setResetOpen] = useState(false);
  const submit = useSubmit();
  const device = useSubmit();
  const retryAt = lock.retry_at_ms;
  const waitSeconds = retryAt !== null && retryAt > now ? Math.ceil((retryAt - now) / 1000) : 0;
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "" || waitSeconds > 0) return;
    await submit.run(() => backend.dispatch({ command: "vault_unlock", password }));
    setPassword("");
  };
  const failure = (code: ErrorCode | undefined): string | undefined => {
    if (code === undefined || code === "rate_limited") return undefined;
    if (code === "wrong_password" && lock.failed_attempts > 0)
      return t("unlock.failed", { n: lock.failed_attempts });
    return errorText(t, code);
  };
  const { available, enabled, biometric } = lock.device_unlock;
  const checkFirst = enabled && biometric.enabled && settings.default_unlock === "biometric";
  const { run: runDevice } = device;
  const unlockWithDevice = useCallback(
    () =>
      runDevice(() =>
        backend.dispatch({
          command: "vault_unlock_device",
          // The words of the system's prompt, in the interface's language.
          reason: biometric.enabled ? t("unlock.biometricReason") : undefined,
        }),
      ),
    [runDevice, backend, biometric.enabled, t],
  );
  // Only while the system offers the check now: a sensor that is away keeps its button, not the
  // prompts.
  useUnlockPrompt({
    active: checkFirst && available && biometric.kind !== null,
    presence: windowFocus,
    prompt: unlockWithDevice,
  });
  const deviceUnlock = enabled && (
    <div className="flex flex-col gap-1.5">
      <Button
        variant={checkFirst ? "primary" : undefined}
        icon={biometric.enabled ? "fingerprint" : "key"}
        loading={device.busy}
        disabled={!available}
        title={available ? undefined : t("settings.security.deviceUnavailable")}
        onClick={() => void unlockWithDevice()}>
        {biometric.enabled
          ? t(`unlock.biometric.${biometricName(biometric.kind, platform)}`)
          : t("unlock.device")}
      </Button>
      {/* A cancelled check is the user's own choice: nothing to say. */}
      {device.error !== undefined && device.error !== "biometric_cancelled" && (
        <p role="alert" className="text-[12px] text-danger">
          {errorText(t, device.error)}
        </p>
      )}
    </div>
  );
  return (
    <div className="flex min-h-full items-center justify-center p-6" data-testid="page-unlock">
      <div className="flex w-full max-w-[380px] flex-col gap-5 rounded-14 bg-surface p-8 hairline">
        <div className="flex flex-col items-center gap-3 text-center">
          <Logo size={44} />
          <div>
            <h1 className="text-[16px] font-semibold text-fg">{t("unlock.title")}</h1>
            <p className="mt-1 text-[13px] text-fg-muted">
              {checkFirst
                ? t(`unlock.subtitleCheck.${biometricName(biometric.kind, platform)}`)
                : t("unlock.subtitle")}
            </p>
          </div>
        </div>
        {checkFirst && deviceUnlock}
        <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
          <PasswordField
            label={t("unlock.password")}
            value={password}
            onChange={setPassword}
            autoFocus
            autoComplete="current-password"
            error={failure(submit.error)}
          />
          {waitSeconds > 0 && (
            <p role="status" className="text-[12px] text-warning" data-testid="retry-wait">
              {t("unlock.retryIn", { s: waitSeconds })}
            </p>
          )}
          <Button
            variant={checkFirst ? undefined : "primary"}
            type="submit"
            icon="unlock"
            loading={submit.busy}
            disabled={password === "" || waitSeconds > 0}>
            {t("unlock.submit")}
          </Button>
        </form>
        {!checkFirst && deviceUnlock}
        {/* Touch ID here but not set up: where to turn it on, once unlocked. */}
        {!biometric.enabled && biometric.kind !== null && (
          <p
            className="text-center text-[12px] text-balance text-fg-muted"
            data-testid="biometric-offer">
            {t(`unlock.biometricOffer.${biometric.kind}`)}
          </p>
        )}
        <Button
          variant="text-muted"
          size="sm"
          className="self-center"
          onClick={() => setResetOpen(true)}>
          {t("unlock.forgot")}
        </Button>
      </div>
      {resetOpen && <ResetDialog onClose={() => setResetOpen(false)} />}
    </div>
  );
}

function ResetDialog({ onClose }: { onClose: () => void }) {
  const { t } = useI18n();
  const { backend } = useBackend();
  const [word, setWord] = useState("");
  const submit = useSubmit();
  const confirmed = word.trim() === t("unlock.resetConfirmWord");
  return (
    <Dialog
      open
      title={t("unlock.resetTitle")}
      onClose={onClose}
      width={460}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="danger"
            loading={submit.busy}
            disabled={!confirmed}
            onClick={() => void submit.run(() => backend.dispatch({ command: "vault_reset" }))}>
            {t("unlock.resetSubmit")}
          </Button>
        </>
      }>
      <div className="flex flex-col gap-3">
        <p>{t("unlock.resetBody")}</p>
        <Input
          label={t("unlock.resetConfirmLabel")}
          value={word}
          onChange={(e) => setWord(e.target.value)}
          data-autofocus
          autoComplete="off"
        />
        {submit.error !== undefined && (
          <p role="alert" className="text-[12px] text-danger">
            {errorText(t, submit.error)}
          </p>
        )}
      </div>
    </Dialog>
  );
}
