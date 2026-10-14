// The Export page's logins (password management on): the chosen ones, every one ticked at first,
// to a file other password managers import, a KeePass database under a password typed twice here,
// or Bitwarden's plain CSV once its risk is accepted. The master password is asked again; the
// file is made and written in Rust after it, and the page hears its name alone.
import {
  type ItemsExportFormat,
  entryLabel,
  errorText,
  itemAccount,
  itemHost,
  itemName,
  passwordLongEnough,
  sortItems,
} from "@lockra/shared";
import {
  Button,
  CardGrid,
  EntryAvatar,
  OptionCard,
  Panel,
  PasswordField,
  Toggle,
  useBackend,
  useItems,
  useT,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useSubmit } from "../../app/dispatch";

const FORMATS: { id: ItemsExportFormat; icon: "lock" | "fileText" }[] = [
  { id: "kdbx", icon: "lock" },
  { id: "csv", icon: "fileText" },
];

export function LoginsExport() {
  const t = useT();
  const { backend } = useBackend();
  // A login of a later Lockra's kind is shown, not exported.
  const logins = sortItems(
    (useItems() ?? []).filter((item) => !item.read_only),
    "name",
  );
  const [format, setFormat] = useState<ItemsExportFormat>("kdbx");
  // What the user unticked; every other login is ticked (a new one too).
  const [unticked, setUnticked] = useState<ReadonlySet<string>>(new Set());
  const [password, setPassword] = useState("");
  const [kdbxPassword, setKdbxPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [plainOk, setPlainOk] = useState(false);
  const [saved, setSaved] = useState<string | undefined>(undefined);
  const submit = useSubmit();
  const chosen = logins.filter((item) => !unticked.has(item.id));
  const mismatch = repeat !== "" && repeat !== kdbxPassword;
  const ready =
    chosen.length > 0 &&
    password !== "" &&
    (format === "kdbx" ? passwordLongEnough(kdbxPassword) && repeat === kdbxPassword : plainOk);

  const toggle = (id: string, on: boolean) =>
    setUnticked((current) => {
      const next = new Set(current);
      if (on) next.delete(id);
      else next.add(id);
      return next;
    });
  const choose = (next: ItemsExportFormat) => {
    setFormat(next);
    setSaved(undefined);
    submit.setError(undefined);
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const name = await submit.run(() =>
      backend.exportItemsFile({
        ids: chosen.map((item) => item.id),
        format,
        password,
        acknowledged: format === "csv" && plainOk,
        kdbxPassword: format === "kdbx" ? kdbxPassword : undefined,
      }),
    );
    setPassword("");
    if (name !== undefined && name !== null) {
      setSaved(name);
      setKdbxPassword("");
      setRepeat("");
    }
  };

  return (
    <>
      <CardGrid min={240} role="listbox" aria-label={t("export.logins.target")}>
        {FORMATS.map((f) => (
          <OptionCard
            key={f.id}
            icon={f.icon}
            title={t(`export.logins.${f.id}.title`)}
            selected={format === f.id}
            onSelect={() => choose(f.id)}
            aria-label={t(`export.logins.${f.id}.title`)}>
            <p className="text-[12px] leading-[18px] text-fg-muted">
              {t(`export.logins.${f.id}.body`)}
            </p>
          </OptionCard>
        ))}
      </CardGrid>
      <Panel
        eyebrow={t("export.logins.select")}
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
              onClick={() => setUnticked(new Set(logins.map((item) => item.id)))}>
              {t("export.selectNone")}
            </Button>
          </>
        }>
        {logins.length === 0 ? (
          <p className="text-[13px] text-fg-muted" data-testid="export-logins">
            {t("export.logins.empty")}
          </p>
        ) : (
          <ul className="flex max-h-[360px] flex-col overflow-y-auto" data-testid="export-logins">
            {logins.map((item) => {
              const name = itemName(item);
              const account = itemAccount(item);
              const host = itemHost(item);
              return (
                <li key={item.id}>
                  <label className="flex cursor-pointer items-center gap-3 rounded-6 px-2 py-1.5 hover:bg-inset">
                    <input
                      type="checkbox"
                      className="size-4 shrink-0 accent-accent"
                      checked={!unticked.has(item.id)}
                      onChange={(e) => toggle(item.id, e.target.checked)}
                      aria-label={entryLabel(name, account)}
                    />
                    <EntryAvatar
                      issuer={name}
                      account={account}
                      color={item.color}
                      mark={item.mark}
                      size={24}
                    />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-[13px] text-fg">{name}</span>
                      {account !== "" && (
                        <span className="truncate text-[11px] text-fg-muted">{account}</span>
                      )}
                    </span>
                    {item.totp_entry !== null && (
                      <span className="shrink-0 text-[12px] text-fg-muted">
                        {t("export.logins.withCode")}
                      </span>
                    )}
                    {host !== null && (
                      <span className="mono shrink-0 truncate text-[11px] text-fg-subtle">
                        {host}
                      </span>
                    )}
                  </label>
                </li>
              );
            })}
          </ul>
        )}
      </Panel>
      <Panel eyebrow={t(`export.logins.${format}.title`)}>
        <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
          {format === "kdbx" && (
            <div className="grid gap-3 sm:grid-cols-2">
              <PasswordField
                label={t("export.logins.kdbx.password")}
                value={kdbxPassword}
                onChange={setKdbxPassword}
                strength
                autoComplete="new-password"
              />
              <PasswordField
                label={t("export.logins.kdbx.repeat")}
                value={repeat}
                onChange={setRepeat}
                autoComplete="new-password"
                error={mismatch ? t("welcome.create.mismatch") : undefined}
              />
            </div>
          )}
          <div className="flex flex-wrap items-end gap-3">
            <PasswordField
              label={t("unlock.password")}
              value={password}
              onChange={setPassword}
              autoComplete="current-password"
              className="min-w-[16rem] flex-1"
              error={submit.error === undefined ? undefined : errorText(t, submit.error)}
            />
            {format === "csv" && (
              <Toggle
                checked={plainOk}
                onChange={setPlainOk}
                label={t("export.logins.csv.confirm")}
                className="h-9"
              />
            )}
            <Button
              variant="primary"
              type="submit"
              icon="download"
              loading={submit.busy}
              disabled={!ready}
              data-testid="export-items-save">
              {t(`export.logins.${format}.save`)}
            </Button>
          </div>
        </form>
        {saved !== undefined && (
          <p
            role="status"
            className="mono mt-3 text-[12px] text-fg-muted"
            data-testid="export-items-saved">
            {t("export.file.saved", { name: saved })}
          </p>
        )}
      </Panel>
    </>
  );
}
