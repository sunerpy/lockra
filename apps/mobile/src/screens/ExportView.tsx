// The export's codes, one page at a time, with the current code of every account on the page so
// the other phone's codes can be checked against them (the desktop's export viewer). The page
// closes itself when it has been up for as long as the core keeps the session, and when the core
// expires the session first; leaving it any way closes the session (App.tsx).
import {
  EXPORT_SECONDS,
  type ExportPage,
  type ExportStarted,
  entryLabel,
  incompatibleText,
} from "@lockra/shared";
import {
  Button,
  OtpCode,
  QrView,
  useBackend,
  useClock,
  useCodes,
  useT,
  useToaster,
  useUiState,
} from "@lockra/ui";
import { useEffect, useState } from "react";
import { useNav } from "../app/nav";
import { Page } from "../components/Page";

export function ExportView({ started }: { started: ExportStarted }) {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const { entries } = useUiState();
  const toaster = useToaster();
  const codes = useCodes();
  const now = useClock();
  const [index, setIndex] = useState(0);
  const [page, setPage] = useState<{ page: ExportPage; at: number } | undefined>(undefined);
  const { session } = started;
  const { back } = nav;

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
        back();
      });
    return () => {
      live = false;
    };
  }, [backend, session, index, toaster, back]);

  useEffect(
    () =>
      backend.on((event) => {
        if (
          event.type === "notice" &&
          event.notice.type === "export_expired" &&
          event.notice.session === session
        )
          back();
      }),
    [backend, session, back],
  );

  // The shared clock ticks on whole seconds, so it can read just before the page arrived.
  const left =
    page === undefined
      ? EXPORT_SECONDS
      : Math.max(0, EXPORT_SECONDS - Math.max(0, Math.floor((now - page.at) / 1000)));
  useEffect(() => {
    if (left === 0) back();
  }, [left, back]);

  const total = started.pages;
  const byId = new Map(entries.map((e) => [e.id, e]));
  const excluded = started.excluded.map((x) => ({ entry: byId.get(x.entry_id), reason: x.reason }));
  return (
    <Page title={t(`export.${started.target}.title`)} testId="page-export-view">
      <div className="flex flex-col gap-4" data-testid="export-viewer">
        <p className="text-[13px] text-fg-muted">{t(`export.${started.target}.body`)}</p>
        <div className="self-center">
          {page === undefined ? (
            <div className="size-[260px] rounded-14 bg-inset" aria-busy />
          ) : (
            <QrView
              svg={page.page.svg}
              size={260}
              footer={t("export.page", { index: index + 1, total })}
            />
          )}
        </div>
        <p className="text-center text-[13px] text-fg-subtle" data-testid="export-countdown">
          {t("export.hideIn", { s: left })}
        </p>
        <p className="text-[13px] text-fg-muted">{t("export.verify")}</p>
        <ul className="flex flex-col gap-1.5" data-testid="export-verify">
          {(page?.page.entry_ids ?? []).map((id) => {
            const entry = byId.get(id);
            const code = codes.get(id);
            return (
              <li
                key={id}
                className="flex items-center justify-between gap-3 rounded-10 bg-inset px-3 py-2">
                <span className="truncate text-[14px] text-fg">
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
          <details className="text-[13px] text-fg-muted" data-testid="export-excluded">
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
        <div className="grid grid-cols-2 gap-2">
          <Button
            size="lg"
            icon="chevronLeft"
            disabled={index === 0}
            onClick={() => setIndex((i) => Math.max(0, i - 1))}>
            {t("export.previous")}
          </Button>
          {index + 1 < total ? (
            <Button
              variant="primary"
              size="lg"
              onClick={() => setIndex((i) => Math.min(total - 1, i + 1))}
              data-testid="export-next">
              {t("export.next")}
            </Button>
          ) : (
            <Button variant="primary" size="lg" onClick={back} data-testid="export-finish">
              {t("export.finish")}
            </Button>
          )}
        </div>
      </div>
    </Page>
  );
}
