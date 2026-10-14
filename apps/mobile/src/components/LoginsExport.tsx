// The Export screen's logins (password management on): the chosen ones, every one ticked at
// first, to a file other password managers import, saved where the user picks: a KeePass database
// under a password typed twice here, or Bitwarden's plain CSV once its risk is accepted. The master
// password is asked again, or left empty the fingerprint that unlocks this vault; the file is made
// in Rust and handed to the system's file picker, and the screen hears its name alone.
import {
  type ItemsExportFormat,
  entryLabel,
  errorText,
  itemAccount,
  itemName,
  passwordLongEnough,
  sortItems,
} from "@lockra/shared";
import {
  Button,
  Card,
  EntryAvatar,
  Icon,
  PasswordField,
  useBackend,
  useItems,
  useSubmit,
  useT,
  useToaster,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { overPhoneScreen } from "../app/phone-screen";
import { shown, useFingerprint } from "../app/presence";
import { SwitchRow } from "./Rows";

const FORMATS: readonly ItemsExportFormat[] = ["kdbx", "csv"];

export function LoginsExport() {
  const t = useT();
  const { backend } = useBackend();
  const toaster = useToaster();
  // A login of a later Lockra's kind is shown, not exported.
  const logins = sortItems(
    (useItems() ?? []).filter((item) => !item.read_only),
    "name",
  );
  const [format, setFormat] = useState<ItemsExportFormat>("kdbx");
  // What the user unticked; every other login is ticked.
  const [unticked, setUnticked] = useState<ReadonlySet<string>>(new Set());
  const [password, setPassword] = useState("");
  const [kdbxPassword, setKdbxPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [plainOk, setPlainOk] = useState(false);
  const submit = useSubmit();
  const fingerprint = useFingerprint();
  const chosen = logins.filter((item) => !unticked.has(item.id));
  const mismatch = repeat !== "" && repeat !== kdbxPassword;
  const ready =
    chosen.length > 0 &&
    (password !== "" || fingerprint !== null) &&
    (format === "kdbx" ? passwordLongEnough(kdbxPassword) && repeat === kdbxPassword : plainOk);
  const failure = shown(submit.error);
  const toggle = (id: string) =>
    setUnticked((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const choose = (next: ItemsExportFormat) => {
    setFormat(next);
    submit.setError(undefined);
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    // Without the password, the fingerprint.
    const typed = password === "" ? undefined : password;
    const name = await submit.run(() =>
      overPhoneScreen(() =>
        backend.exportItemsFile({
          ids: chosen.map((item) => item.id),
          format,
          password: typed,
          reason: typed === undefined ? t("export.logins.reason") : undefined,
          acknowledged: format === "csv" && plainOk,
          kdbxPassword: format === "kdbx" ? kdbxPassword : undefined,
        }),
      ),
    );
    setPassword("");
    if (typeof name === "string") {
      setKdbxPassword("");
      setRepeat("");
      toaster.info(t("export.file.saved", { name }));
    }
  };
  return (
    <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
      <p className="text-[14px] text-fg-muted">{t("export.logins.subtitle")}</p>
      <Card padding="none" className="flex flex-col p-1">
        <div role="radiogroup" aria-label={t("export.logins.target")}>
          {FORMATS.map((id) => {
            const checked = format === id;
            return (
              <button
                key={id}
                type="button"
                role="radio"
                aria-checked={checked}
                onClick={() => choose(id)}
                className="flex w-full items-start gap-3 rounded-10 px-3 py-3 text-left">
                <Icon
                  name={id === "kdbx" ? "lock" : "fileText"}
                  size={20}
                  className="mt-0.5 text-fg-muted"
                />
                <span className="flex min-w-0 flex-1 flex-col gap-1">
                  <span className="text-[15px] text-fg">{t(`export.logins.${id}.title`)}</span>
                  <span className="text-[13px] text-fg-muted">{t(`export.logins.${id}.body`)}</span>
                </span>
                {checked && <Icon name="check" size={18} className="mt-0.5 text-accent-text" />}
              </button>
            );
          })}
        </div>
      </Card>
      <div className="flex items-center justify-between gap-2 px-1">
        <span className="text-[13px] font-medium text-fg-muted">{t("export.logins.select")}</span>
        <span className="mono text-[12px] text-fg-subtle" data-testid="export-count">
          {t("export.selected", { n: chosen.length })}
        </span>
      </div>
      <Card padding="none" className="flex flex-col p-1">
        {logins.length === 0 ? (
          <p className="px-3 py-3 text-[14px] text-fg-muted" data-testid="export-logins">
            {t("export.logins.empty")}
          </p>
        ) : (
          <ul aria-label={t("export.logins.select")} data-testid="export-logins">
            {logins.map((item) => {
              const name = itemName(item);
              const account = itemAccount(item);
              const detail = [account, item.totp_entry === null ? "" : t("export.logins.withCode")]
                .filter((part) => part !== "")
                .join(" · ");
              return (
                <li key={item.id}>
                  <label className="flex min-h-14 items-center gap-3 rounded-10 px-3 py-2">
                    <input
                      type="checkbox"
                      className="size-5 shrink-0 accent-accent"
                      checked={!unticked.has(item.id)}
                      onChange={() => toggle(item.id)}
                      aria-label={entryLabel(name, account)}
                    />
                    <EntryAvatar
                      issuer={name}
                      account={account}
                      color={item.color}
                      mark={item.mark}
                      size={32}
                    />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-[15px] text-fg">{name}</span>
                      {detail !== "" && (
                        <span className="truncate text-[13px] text-fg-muted">{detail}</span>
                      )}
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        )}
      </Card>
      {format === "kdbx" ? (
        <>
          <PasswordField
            size="lg"
            label={t("export.logins.kdbx.password")}
            value={kdbxPassword}
            onChange={setKdbxPassword}
            strength
            autoComplete="new-password"
          />
          <PasswordField
            size="lg"
            label={t("export.logins.kdbx.repeat")}
            value={repeat}
            onChange={setRepeat}
            autoComplete="new-password"
            error={mismatch ? t("welcome.create.mismatch") : undefined}
          />
        </>
      ) : (
        <Card padding="none">
          <SwitchRow
            label={t("export.logins.csv.confirm")}
            checked={plainOk}
            onChange={setPlainOk}
            testId="export-items-plain-ok"
          />
        </Card>
      )}
      <PasswordField
        size="lg"
        label={t("unlock.password")}
        value={password}
        onChange={setPassword}
        autoComplete="current-password"
        help={fingerprint === null ? undefined : t("mobile.passwordOrFingerprint")}
        error={failure === undefined ? undefined : errorText(t, failure)}
      />
      <Button
        variant="primary"
        size="lg"
        type="submit"
        icon="download"
        loading={submit.busy}
        disabled={!ready}
        data-testid="export-items-save">
        {t(`export.logins.${format}.save`)}
      </Button>
    </form>
  );
}
