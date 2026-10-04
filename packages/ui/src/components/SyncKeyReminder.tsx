// The reminder to save the sync key, on the device that made the space, until the key is saved to
// a file or the user says it is written down (Settings › Sync, on the desktop and the phone).
import { errorText } from "@lockra/shared";
import { type SubmitEvent, useState } from "react";
import { useBackend, useUiState } from "../backend/BackendProvider";
import { useSubmit } from "../hooks/dispatch";
import { unlockBiometric, useSaveSyncKey } from "../hooks/presence";
import { useT } from "../i18n/I18nProvider";
import { Banner } from "./Banner";
import { Button } from "./Button";
import { PasswordField } from "./PasswordField";

/** Saving asks the biometric check that unlocks this vault, else (or when it cannot be used) the
 *  master password. `lg` is the phone's: taller controls. */
export function SyncKeyReminder({ size = "md" }: { size?: "md" | "lg" }) {
  const t = useT();
  const { backend } = useBackend();
  const { sync, lock } = useUiState();
  const saving = useSaveSyncKey();
  const acknowledge = useSubmit();
  const [password, setPassword] = useState("");
  if (sync.space === null || sync.space.key_saved) return null;
  const biometric = unlockBiometric(lock);
  const onSave = () => {
    if (biometric === null) saving.askPassword();
    else void saving.save();
  };
  const onPassword = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    await saving.save(password);
    setPassword("");
  };
  const error = saving.error ?? acknowledge.error;
  return (
    <Banner
      tone="warn"
      marker="icon"
      actions={
        <>
          <Button size={size} variant="primary" onClick={onSave} loading={saving.busy}>
            {t("sync.keyReminder.save")}
          </Button>
          <Button
            size={size}
            variant="ghost"
            loading={acknowledge.busy}
            onClick={() =>
              void acknowledge.run(() => backend.dispatch({ command: "sync_key_acknowledge" }))
            }>
            {t("sync.keyReminder.done")}
          </Button>
        </>
      }>
      <div className="flex flex-col gap-3" data-testid="sync-key-reminder">
        <span>{t("sync.keyReminder.body")}</span>
        {saving.needsPassword && (
          <form onSubmit={(e) => void onPassword(e)} className="flex flex-col gap-2">
            <PasswordField
              size={size}
              label={t("sync.keyReminder.password")}
              value={password}
              onChange={setPassword}
              autoComplete="current-password"
              error={error === undefined ? undefined : errorText(t, error)}
            />
            <Button
              size={size}
              variant="primary"
              type="submit"
              loading={saving.busy}
              disabled={password === ""}
              className="self-start">
              {t("sync.keyReminder.submit")}
            </Button>
          </form>
        )}
        {!saving.needsPassword && error !== undefined && (
          <p role="alert" className="text-[12px] text-danger">
            {errorText(t, error)}
          </p>
        )}
      </div>
    </Banner>
  );
}
