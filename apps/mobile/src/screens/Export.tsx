// Send accounts to another authenticator: Google Authenticator's migration codes, one standard
// code per account for Microsoft Authenticator, or a plain otpauth list file saved where the user
// picks. Every way out asks for the master password again (as the desktop's Export page).
import {
  type ExportWay,
  entryLabel,
  errorText,
  exportBlocker,
  incompatibleText,
} from "@lockra/shared";
import {
  Button,
  Card,
  EntryAvatar,
  Icon,
  PasswordField,
  cx,
  useBackend,
  useSubmit,
  useT,
  useToaster,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { overPhoneScreen } from "../app/phone-screen";
import { Page } from "../components/Page";
import { SwitchRow } from "../components/Rows";

const WAYS: readonly ExportWay[] = ["google", "microsoft", "file"];

export function Export() {
  const t = useT();
  const nav = useNav();
  const { entries } = useUiState();
  const { backend } = useBackend();
  const toaster = useToaster();
  const [way, setWay] = useState<ExportWay>("google");
  // What the user unticked; everything else that can go is ticked.
  const [unticked, setUnticked] = useState<ReadonlySet<string>>(new Set());
  const [password, setPassword] = useState("");
  const [plainOk, setPlainOk] = useState(false);
  const submit = useSubmit();
  const chosen = entries.filter((e) => exportBlocker(e, way) === null && !unticked.has(e.id));
  const ready = chosen.length > 0 && password !== "" && (way !== "file" || plainOk);
  const toggle = (id: string) =>
    setUnticked((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const choose = (next: ExportWay) => {
    setWay(next);
    submit.setError(undefined);
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const ids = chosen.map((e) => e.id);
    if (way === "file") {
      const name = await submit.run(() =>
        overPhoneScreen(() => backend.exportOtpauthFile(ids, password)),
      );
      setPassword("");
      if (typeof name === "string") toaster.info(t("export.file.saved", { name }));
      return;
    }
    const started = await submit.run(() =>
      backend.dispatch({ command: "export_start", target: way, entry_ids: ids, password }),
    );
    setPassword("");
    if (started !== undefined) nav.open({ name: "exportView", started });
  };
  return (
    <Page title={t("export.title")} testId="page-export">
      <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
        <p className="text-[14px] text-fg-muted">{t("export.subtitle")}</p>
        <Card padding="none" className="flex flex-col p-1">
          <div role="radiogroup" aria-label={t("export.target")}>
            {WAYS.map((id) => {
              const checked = way === id;
              return (
                <button
                  key={id}
                  type="button"
                  role="radio"
                  aria-checked={checked}
                  onClick={() => choose(id)}
                  className="flex w-full items-start gap-3 rounded-10 px-3 py-3 text-left">
                  <Icon
                    name={id === "file" ? "fileText" : "qr"}
                    size={20}
                    className="mt-0.5 text-fg-muted"
                  />
                  <span className="flex min-w-0 flex-1 flex-col gap-1">
                    <span className="text-[15px] text-fg">{t(`export.${id}.title`)}</span>
                    <span className="text-[13px] text-fg-muted">{t(`export.${id}.body`)}</span>
                  </span>
                  {checked && <Icon name="check" size={18} className="mt-0.5 text-accent-text" />}
                </button>
              );
            })}
          </div>
        </Card>
        <div className="flex items-center justify-between gap-2 px-1">
          <span className="text-[13px] font-medium text-fg-muted">{t("export.select")}</span>
          <span className="mono text-[12px] text-fg-subtle" data-testid="export-count">
            {t("export.selected", { n: chosen.length })}
          </span>
        </div>
        <Card padding="none" className="flex flex-col p-1">
          <ul aria-label={t("export.select")} data-testid="export-entries">
            {entries.map((entry) => {
              const blocker = exportBlocker(entry, way);
              const on = blocker === null && !unticked.has(entry.id);
              return (
                <li key={entry.id}>
                  <label
                    className={cx(
                      "flex min-h-14 items-center gap-3 rounded-10 px-3 py-2",
                      blocker !== null && "opacity-60",
                    )}>
                    <input
                      type="checkbox"
                      className="size-5 shrink-0 accent-accent"
                      checked={on}
                      disabled={blocker !== null}
                      onChange={() => toggle(entry.id)}
                      aria-label={entryLabel(entry.issuer, entry.account)}
                    />
                    <EntryAvatar
                      issuer={entry.issuer}
                      account={entry.account}
                      color={entry.color}
                      mark={entry.mark}
                      size={32}
                    />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-[15px] text-fg">
                        {entry.issuer || entry.account}
                      </span>
                      <span className="truncate text-[13px] text-fg-muted">
                        {blocker !== null
                          ? t("export.unavailable", { reason: incompatibleText(t, blocker) })
                          : entry.issuer !== ""
                            ? entry.account
                            : ""}
                      </span>
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        </Card>
        {way === "file" && (
          <Card padding="none">
            <SwitchRow
              label={t("export.file.confirm")}
              checked={plainOk}
              onChange={setPlainOk}
              testId="export-plain-ok"
            />
          </Card>
        )}
        <PasswordField
          size="lg"
          label={t("unlock.password")}
          value={password}
          onChange={setPassword}
          autoComplete="current-password"
          error={submit.error === undefined ? undefined : errorText(t, submit.error)}
        />
        <Button
          variant="primary"
          size="lg"
          type="submit"
          icon={way === "file" ? "download" : "qr"}
          loading={submit.busy}
          disabled={!ready}
          data-testid="export-start">
          {way === "file" ? t("export.file.save") : t("export.start")}
        </Button>
      </form>
    </Page>
  );
}
