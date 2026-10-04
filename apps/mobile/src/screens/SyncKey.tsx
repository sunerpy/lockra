// The sync key of the space just set up, shown once (an invitation shows it again): it hides
// after REVEAL_SECONDS like any secret (app/secret-page.ts). "Save to a file" uses the password
// the space was just made with; "I have kept it" says the key is written down. Left otherwise,
// Settings › Sync goes on reminding of it.
import { errorText } from "@lockra/shared";
import { Banner, Button, useBackend, useSaveSyncKey, useT } from "@lockra/ui";
import { useState } from "react";
import { useNav } from "../app/nav";
import { useSecretPage } from "../app/secret-page";
import { Page } from "../components/Page";
import { SyncKeyText } from "../components/SyncKeyText";

export function SyncKey({ syncKey, password }: { syncKey: string; password: string }) {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const [at] = useState(() => Date.now());
  const left = useSecretPage(at);
  const saving = useSaveSyncKey();
  const [saved, setSaved] = useState(false);
  const save = async () => {
    if ((await saving.save(password)) === true) setSaved(true);
  };
  const kept = async () => {
    await backend.dispatch({ command: "sync_key_acknowledge" }).catch(() => undefined);
    nav.back();
  };
  return (
    <Page title={t("sync.created.title")} testId="page-sync-key">
      <div className="flex flex-col gap-4">
        <Banner tone="warn" marker="icon">
          {t("sync.created.body")}
        </Banner>
        <div className="flex flex-col gap-1.5">
          <span className="text-[13px] text-fg-muted">{t("sync.created.key")}</span>
          <SyncKeyText value={syncKey} />
        </div>
        <p className="text-[13px] text-fg-subtle">{t("sync.created.again")}</p>
        <p className="text-[13px] text-fg-subtle" data-testid="sync-key-countdown">
          {t("sync.created.hideIn", { s: left })}
        </p>
        {saved && (
          <p className="text-[13px] text-ok" role="status" data-testid="sync-key-saved">
            {t("sync.created.savedTo")}
          </p>
        )}
        {saving.error !== undefined && (
          <p className="text-[13px] text-danger" role="alert">
            {errorText(t, saving.error)}
          </p>
        )}
        <Button
          variant="outline"
          size="lg"
          icon="download"
          loading={saving.busy}
          onClick={() => void save()}>
          {t("sync.created.save")}
        </Button>
        <Button variant="primary" size="lg" onClick={() => void kept()}>
          {t("sync.created.done")}
        </Button>
      </div>
    </Page>
  );
}
