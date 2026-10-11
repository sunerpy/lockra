// Set up sync from this phone: a new space on a Lockra relay or storage of the user's own (an
// S3-compatible bucket or a WebDAV folder). Made, the page goes back to Settings › Sync, which
// reminds of the space's recovery key until it is saved.
import { emptyStorageForm, errorText, storageComplete, storageConfig } from "@lockra/shared";
import {
  Button,
  Input,
  PasswordField,
  StorageFields,
  useBackend,
  useSubmit,
  useT,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { Page } from "../components/Page";

export function SyncSetup() {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const [storage, setStorage] = useState(emptyStorageForm);
  const [deviceName, setDeviceName] = useState(() => t("sync.platformDevice.android"));
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const ready = storageComplete(storage) && password !== "";
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const made = await submit.run(() =>
      backend.dispatch({
        command: "sync_create",
        storage: storageConfig(storage),
        password,
        device_name: deviceName,
      }),
    );
    setPassword("");
    if (made !== undefined) nav.back();
  };
  return (
    <Page title={t("sync.off.createTitle")} testId="page-sync-setup">
      <form
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-4"
        data-testid="sync-create">
        <p className="text-[14px] text-fg-muted">{t("sync.off.createBody")}</p>
        <StorageFields
          failure={submit.error}
          size="lg"
          form={storage}
          onChange={(patch) => setStorage((form) => ({ ...form, ...patch }))}
        />
        <Input
          size="lg"
          label={t("sync.deviceName")}
          value={deviceName}
          onChange={(e) => setDeviceName(e.target.value)}
          help={t("sync.deviceNameHint")}
          maxLength={64}
        />
        <PasswordField
          size="lg"
          label={t("sync.masterPassword")}
          value={password}
          onChange={setPassword}
          autoComplete="current-password"
        />
        {submit.error !== undefined && (
          <p role="alert" className="text-[13px] text-danger">
            {errorText(t, submit.error)}
          </p>
        )}
        <Button
          variant="primary"
          size="lg"
          type="submit"
          icon="cloud"
          loading={submit.busy}
          disabled={!ready}>
          {t("sync.off.createSubmit")}
        </Button>
      </form>
    </Page>
  );
}
