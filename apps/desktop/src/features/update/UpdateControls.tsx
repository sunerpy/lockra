import { updateStatusLine } from "@lockra/shared";
import { Button, LampText, useLocale, useT, useUiState } from "@lockra/ui";
import { useDispatch } from "../../app/dispatch";
import { useShell } from "../../app/shell-state";

/** The updater's status line plus the one action its state allows (check / view / restart), after
 *  Voltip's: in full in Settings › General, compact in › About. */
export function UpdateControls({ compact = false }: { compact?: boolean }) {
  const t = useT();
  const locale = useLocale();
  const dispatch = useDispatch();
  const shell = useShell();
  const { update, app_version: current } = useUiState();
  const status = update.status;
  const line = updateStatusLine(update, current, t, locale);
  const unavailable = update.method === null;
  const busy =
    status.state === "checking" || status.state === "downloading" || status.state === "installing";
  return (
    <div className="flex flex-wrap items-center gap-3" data-testid="update-controls">
      <LampText
        tone={line.tone}
        size="sm"
        mono={compact}
        pulse={status.state === "checking" || status.state === "downloading"}>
        <span data-testid="update-status">{line.text}</span>
      </LampText>
      {!unavailable && (status.state === "available" || status.state === "downloading") && (
        <Button
          size="sm"
          variant="primary"
          icon="download"
          onClick={() => shell.setUpdateOpen(true)}
          data-testid="update-view">
          {t("update.view")}
        </Button>
      )}
      {!unavailable && status.state === "ready" && (
        <Button
          size="sm"
          variant="primary"
          icon="refresh"
          onClick={() => void dispatch({ command: "update_install" })}
          data-testid="update-restart">
          {t("update.restart")}
        </Button>
      )}
      {(unavailable ||
        (status.state !== "available" &&
          status.state !== "downloading" &&
          status.state !== "ready")) && (
        <Button
          size="sm"
          variant={compact ? "ghost" : "outline"}
          disabled={busy || unavailable}
          loading={status.state === "checking"}
          onClick={() => void dispatch({ command: "update_check" })}
          data-testid="update-check">
          {status.state === "checking" ? t("update.checking") : t("update.check")}
        </Button>
      )}
    </div>
  );
}
