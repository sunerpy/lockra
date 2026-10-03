// Join a sync space with this phone's vault: its accounts join the space's, and the page closes
// on the sync page, now with the space.
import { useT } from "@lockra/ui";
import { useNav } from "../app/nav";
import { JoinSync } from "../components/JoinSync";
import { Page } from "../components/Page";

export function SyncJoin() {
  const t = useT();
  const nav = useNav();
  return (
    <Page title={t("sync.off.joinTitle")} testId="page-sync-join">
      <div className="flex flex-col gap-4">
        <p className="text-[14px] text-fg-muted">{t("sync.off.joinBody")}</p>
        <JoinSync onJoined={nav.back} />
      </div>
    </Page>
  );
}
