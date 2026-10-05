// Pair this phone with a computer's LAN hub: with a vault, its accounts join the computer's space
// (or a space of this phone gets the LAN beside its storage), and the page closes on the sync
// page.
import { useT } from "@lockra/ui";
import { useNav } from "../app/nav";
import { Page } from "../components/Page";
import { PairSync } from "../components/PairSync";

export function SyncPair() {
  const t = useT();
  const nav = useNav();
  return (
    <Page title={t("mobile.sync.pairTitle")} testId="page-sync-pair">
      <div className="flex flex-col gap-4">
        <p className="text-[14px] text-fg-muted">{t("mobile.sync.pairBody")}</p>
        <PairSync onPaired={nav.back} />
      </div>
    </Page>
  );
}
