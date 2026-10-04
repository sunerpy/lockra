// Label helpers: one place that turns codes from the core into words, so a page never builds a
// sentence out of an enum by itself.
import { DEFAULT_LOCALE, type Locale, type TFunction, formatDateTime, translate } from "./i18n";
import type {
  Algorithm,
  CandidateStatus,
  CandidateView,
  ErrorCode,
  Incompatible,
  Notice,
  OtpKind,
  Origin,
  RejectReason,
  SyncStatus,
  ThemeId,
  UpdateStatus,
  UpdateView,
} from "./schema";

export function themeName(theme: ThemeId, locale: Locale = DEFAULT_LOCALE): string {
  return translate(locale, `theme.name.${theme}`);
}

export function themeSubtitle(theme: ThemeId, locale: Locale = DEFAULT_LOCALE): string {
  return translate(locale, `theme.subtitle.${theme}`);
}

export function errorText(t: TFunction, code: ErrorCode): string {
  return t(`error.${code}`);
}

export function rejectText(t: TFunction, reason: RejectReason): string {
  return t(`reject.${reason}`);
}

export function incompatibleText(t: TFunction, reason: Incompatible): string {
  return t(`incompatible.${reason}`);
}

export function originText(t: TFunction, origin: Origin): string {
  return t(`origin.${origin}`);
}

export function statusText(t: TFunction, status: CandidateStatus): string {
  return status.type === "unsupported"
    ? `${t("import.status.unsupported")} · ${rejectText(t, status.reason)}`
    : t(`import.status.${status.type}`);
}

/** Where a found account came from: the file, the clipboard, the pasted text or the camera, and
 *  its line. */
export function candidateSource(t: TFunction, candidate: CandidateView): string {
  const source =
    candidate.source.type === "file"
      ? candidate.source.name
      : candidate.source.type === "clipboard"
        ? t("import.preview.sourceClipboard")
        : candidate.source.type === "camera"
          ? t("import.preview.sourceCamera")
          : t("import.preview.sourceText");
  return candidate.line === null
    ? source
    : `${source} · ${t("import.preview.line", { n: candidate.line })}`;
}

/** `SHA1 · 6 位 · 30 秒` style parameters of an account. */
export function parametersText(
  t: TFunction,
  kind: OtpKind,
  algorithm: Algorithm,
  digits: number,
): string {
  const timing =
    kind.type === "totp"
      ? t("codes.kind.totp", { period: kind.period })
      : t("codes.kind.hotp", { counter: kind.counter });
  return `${algorithm.toUpperCase()} · ${digits} · ${timing}`;
}

/** What a notice says, with the error inside a backup failure translated too. */
export function noticeText(t: TFunction, notice: Notice): string {
  switch (notice.type) {
    case "copied":
      return notice.clear_after_s === null
        ? t("notice.copiedKept")
        : t("notice.copied", { s: notice.clear_after_s });
    case "clipboard_cleared":
      return t("notice.clipboardCleared");
    case "imported":
      return t("notice.imported", {
        added: notice.added,
        replaced: notice.replaced,
        skipped: notice.skipped,
      });
    case "file_unrecognized":
      return t("notice.fileUnrecognized", { name: notice.name });
    case "file_unreadable":
      return t("notice.fileUnreadable", { name: notice.name });
    case "backup_written":
      return notice.automatic
        ? t("notice.backupWrittenAuto", { file: notice.file_name })
        : t("notice.backupWritten", { file: notice.file_name });
    case "backup_failed":
      return t("notice.backupFailed", { error: errorText(t, notice.code) });
    case "restored":
      return t("notice.restored", { n: notice.entries });
    case "auto_locked":
      return t("notice.autoLocked");
    case "export_expired":
      return t("notice.exportExpired");
    case "device_unlock_turned_off":
      return t("notice.deviceUnlockTurnedOff");
    case "sync_brought":
      return syncBroughtText(t, notice);
  }
}

/** `已从 Pixel 8 同步：新增 2 个，修改 1 个`: what a sync run brought, the counts that are not zero. */
function syncBroughtText(t: TFunction, notice: Extract<Notice, { type: "sync_brought" }>): string {
  const counts = [
    ["notice.syncAdded", notice.added],
    ["notice.syncUpdated", notice.updated],
    ["notice.syncRemoved", notice.removed],
  ] as const;
  const changes = counts
    .filter(([, n]) => n > 0)
    .map(([key, n]) => t(key, { n }))
    .join(t("notice.syncChangesSeparator"));
  return notice.devices.length === 0
    ? t("notice.syncBroughtFailed", { changes })
    : t("notice.syncBrought", {
        devices: notice.devices.join(t("notice.syncDevicesSeparator")),
        changes,
      });
}

/** `512 KB`, `4.0 MB`: a download's size for people. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(0, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Whether a notice is bad news (the toast is drawn in the danger tone). */
export function noticeIsProblem(notice: Notice): boolean {
  return (
    notice.type === "backup_failed" ||
    notice.type === "file_unreadable" ||
    notice.type === "file_unrecognized" ||
    notice.type === "device_unlock_turned_off"
  );
}

/** `3 分钟前` / `3 minutes ago`; the label for a moment in the past. */
export function relativeTime(t: TFunction, thenMs: number, nowMs: number): string {
  const seconds = Math.max(0, Math.floor((nowMs - thenMs) / 1000));
  if (seconds < 60) return t("common.justNow");
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t("common.minutesAgo", { n: minutes });
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return t("common.hoursAgo", { n: hours });
  return t("common.daysAgo", { n: Math.floor(hours / 24) });
}

/** What a sync space's last run says, and the tone of the lamp beside it. */
export function syncStatusLine(
  status: SyncStatus,
  t: TFunction,
  nowMs: number,
): { tone: "idle" | "accent" | "ok" | "danger"; text: string } {
  switch (status.state) {
    case "idle":
      return { tone: "idle", text: t("sync.status.idle") };
    case "syncing":
      return { tone: "accent", text: t("sync.status.syncing") };
    case "synced":
      return {
        tone: "ok",
        text: t("sync.status.synced", { when: relativeTime(t, status.at_ms, nowMs) }),
      };
    case "failed":
      return {
        tone: "danger",
        text: t("sync.status.failed", { error: errorText(t, status.code) }),
      };
  }
}

/** `4194304 / 11508084` → `36%`; without a total, the bytes so far. */
export function downloadProgress(received: number, total: number | null): string {
  if (total !== null && total > 0) return `${Math.min(100, Math.floor((received / total) * 100))}%`;
  return formatBytes(received);
}

/** The version the update status is about, when it names one. */
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

/** One line for the updater's state, and the tone of the lamp beside it: the desktop's Settings ›
 *  General and › About, the phone's About (after Voltip's `updateStatusLine`). */
export function updateStatusLine(
  update: UpdateView,
  current: string,
  t: TFunction,
  locale: Locale,
): { text: string; tone: "idle" | "accent" | "ok" | "danger" } {
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

/** `issuer: account`, or whichever exists. */
export function entryLabel(issuer: string, account: string): string {
  if (issuer && account) return `${issuer}: ${account}`;
  return issuer || account;
}

/** A code split for reading: `123 456`, `1234 5678`, `123 4567`. */
export function groupCode(code: string): string {
  if (code.length === 6) return `${code.slice(0, 3)} ${code.slice(3)}`;
  if (code.length === 8) return `${code.slice(0, 4)} ${code.slice(4)}`;
  if (code.length === 7) return `${code.slice(0, 3)} ${code.slice(3)}`;
  return code;
}
