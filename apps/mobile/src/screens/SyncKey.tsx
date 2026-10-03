// The sync key of the space just set up, shown once (an invitation shows it again): it hides
// after REVEAL_SECONDS like any secret (app/secret-page.ts).
import { Banner, Button, useT } from "@lockra/ui";
import { useState } from "react";
import { useNav } from "../app/nav";
import { useSecretPage } from "../app/secret-page";
import { Page } from "../components/Page";
import { SyncKeyText } from "../components/SyncKeyText";

export function SyncKey({ syncKey }: { syncKey: string }) {
  const t = useT();
  const nav = useNav();
  const [at] = useState(() => Date.now());
  const left = useSecretPage(at);
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
        <Button variant="primary" size="lg" onClick={nav.back}>
          {t("sync.created.done")}
        </Button>
      </div>
    </Page>
  );
}
