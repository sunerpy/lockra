// New storage settings for the space: another access key, password or address, or another storage
// the space moves to once the user confirms it (a place that holds nothing of it yet); the secret
// is typed in again (the core never sends it back). The master password confirms the change or,
// left empty, the fingerprint that unlocks this vault (asked again for a confirmed move).
import {
  type StorageConfig,
  type SyncSpaceView,
  errorText,
  formSummary,
  isLockraError,
  storageComplete,
  storageConfig,
  storageFormFrom,
} from "@lockra/shared";
import {
  Button,
  Dialog,
  PasswordField,
  StorageFields,
  useBackend,
  useSubmit,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { overPhoneScreen } from "../app/phone-screen";
import { shown, useFingerprint } from "../app/presence";
import { Page } from "../components/Page";

export function SyncStorage({ space }: { space: SyncSpaceView }) {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const { settings } = useUiState();
  const [form, setForm] = useState(() => storageFormFrom(space.storage));
  const [password, setPassword] = useState("");
  // Settings that point where the space is not yet, kept with the password while the move is asked.
  const [moving, setMoving] = useState<StorageConfig | null>(null);
  const submit = useSubmit();
  const fingerprint = useFingerprint();
  const ready = storageComplete(form) && (password !== "" || fingerprint !== null);
  const failure = shown(submit.error);
  const save = async (storage: StorageConfig, move: boolean) => {
    const presence = password === "" ? { reason: t("sync.storageReason") } : { password };
    let ask = false;
    const done = await submit.run(async () => {
      try {
        return await backend.dispatch({
          command: "sync_set_storage",
          storage,
          ...presence,
          ...(move ? { move_here: true } : {}),
        });
      } catch (error) {
        // A place that holds nothing of the space yet: the user is asked to move it there.
        ask = !move && isLockraError(error) && error.code === "sync_space_not_found";
        if (ask) return undefined;
        throw error;
      }
    });
    if (ask) {
      setMoving(storage);
      return;
    }
    setMoving(null);
    setPassword("");
    if (done !== undefined) nav.back();
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    await save(storageConfig(form), false);
  };
  // An invitation from a device of this space brings where the space is now (the core takes its
  // settings as a join of the space this phone is already in).
  const scan = async () => {
    const proof = password === "" ? { reason: t("sync.storageReason") } : { password };
    const taken = await submit.run(() =>
      overPhoneScreen(() =>
        backend.scanJoin(
          { prompt: t("mobile.sync.scanPrompt"), cancel: t("common.cancel") },
          { ...proof, deviceName: space.device_name },
        ),
      ),
    );
    // Left without a code, nothing was tried: the password stays.
    if (taken === false) return;
    setPassword("");
    if (taken === true) nav.back();
  };
  const cancelMove = () => {
    setMoving(null);
    setPassword("");
  };
  return (
    <Page title={t("sync.storageEdit")} testId="page-sync-storage">
      <form
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-4"
        data-testid="sync-storage-form">
        <p className="text-[14px] text-fg-muted">{t("sync.storageEditBody")}</p>
        <div className="flex flex-col gap-2">
          <p className="text-[14px] text-fg-muted">{t("mobile.sync.storageScanHint")}</p>
          <Button
            variant="outline"
            size="lg"
            icon="qr"
            disabled={password === "" && fingerprint === null}
            loading={submit.busy}
            onClick={() => void scan()}>
            {t("mobile.sync.storageScan")}
          </Button>
        </div>
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
      {moving !== null && (
        <Dialog
          open
          title={t("sync.move.title")}
          onClose={cancelMove}
          actions={
            <>
              <Button variant="ghost" size="lg" onClick={cancelMove}>
                {t("common.cancel")}
              </Button>
              <Button
                variant="primary"
                size="lg"
                loading={submit.busy}
                onClick={() => void save(moving, true)}>
                {t("sync.move.confirm")}
              </Button>
            </>
          }>
          <p>
            {t("sync.move.body", {
              place: `${t(`sync.storage.${moving.kind}`)} · ${formSummary(form)}`,
            })}
          </p>
          {settings.auto_backup.enabled && <p className="mt-2">{t("sync.move.backup")}</p>}
        </Dialog>
      )}
    </Page>
  );
}
