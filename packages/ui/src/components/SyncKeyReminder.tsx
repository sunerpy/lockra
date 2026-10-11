// The reminder to save the recovery key (the space's sync key), on the device that made the space,
// until the key is saved to a file or written down from the view that shows it (Settings › Sync,
// on the desktop and the phone).
import { errorText } from "@lockra/shared";
import { type SubmitEvent, useState } from "react";
import { useUiState } from "../backend/BackendProvider";
import { unlockBiometric, useSaveSyncKey } from "../hooks/presence";
import { useT } from "../i18n/I18nProvider";
import { Banner } from "./Banner";
import { Button } from "./Button";
import { PasswordField } from "./PasswordField";

/** `onShow` opens the platform's view of the key (a dialog, a page), where "I have kept it" ends
 *  the reminder. Saving asks the biometric check that unlocks this vault, else (or when it cannot
 *  be used) the master password. `lg` is the phone's: taller controls. */
export function SyncKeyReminder({
  onShow,
  size = "md",
}: {
  onShow: () => void;
  size?: "md" | "lg";
}) {
  const t = useT();
  const { sync, lock } = useUiState();
  const saving = useSaveSyncKey();
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
  const error = saving.error;
  return (
    <div data-testid="sync-key-reminder">
      <Banner
        tone="warn"
        marker="icon"
        actions={
          <>
            <Button size={size} variant="primary" onClick={onShow}>
              {t("sync.keyReminder.show")}
            </Button>
            <Button size={size} variant="ghost" onClick={onSave} loading={saving.busy}>
              {t("sync.keyReminder.save")}
            </Button>
          </>
        }>
        <div className="flex flex-col gap-3">
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
    </div>
  );
}
