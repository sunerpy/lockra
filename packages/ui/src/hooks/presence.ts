// Proving the user is at this device before something shows or writes a secret (the sync
// invitation, the sync key file): the biometric check that unlocks this vault when it is on, else
// the master password. A check that cannot be used falls back to the password; nothing is sealed
// under it, so it may stand in for the password here.
import {
  type BiometricKind,
  type ErrorCode,
  type SaveSyncKey,
  type UiState,
  isLockraError,
} from "@lockra/shared";
import { useCallback, useState } from "react";
import { useBackend } from "../backend/BackendProvider";
import { useT } from "../i18n/I18nProvider";
import { useSubmit } from "./dispatch";

/** The biometric check that unlocks this vault, or `null` when there is none to ask. */
export function unlockBiometric(lock: UiState["lock"]): BiometricKind | null {
  const { biometric } = lock.device_unlock;
  return biometric.enabled ? biometric.kind : null;
}

/** The answers after which the master password is asked instead of the biometric check. */
export function biometricFellThrough(code: ErrorCode | undefined): boolean {
  return (
    code === "biometric_unavailable" ||
    code === "biometric_failed" ||
    code === "biometric_cancelled"
  );
}

/** What the shell writes for `saveSyncKey`, in the app's words: the key goes in its slot in Rust. */
export function syncKeyFile(t: ReturnType<typeof useT>, password?: string): SaveSyncKey {
  return {
    password,
    reason: password === undefined ? t("sync.keyFile.reason") : undefined,
    fileName: t("sync.keyFile.name"),
    template: `${t("sync.keyFile.heading")}\n\n{{sync_key}}\n\n${t("sync.keyFile.body")}\n`,
  };
}

/**
 * Saving the sync key to a file. `save(password?)` asks the shell; without a password it uses the
 * biometric check, and when that cannot be used `needsPassword` turns on for the caller to ask
 * for the password. `true` once saved, `false` when the dialog was left.
 */
export function useSaveSyncKey() {
  const t = useT();
  const { backend } = useBackend();
  const submit = useSubmit();
  const [needsPassword, setNeedsPassword] = useState(false);
  const { run } = submit;
  const save = useCallback(
    (password?: string) =>
      run(async () => {
        try {
          return await backend.saveSyncKey(syncKeyFile(t, password));
        } catch (failure: unknown) {
          if (
            password === undefined &&
            isLockraError(failure) &&
            biometricFellThrough(failure.code)
          )
            setNeedsPassword(true);
          throw failure;
        }
      }),
    [backend, run, t],
  );
  return { ...submit, save, needsPassword, askPassword: () => setNeedsPassword(true) };
}
