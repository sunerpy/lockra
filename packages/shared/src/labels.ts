// Label helpers: one place that turns codes from the core into words, so a page never builds a
// sentence out of an enum by itself.
import { DEFAULT_LOCALE, type Locale, type TFunction, translate } from "./i18n";
import type {
  Algorithm,
  CandidateStatus,
  ErrorCode,
  Incompatible,
  Notice,
  OtpKind,
  Origin,
  RejectReason,
  ThemeId,
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
    case "update_available":
      return t("notice.updateAvailable", { version: notice.version });
  }
}

/**
 * A release's notes as plain lines: the Markdown release-please writes, without the link syntax,
 * the emphasis, the pull request and commit references, or the heading that repeats the version.
 * The interface renders the lines as text, never as HTML.
 */
export function releaseNotesLines(markdown: string): string[] {
  return markdown
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => !/^#+\s*\[?v?\d+\.\d+\.\d+/.test(line))
    .map((line) =>
      line
        .replace(/\s*\(\[[^\]]*\]\([^)]*\)\)/g, "")
        .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
        .replace(/\*\*([^*]+)\*\*/g, "$1")
        .replace(/^#+\s*/, "")
        .replace(/^[*-]\s+/, "• ")
        .trim(),
    )
    .filter((line, index, lines) => line !== "" || (index > 0 && lines[index - 1] !== ""))
    .filter((line, index, lines) => !(line === "" && (index === 0 || index === lines.length - 1)));
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
