// Settings › About: is there a newer Lockra? Asked only on the button, from the release manifest on
// GitHub (no account or vault data goes with it); a newer release opens its page in the phone's
// browser, whose APK installs over this one. A copy from Google Play updates through the store.
import { updateStatusLine } from "@lockra/shared";
import {
  Button,
  LampText,
  useBackend,
  useDispatch,
  useGuarded,
  useI18n,
  useUiState,
} from "@lockra/ui";
import { useState } from "react";

export function UpdateRow() {
  const { t, locale } = useI18n();
  const { update, app_version: version } = useUiState();
  const dispatch = useDispatch();
  const { backend } = useBackend();
  const guarded = useGuarded();
  // The page's address, where no browser opened it.
  const [address, setAddress] = useState<string | null>(null);
  if (update.method === null) return null;
  const line = updateStatusLine(update, version, t, locale);
  const checking = update.status.state === "checking";
  const open = async () => {
    setAddress((await guarded(() => backend.openRelease())) ?? null);
  };
  return (
    <div className="flex flex-col gap-3 px-4 py-3" data-testid="about-update">
      <LampText tone={line.tone} pulse={checking}>
        <span data-testid="update-status">{line.text}</span>
      </LampText>
      <div className="flex flex-wrap gap-2">
        <Button
          size="lg"
          icon="refresh"
          loading={checking}
          onClick={() => void dispatch({ command: "update_check" })}>
          {t("update.check")}
        </Button>
        {update.status.state === "available" && (
          <Button variant="primary" size="lg" icon="external" onClick={() => void open()}>
            {t("mobile.update.open")}
          </Button>
        )}
      </div>
      {address !== null && (
        <p className="mono text-[13px] break-all text-fg" data-testid="update-address">
          {t("mobile.update.address", { url: address })}
        </p>
      )}
      <p className="text-[13px] text-fg-muted">{t("mobile.update.hint")}</p>
    </div>
  );
}
