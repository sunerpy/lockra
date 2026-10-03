import {
  downloadProgress,
  errorText,
  formatBytes,
  formatDateTime,
  statusVersion,
  updateStatusLine,
} from "@lockra/shared";
import {
  Button,
  Dialog,
  LampText,
  Progress,
  useBackend,
  useLocale,
  useT,
  useUiState,
} from "@lockra/ui";
import type { ReactNode } from "react";
import { useDispatch } from "../../app/dispatch";
import { etaText, useDownloadRate } from "./download-rate";
import { ReleaseNotes } from "./release-notes";

export interface UpdateDialogProps {
  open: boolean;
  onClose: () => void;
}

/** The update dialog, after Voltip's: what the new version brings, then the download with its
 *  speed and the time left, then the restart. The release notes come from the updater's own
 *  manifest (`latest.json`), rendered as text: nothing in them is a live link. It stays open
 *  across the states, so one dialog walks the user from 发现新版本 to 重启并更新; closing it
 *  never cancels a download, which carries on in the background. */
export function UpdateDialog({ open, onClose }: UpdateDialogProps) {
  const t = useT();
  const locale = useLocale();
  const { backend } = useBackend();
  const dispatch = useDispatch();
  const { update, app_version: current } = useUiState();
  const rate = useDownloadRate(backend);
  const status = update.status;
  const version = statusVersion(status);

  const title =
    status.state === "available" || status.state === "downloading"
      ? t("update.title", { version: status.version })
      : status.state === "ready"
        ? t("update.titleReady", { version: status.version })
        : status.state === "installing"
          ? t("update.titleInstalling", { version: status.version })
          : status.state === "failed"
            ? t("update.titleFailed")
            : t("update.titleStatus");

  const install = () => void dispatch({ command: "update_install" });
  const retry = () => void dispatch({ command: "update_check" });

  let actions: ReactNode;
  switch (status.state) {
    case "available":
      actions = (
        <>
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t("update.later")}
          </Button>
          <Button size="sm" variant="primary" icon="download" onClick={install} data-autofocus>
            {t("update.install")}
          </Button>
        </>
      );
      break;
    case "downloading":
      actions = (
        <Button size="sm" variant="ghost" onClick={onClose} data-autofocus>
          {t("update.background")}
        </Button>
      );
      break;
    case "ready":
      actions = (
        <>
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t("update.later")}
          </Button>
          <Button size="sm" variant="primary" icon="refresh" onClick={install} data-autofocus>
            {t("update.restart")}
          </Button>
        </>
      );
      break;
    case "installing":
      actions = (
        <Button size="sm" variant="primary" disabled loading>
          {t("update.installing")}
        </Button>
      );
      break;
    case "failed":
      actions = (
        <>
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t("update.close")}
          </Button>
          <Button size="sm" variant="primary" icon="refresh" onClick={retry} data-autofocus>
            {t("update.retry")}
          </Button>
        </>
      );
      break;
    default:
      actions = (
        <Button size="sm" variant="ghost" onClick={onClose} data-autofocus>
          {t("update.close")}
        </Button>
      );
  }

  const notes = status.state === "available" ? status.notes : null;
  const date = status.state === "available" ? status.date : null;
  const published =
    date !== null && !Number.isNaN(Date.parse(date))
      ? formatDateTime(locale, Date.parse(date), { dateStyle: "medium" })
      : undefined;
  const line = updateStatusLine(update, current, t, locale);
  const eta =
    status.state === "downloading" ? etaText(status.received, status.total, rate, t) : undefined;

  return (
    <Dialog open={open} title={title} width={600} onClose={onClose} actions={actions}>
      <div className="flex flex-col gap-4" data-testid="update-dialog" data-state={status.state}>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-fg-muted">
          <span className="mono" data-testid="update-current">
            {t("update.current", { version: current })}
          </span>
          {published !== undefined && (
            <span data-testid="update-published">{t("update.published", { date: published })}</span>
          )}
          {version !== undefined && update.method !== null && (
            <span data-testid="update-method">{t(`update.method.${update.method}`)}</span>
          )}
        </div>
        {status.state === "available" && (
          <section
            aria-label={t("update.notes")}
            className="max-h-[320px] overflow-auto rounded-10 bg-inset p-4 hairline">
            {notes !== null && notes.trim().length > 0 ? (
              <ReleaseNotes markdown={notes} version={status.version} />
            ) : (
              <p className="text-[13px] text-fg-muted">{t("update.noNotes")}</p>
            )}
          </section>
        )}
        {status.state === "downloading" && (
          <div className="flex flex-col gap-2" data-testid="update-progress">
            <Progress
              value={
                status.total !== null && status.total > 0
                  ? status.received / status.total
                  : undefined
              }
              indeterminate={status.total === null || status.total === 0}
              size={6}
              label={t("update.downloading", {
                progress: downloadProgress(status.received, status.total),
              })}
            />
            <div className="mono flex flex-wrap gap-x-3 text-[11px] text-fg-muted">
              <span>{downloadProgress(status.received, status.total)}</span>
              {status.total !== null && (
                <span>
                  {t("update.size", {
                    received: formatBytes(status.received),
                    total: formatBytes(status.total),
                  })}
                </span>
              )}
              {rate !== undefined && (
                <span data-testid="update-speed">
                  {t("update.speed", { speed: formatBytes(rate) })}
                </span>
              )}
              {eta !== undefined && <span data-testid="update-eta">{eta}</span>}
            </div>
          </div>
        )}
        {status.state === "ready" && (
          <LampText tone="ok" size="sm">
            {t("update.ready")}
          </LampText>
        )}
        {status.state === "failed" && (
          <p role="alert" className="text-[13px] text-danger">
            {t("update.failed", { error: errorText(t, status.code) })}
          </p>
        )}
        {status.state !== "available" &&
          status.state !== "downloading" &&
          status.state !== "ready" &&
          status.state !== "failed" && (
            <LampText tone={line.tone} size="sm">
              {line.text}
            </LampText>
          )}
        <p className="text-[12px] text-fg-subtle">{t("update.network")}</p>
      </div>
    </Dialog>
  );
}

/** The title bar's note that an update is waiting: the version, the download's progress, or the
 *  restart it needs. Nothing when there is nothing to update. */
export function UpdateBadge({ onOpen }: { onOpen: () => void }) {
  const t = useT();
  const { update } = useUiState();
  const status = update.status;
  let label: string;
  switch (status.state) {
    case "available":
      label = t("update.badge.available", { version: status.version });
      break;
    case "downloading":
      label = t("update.badge.downloading", {
        progress: downloadProgress(status.received, status.total),
      });
      break;
    case "ready":
      label = t("update.badge.ready");
      break;
    default:
      return null;
  }
  return (
    <button
      type="button"
      data-testid="update-badge"
      title={t("update.badgeTitle")}
      onClick={onOpen}
      className="inline-flex h-7 items-center gap-1.5 rounded-full bg-accent-soft px-2.5 text-[12px] whitespace-nowrap text-accent-text transition-colors hover:bg-accent hover:text-accent-fg">
      <span aria-hidden className="h-1.5 w-1.5 rounded-full bg-accent" />
      {label}
    </button>
  );
}
