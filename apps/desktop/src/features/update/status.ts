import {
  type Locale,
  type TFunction,
  type UpdateStatus,
  type UpdateView,
  errorText,
  formatBytes,
  formatDateTime,
} from "@lockra/shared";
import type { LampTone } from "@lockra/ui";

/** `4194304 / 11508084` → `36%`; without a total, the bytes so far. */
export function downloadProgress(received: number, total: number | null): string {
  if (total !== null && total > 0) return `${Math.min(100, Math.floor((received / total) * 100))}%`;
  return formatBytes(received);
}

/** The version the status is about, when it names one. */
export function statusVersion(status: UpdateStatus): string | undefined {
  switch (status.state) {
    case "available":
    case "downloading":
    case "ready":
    case "installing":
      return status.version;
    default:
      return undefined;
  }
}

/** One line for the updater's state, shared by Settings › General and › About (after Voltip's
 *  `updateStatusLine`). */
export function updateStatusLine(
  update: UpdateView,
  current: string,
  t: TFunction,
  locale: Locale,
): { text: string; tone: LampTone } {
  if (update.method === null) return { text: t("update.status.unavailable"), tone: "idle" };
  const status = update.status;
  switch (status.state) {
    case "idle":
      return { text: t("update.status.idle"), tone: "idle" };
    case "checking":
      return { text: t("update.status.checking"), tone: "accent" };
    case "up_to_date":
      return {
        text: t("update.status.upToDate", {
          version: current,
          at: formatDateTime(locale, status.checked_at_ms, {
            dateStyle: "medium",
            timeStyle: "short",
          }),
        }),
        tone: "ok",
      };
    case "available":
      return {
        text: t("update.status.available", { version: status.version, current }),
        tone: "accent",
      };
    case "downloading":
      return {
        text: t("update.status.downloading", {
          version: status.version,
          progress: downloadProgress(status.received, status.total),
        }),
        tone: "accent",
      };
    case "ready":
      return { text: t("update.status.ready", { version: status.version }), tone: "ok" };
    case "installing":
      return { text: t("update.status.installing", { version: status.version }), tone: "accent" };
    case "failed":
      return {
        text: t("update.status.failed", { error: errorText(t, status.code) }),
        tone: "danger",
      };
  }
}
