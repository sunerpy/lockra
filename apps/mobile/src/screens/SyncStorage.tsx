// New storage settings for the space: another access key, password or address. The space must be
// at the new place already; the secret is typed in again (the core never sends it back). The
// master password confirms the change or, left empty, the fingerprint that unlocks this vault.
import {
  type SyncSpaceView,
  errorText,
  storageComplete,
  storageConfig,
  storageFormFrom,
} from "@lockra/shared";
import { Button, PasswordField, StorageFields, useBackend, useSubmit, useT } from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { shown, useFingerprint } from "../app/presence";
import { Page } from "../components/Page";

export function SyncStorage({ space }: { space: SyncSpaceView }) {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const [form, setForm] = useState(() => storageFormFrom(space.storage));
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const fingerprint = useFingerprint();
  const ready = storageComplete(form) && (password !== "" || fingerprint !== null);
  const failure = shown(submit.error);
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const storage = storageConfig(form);
    const done = await submit.run(() =>
      backend.dispatch(
        password === ""
          ? { command: "sync_set_storage", storage, reason: t("sync.storageReason") }
          : { command: "sync_set_storage", storage, password },
      ),
    );
    setPassword("");
    if (done !== undefined) nav.back();
  };
  return (
    <Page title={t("sync.storageEdit")} testId="page-sync-storage">
      <form
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-4"
        data-testid="sync-storage-form">
        <p className="text-[14px] text-fg-muted">{t("sync.storageEditBody")}</p>
        <StorageFields
          failure={submit.error}
          size="lg"
          form={form}
          onChange={(patch) => setForm((current) => ({ ...current, ...patch }))}
        />
        <PasswordField
          size="lg"
          label={t("sync.masterPassword")}
          value={password}
          onChange={setPassword}
          autoComplete="current-password"
          help={fingerprint === null ? undefined : t("mobile.passwordOrFingerprint")}
        />
        {failure !== undefined && (
          <p role="alert" className="text-[13px] text-danger">
            {errorText(t, failure)}
          </p>
        )}
        <Button variant="primary" size="lg" type="submit" loading={submit.busy} disabled={!ready}>
          {t("common.save")}
        </Button>
      </form>
    </Page>
  );
}
