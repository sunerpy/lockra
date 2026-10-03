// The locked vault: the master password, or the fingerprint once it is turned on. Wrong passwords
// slow down (the core's rate limit).
import { type ErrorCode, errorText } from "@lockra/shared";
import {
  Button,
  Logo,
  PasswordField,
  useBackend,
  useClock,
  useI18n,
  useSubmit,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";

export function Unlock() {
  const { t } = useI18n();
  const { backend } = useBackend();
  const { lock } = useUiState();
  const now = useClock();
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const device = useSubmit();
  const { available, enabled, biometric } = lock.device_unlock;
  // The phone remembers the key only behind the fingerprint, and only while one is enrolled.
  const fingerprint = enabled && biometric.enabled && available;
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
  return (
    <main
      className="flex min-h-full flex-col justify-center gap-8 px-5 pt-[max(env(safe-area-inset-top),2rem)] pb-[max(env(safe-area-inset-bottom),2rem)]"
      data-testid="page-unlock">
      <header className="flex flex-col items-center gap-3 text-center">
        <Logo size={56} />
        <h1 className="text-[20px] font-semibold text-fg">{t("unlock.title")}</h1>
        <p className="text-[14px] text-fg-muted">{t("unlock.subtitle")}</p>
      </header>
      <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
        <PasswordField
          label={t("unlock.password")}
          value={password}
          onChange={setPassword}
          size="lg"
          autoComplete="current-password"
          error={failure(submit.error)}
        />
        {waitSeconds > 0 && (
          <p role="status" className="text-[13px] text-warning" data-testid="retry-wait">
            {t("unlock.retryIn", { s: waitSeconds })}
          </p>
        )}
        <Button
          variant="primary"
          type="submit"
          size="lg"
          icon="unlock"
          loading={submit.busy}
          disabled={password === "" || waitSeconds > 0}>
          {t("unlock.submit")}
        </Button>
      </form>
      {fingerprint && (
        <div className="flex flex-col gap-2">
          <Button
            size="lg"
            icon="fingerprint"
            loading={device.busy}
            onClick={() =>
              void device.run(() =>
                backend.dispatch({
                  command: "vault_unlock_device",
                  // The words of the system's prompt, in the interface's language.
                  reason: t("unlock.biometricReason"),
                }),
              )
            }>
            {t("unlock.biometric.fingerprint")}
          </Button>
          {/* A cancelled check is the user's own choice: nothing to say. */}
          {device.error !== undefined && device.error !== "biometric_cancelled" && (
            <p role="alert" className="text-[13px] text-danger">
              {errorText(t, device.error)}
            </p>
          )}
        </div>
      )}
      {/* A fingerprint here but not turned on: where to turn it on, once unlocked. */}
      {!fingerprint && biometric.kind !== null && (
        <p className="text-center text-[13px] text-fg-muted" data-testid="biometric-offer">
          {t(`unlock.biometricOffer.${biometric.kind}`)}
        </p>
      )}
    </main>
  );
}
