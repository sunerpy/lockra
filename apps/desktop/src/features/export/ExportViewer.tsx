// The export codes, one page at a time, beside the current code of every account on the page (so
// the phone's codes can be checked against them). The view closes itself when it has been idle for
// as long as the core keeps the session, and when the core expires the session first.
import { type ExportPage, type ExportStarted, entryLabel, incompatibleText } from "@lockra/shared";
import {
  Button,
  Dialog,
  OtpCode,
  QrView,
  useBackend,
  useClock,
  useCodes,
  useT,
  useUiState,
} from "@lockra/ui";
import { useCallback, useEffect, useState } from "react";
import { useToaster } from "../../app/notices";
import { useShell } from "../../app/shell-state";

/** lockra-core `EXPORT_IDLE`: a page stays up this long after it was shown. */
export const EXPORT_SECONDS = 120;

export function ExportViewer({ started }: { started: ExportStarted }) {
  const t = useT();
  const { backend } = useBackend();
  const { entries } = useUiState();
  const shell = useShell();
  const toaster = useToaster();
  const codes = useCodes();
  const now = useClock();
  const [index, setIndex] = useState(0);
  const [page, setPage] = useState<{ page: ExportPage; at: number } | undefined>(undefined);
  const { session } = started;
  const { close: closeOverlay } = shell;

  // Closing is explicit (never an effect cleanup: StrictMode runs those on mount and the session
  // would be gone before its first page).
  const close = useCallback(() => {
    void backend.dispatch({ command: "export_close", session }).catch(() => undefined);
    closeOverlay();
  }, [backend, session, closeOverlay]);

  useEffect(() => {
    let live = true;
    backend
      .dispatch({ command: "export_page", session, index })
      .then((answer) => {
        if (live) setPage({ page: answer, at: Date.now() });
      })
      .catch((error: unknown) => {
        if (!live) return;
        toaster.error(error);
        close();
      });
    return () => {
      live = false;
    };
  }, [backend, session, index, toaster, close]);

  useEffect(
    () =>
      backend.on((event) => {
        if (
          event.type === "notice" &&
          event.notice.type === "export_expired" &&
          event.notice.session === session
        )
          close();
      }),
    [backend, session, close],
  );

  // The shared clock ticks on whole seconds, so it can read just before the page arrived.
  const left =
    page === undefined
      ? EXPORT_SECONDS
      : Math.max(0, EXPORT_SECONDS - Math.max(0, Math.floor((now - page.at) / 1000)));
  useEffect(() => {
    if (left === 0) close();
  }, [left, close]);

  const total = started.pages;
  const byId = new Map(entries.map((e) => [e.id, e]));
  const excluded = started.excluded.map((x) => ({ entry: byId.get(x.entry_id), reason: x.reason }));
  return (
    <Dialog
      open
      title={t(`export.${started.target}.title`)}
      onClose={close}
      width={760}
      hint={<span data-testid="export-countdown">{t("export.hideIn", { s: left })}</span>}
      actions={
        <>
          <Button
            disabled={index === 0}
            icon="chevronLeft"
            onClick={() => setIndex((i) => Math.max(0, i - 1))}>
            {t("export.previous")}
          </Button>
          {index + 1 < total ? (
            <Button
              variant="primary"
              onClick={() => setIndex((i) => Math.min(total - 1, i + 1))}
              data-testid="export-next">
              {t("export.next")}
            </Button>
          ) : (
            <Button variant="primary" onClick={close} data-testid="export-finish">
              {t("export.finish")}
            </Button>
          )}
        </>
      }>
      <div className="grid grid-cols-[auto_minmax(0,1fr)] gap-6" data-testid="export-viewer">
        {page === undefined ? (
          <div className="size-[324px] rounded-14 bg-inset" aria-busy />
        ) : (
          <QrView
            svg={page.page.svg}
            size={300}
            footer={t("export.page", { index: index + 1, total })}
          />
        )}
        <div className="flex min-w-0 flex-col gap-3">
          <p className="text-[12px] leading-[18px] text-fg-muted">
            {t(`export.${started.target}.body`)}
          </p>
          <p className="text-[12px] text-fg-muted">{t("export.verify")}</p>
          <ul className="flex flex-col gap-1.5" data-testid="export-verify">
            {(page?.page.entry_ids ?? []).map((id) => {
              const entry = byId.get(id);
              const code = codes.get(id);
              return (
                <li
                  key={id}
                  className="flex items-center justify-between gap-3 rounded-6 bg-inset px-3 py-1.5">
                  <span className="truncate text-[13px] text-fg">
                    {entry ? entryLabel(entry.issuer, entry.account) : id}
                  </span>
                  {code === undefined ? (
                    <span className="mono text-[13px] text-fg-subtle">— — —</span>
                  ) : (
                    <OtpCode code={code.code} size="sm" />
                  )}
                </li>
              );
            })}
          </ul>
          {excluded.length > 0 && (
            <details className="text-[12px] text-fg-muted" data-testid="export-excluded">
              <summary>{t("export.excluded", { n: excluded.length })}</summary>
              <ul className="mt-1.5 flex flex-col gap-1">
                {excluded.map(({ entry, reason }, i) => (
                  <li key={entry?.id ?? i} className="flex justify-between gap-3">
                    <span className="truncate">
                      {entry ? entryLabel(entry.issuer, entry.account) : "—"}
                    </span>
                    <span className="shrink-0">{incompatibleText(t, reason)}</span>
                  </li>
                ))}
              </ul>
            </details>
          )}
        </div>
      </div>
    </Dialog>
  );
}
