// Label helpers: one place that turns codes from the core into words, so a page never builds a
// sentence out of an enum by itself.
import { DEFAULT_LOCALE, type Locale, type TFunction, formatDateTime, translate } from "./i18n";
import type {
  Algorithm,
  CandidateStatus,
  CandidateView,
  ConflictReason,
  ErrorCode,
  Incompatible,
  Notice,
  OtpKind,
  Origin,
  RejectReason,
  SyncSpaceView,
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
  if (status.type === "unsupported")
    return `${t("import.status.unsupported")} · ${rejectText(t, status.reason)}`;
  if (status.type === "login_conflict")
    return `${t("import.status.login_conflict")} · ${conflictText(t, status.reason)}`;
  return t(`import.status.${status.type}`);
}

/** What differs between a login of the import and the vault's, and what replacing does. */
export function conflictText(t: TFunction, reason: ConflictReason): string {
  return t(`import.conflict.${reason}`);
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

/** What a notice says, with the error inside a backup failure translated too. A copy names the
 *  account when the caller knows it (`name`, the account's label). */
export function noticeText(t: TFunction, notice: Notice, name?: string): string {
  switch (notice.type) {
    case "copied":
      if (name !== undefined && name !== "")
        return notice.clear_after_s === null
          ? t("notice.copiedKeptName", { name })
          : t("notice.copiedName", { name, s: notice.clear_after_s });
      return notice.clear_after_s === null
        ? t("notice.copiedKept")
        : t("notice.copied", { s: notice.clear_after_s });
    case "item_copied": {
      const part = t(`notice.copiedPart.${notice.part}`);
      if (name !== undefined && name !== "")
        return notice.clear_after_s === null
          ? t("notice.itemCopiedKeptName", { name, part })
          : t("notice.itemCopiedName", { name, part, s: notice.clear_after_s });
      return notice.clear_after_s === null
        ? t("notice.itemCopiedKept", { part })
        : t("notice.itemCopied", { part, s: notice.clear_after_s });
    }
    case "clipboard_cleared":
      return t("notice.clipboardCleared");
    case "item_saved":
      return t(notice.updated ? "notice.itemUpdated" : "notice.itemSaved", { title: notice.title });
    case "browser_pair_requested":
      return t("notice.browserPairRequested", {
        browser: browserName(notice.browser),
        code: pairingCode(notice.code),
      });
    case "browser_confirm_requested":
      return t("notice.browserConfirmRequested", {
        browser: browserName(notice.browser),
        title: notice.title,
      });
    case "browser_paused":
      return t("notice.browserPaused", {
        name:
          notice.label === ""
            ? browserName(notice.browser)
            : `${browserName(notice.browser)} · ${notice.label}`,
      });
    case "browser_unlock_requested":
      return t("notice.browserUnlockRequested", { browser: browserName(notice.browser) });
    case "imported":
      return importedText(t, notice);
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

function anyCount(counts: { added: number; replaced: number; skipped: number }): boolean {
  return counts.added + counts.replaced + counts.skipped > 0;
}

/** What a commit did: the accounts' counts as before, the logins' alone when only logins were
 *  found, both when the import held both. */
function importedText(t: TFunction, notice: Extract<Notice, { type: "imported" }>): string {
  const codes = { added: notice.added, replaced: notice.replaced, skipped: notice.skipped };
  const logins = notice.logins;
  if (!anyCount(logins)) return t("notice.imported", codes);
  if (!anyCount(codes)) return t("notice.importedLogins", logins);
  return t("notice.importedBoth", {
    ...codes,
    loginsAdded: logins.added,
    loginsReplaced: logins.replaced,
    loginsSkipped: logins.skipped,
  });
}

/** `已从 Pixel 8 同步：新增 2 个，修改 1 个`: what a sync run brought, the counts that are not zero
 *  (the accounts', then the logins'). */
function syncBroughtText(t: TFunction, notice: Extract<Notice, { type: "sync_brought" }>): string {
  const counts = [
    ["notice.syncAdded", notice.added],
    ["notice.syncUpdated", notice.updated],
    ["notice.syncRemoved", notice.removed],
    ["notice.syncLogins", notice.logins],
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

/** The sync line of the sidebar's status card and of the codes page's vault card (board A1): off,
 *  running, failed, or the devices of the space once it has synced. */
export function syncSummary(t: TFunction, space: SyncSpaceView | null): string {
  if (space === null) return t("codes.hero.syncOff");
  switch (space.status.state) {
    case "syncing":
      return t("codes.hero.syncing");
    case "failed":
      return t("codes.hero.syncFailed");
    case "idle":
      if (space.last_sync_ms === null) return t("sync.status.idle");
  }
  return t("codes.hero.devices", { n: space.devices.length });
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
  // Google Play updates a copy it installed: there is nothing to check.
  if (update.method === "play")
    return { text: t("update.status.play", { version: current }), tone: "idle" };
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

/** A browser's name as an extension gave it (`chrome`, `edge`…), as people write it. */
export function browserName(browser: string): string {
  const known: Record<string, string> = {
    chrome: "Chrome",
    chromium: "Chromium",
    edge: "Edge",
    firefox: "Firefox",
    brave: "Brave",
  };
  return known[browser] ?? (browser === "" ? "?" : browser);
}

/** A pairing code in two groups of three digits, as both sides show it. */
export function pairingCode(code: string): string {
  return code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code;
}
