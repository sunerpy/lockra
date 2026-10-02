// Send accounts to a phone: Google Authenticator's migration codes, one standard code per account
// for Microsoft Authenticator, or a plain otpauth list file. Every way out asks for the master
// password again; the codes themselves open in the export viewer (an overlay of the shell).
import {
  type EntryView,
  type ExportTarget,
  entryLabel,
  errorText,
  incompatibleText,
  parametersText,
} from "@lockra/shared";
import {
  Button,
  CardGrid,
  EntryAvatar,
  OptionCard,
  Panel,
  PasswordField,
  Toggle,
  cx,
  useBackend,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useSubmit } from "../app/dispatch";
import { useShell } from "../app/shell-state";

export type ExportWay = ExportTarget | "file";

/** Why `entry` cannot go to `way`, or `null` when it can (a file takes every account). */
export function exportBlocker(entry: EntryView, way: ExportWay): EntryView["export"]["google"] {
  return way === "file" ? null : entry.export[way];
}

export function Export() {
  const t = useT();
  const { entries } = useUiState();
  const { backend } = useBackend();
  const shell = useShell();
  const [way, setWay] = useState<ExportWay>("google");
  // What the user unticked; everything else that can go is ticked (a new account too).
  const [unticked, setUnticked] = useState<ReadonlySet<string>>(new Set());
  const [password, setPassword] = useState("");
  const [plainOk, setPlainOk] = useState(false);
  const [saved, setSaved] = useState<string | undefined>(undefined);
  const submit = useSubmit();
  const eligible = entries.filter((e) => exportBlocker(e, way) === null);
  const chosen = eligible.filter((e) => !unticked.has(e.id));
  const ready = chosen.length > 0 && password !== "" && (way !== "file" || plainOk);

  const toggle = (id: string, on: boolean) =>
    setUnticked((current) => {
      const next = new Set(current);
      if (on) next.delete(id);
      else next.add(id);
      return next;
    });
  const choose = (next: ExportWay) => {
    setWay(next);
    setSaved(undefined);
    submit.setError(undefined);
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const ids = chosen.map((e) => e.id);
    if (way === "file") {
      const name = await submit.run(() => backend.exportOtpauthFile(ids, password));
      setPassword("");
      if (name !== undefined && name !== null) setSaved(name);
      return;
    }
    const started = await submit.run(() =>
      backend.dispatch({ command: "export_start", target: way, entry_ids: ids, password }),
    );
    setPassword("");
    if (started !== undefined) shell.open({ type: "export", started });
  };

  const ways: { id: ExportWay; icon: "qr" | "fileText"; title: string; body: string }[] = [
    { id: "google", icon: "qr", title: t("export.google.title"), body: t("export.google.body") },
    {
      id: "microsoft",
      icon: "qr",
      title: t("export.microsoft.title"),
      body: t("export.microsoft.body"),
    },
    { id: "file", icon: "fileText", title: t("export.file.title"), body: t("export.file.body") },
  ];
  return (
    <div className="flex flex-col gap-4" data-testid="page-export">
      <p className="text-[13px] text-fg-muted">{t("export.subtitle")}</p>
      <CardGrid min={240} role="listbox" aria-label={t("export.target")}>
        {ways.map((w) => (
          <OptionCard
            key={w.id}
            icon={w.icon}
            title={w.title}
            selected={way === w.id}
            onSelect={() => choose(w.id)}
            aria-label={w.title}>
            <p className="text-[12px] leading-[18px] text-fg-muted">{w.body}</p>
          </OptionCard>
        ))}
      </CardGrid>
      <Panel
        eyebrow={t("export.select")}
        right={
          <>
            <span className="mono text-fg-subtle" data-testid="export-count">
              {t("export.selected", { n: chosen.length })}
            </span>
            <Button variant="text" size="sm" onClick={() => setUnticked(new Set())}>
              {t("export.selectAll")}
            </Button>
            <Button
              variant="text-muted"
              size="sm"
              onClick={() => setUnticked(new Set(entries.map((e) => e.id)))}>
              {t("export.selectNone")}
            </Button>
          </>
        }>
        {entries.length === 0 ? (
          <p className="text-[13px] text-fg-muted">{t("codes.emptyTitle")}</p>
        ) : (
          <ul className="flex max-h-[360px] flex-col overflow-y-auto" data-testid="export-entries">
            {entries.map((entry) => {
              const blocker = exportBlocker(entry, way);
              const on = blocker === null && !unticked.has(entry.id);
              return (
                <li key={entry.id}>
                  <label
                    className={cx(
                      "flex items-center gap-3 rounded-6 px-2 py-1.5",
                      blocker === null
                        ? "cursor-pointer hover:bg-inset"
                        : "cursor-not-allowed opacity-60",
                    )}>
                    <input
                      type="checkbox"
                      className="size-4 shrink-0 accent-accent"
                      checked={on}
                      disabled={blocker !== null}
                      onChange={(e) => toggle(entry.id, e.target.checked)}
                      aria-label={entryLabel(entry.issuer, entry.account)}
                    />
                    <EntryAvatar
                      issuer={entry.issuer}
                      account={entry.account}
                      color={entry.color}
                      mark={entry.mark}
                      size={24}
                    />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-[13px] text-fg">
                        {entry.issuer || entry.account}
                      </span>
                      {entry.issuer !== "" && (
                        <span className="truncate text-[11px] text-fg-muted">{entry.account}</span>
                      )}
                    </span>
                    {blocker === null ? (
                      <span className="mono shrink-0 text-[11px] text-fg-subtle">
                        {parametersText(t, entry.kind, entry.algorithm, entry.digits)}
                      </span>
                    ) : (
                      <span className="shrink-0 text-[12px] text-fg-muted">
                        {t("export.unavailable", { reason: incompatibleText(t, blocker) })}
                      </span>
                    )}
                  </label>
                </li>
              );
            })}
          </ul>
        )}
      </Panel>
      <Panel eyebrow={way === "file" ? t("export.file.title") : t("export.passwordPrompt")}>
        <form onSubmit={(e) => void onSubmit(e)} className="flex flex-wrap items-end gap-3">
          <PasswordField
            label={t("unlock.password")}
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            className="min-w-[16rem] flex-1"
            error={submit.error === undefined ? undefined : errorText(t, submit.error)}
          />
          {way === "file" && (
            <Toggle
              checked={plainOk}
              onChange={setPlainOk}
              label={t("export.file.confirm")}
              className="h-9"
            />
          )}
          <Button
            variant="primary"
            type="submit"
            icon={way === "file" ? "download" : "qr"}
            loading={submit.busy}
            disabled={!ready}
            data-testid="export-start">
            {way === "file" ? t("export.file.save") : t("export.start")}
          </Button>
        </form>
        {saved !== undefined && (
          <p
            role="status"
            className="mono mt-3 text-[12px] text-fg-muted"
            data-testid="export-saved">
            {t("export.file.saved", { name: saved })}
          </p>
        )}
      </Panel>
    </div>
  );
}
