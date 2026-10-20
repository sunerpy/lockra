// The background is of another build than the page (an older one the browser still runs after
// Lockra's folder changed under it, LOCKRA_BUILD): the popup (under its header) and the options
// page (in the connection) say so and offer the reload, which nothing else brings about.
import { Button, Icon, cx } from "@lockra/ui";
import type { PopupState } from "./messages";
import { useText } from "./text";

/** A background that says another build, or none (older than this check), and waits for no new
 *  copy: one that does takes it once the pages close, and the update note says so. */
export function staleBackground(state: PopupState | null): boolean {
  return state !== null && state.build !== LOCKRA_BUILD && state.update !== "waiting";
}

export function StaleNote({ onReload, className }: { onReload: () => void; className?: string }) {
  const t = useText();
  return (
    <div
      role="alert"
      className={cx(
        "flex shrink-0 flex-col items-start gap-1.5 rounded-md border-l-[3px] border-l-warning bg-warning-soft px-2.5 py-1.5 text-[11px] leading-4 text-fg",
        className,
      )}
      data-testid="popup-stale">
      <span className="flex items-start gap-1.5">
        <Icon name="refresh" size={13} className="mt-px shrink-0" />
        <span>{t("selfUpdate.stale")}</span>
      </span>
      <Button variant="outline" size="sm" icon="refresh" onClick={onReload}>
        {t("selfUpdate.reload")}
      </Button>
    </div>
  );
}
