// The fingerprint that unlocks this vault stands in for the master password wherever the phone
// asks to prove the user is there (a secret shown, accounts exported, the sync storage changed, a
// space joined or a device invited). Where it is the default unlock (Settings › Security), a page
// that asks for nothing but that proof asks for the fingerprint by itself as it comes to the
// screen, as the lock screen does (@lockra/ui `useUnlockPrompt`); a cancel leaves the password.
import type { BiometricKind, ErrorCode } from "@lockra/shared";
import { pageVisible, unlockBiometric, useUiState, useUnlockPrompt } from "@lockra/ui";

/** The fingerprint's kind where it unlocks this vault, else `null`. */
export function useFingerprint(): BiometricKind | null {
  const { lock } = useUiState();
  return unlockBiometric(lock);
}

/** `ask()` by itself where the fingerprint is the default unlock; the fingerprint, as above. */
export function useAutoFingerprint(ask: () => Promise<unknown>): BiometricKind | null {
  const { settings } = useUiState();
  const fingerprint = useFingerprint();
  useUnlockPrompt({
    active: fingerprint !== null && settings.default_unlock === "biometric",
    presence: pageVisible,
    prompt: ask,
  });
  return fingerprint;
}

/** A failure to show: a cancelled check is the user's own choice, as on the lock screen. */
export function shown(code: ErrorCode | undefined): ErrorCode | undefined {
  return code === "biometric_cancelled" ? undefined : code;
}
