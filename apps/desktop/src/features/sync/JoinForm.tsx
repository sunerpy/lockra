import {
  type JoinSource,
  emptyStorageForm,
  errorText,
  storageComplete,
  storageConfig,
} from "@lockra/shared";
import {
  Button,
  Input,
  PasswordField,
  Segmented,
  StorageFields,
  Textarea,
  useBackend,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useSubmit } from "../../app/dispatch";

type Mode = "invite" | "key";

/** Joining a space: another device's invitation, or the storage and the sync key typed in. On the
 *  welcome screen (`newVault`) the master password of a device in the space becomes this vault's;
 *  with a vault, its own master password is checked and opens the space unless the space's devices
 *  use another one, typed in apart. */
export function JoinForm({
  newVault = false,
  onCancel,
}: {
  newVault?: boolean;
  onCancel?: () => void;
}) {
  const t = useT();
  const { backend } = useBackend();
  const { platform } = useUiState();
  const [mode, setMode] = useState<Mode>("invite");
  const [invite, setInvite] = useState("");
  const [storage, setStorage] = useState(emptyStorageForm);
  const [syncKey, setSyncKey] = useState("");
  const [password, setPassword] = useState("");
  const [spacePassword, setSpacePassword] = useState("");
  const [deviceName, setDeviceName] = useState(() => t(`sync.platformDevice.${platform}`));
  const submit = useSubmit();
  const sourceReady =
    mode === "invite" ? invite.trim() !== "" : storageComplete(storage) && syncKey.trim() !== "";
  const ready = sourceReady && password !== "";
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const source: JoinSource =
      mode === "invite"
        ? { type: "invite", text: invite.trim() }
        : { type: "manual", storage: storageConfig(storage), sync_key: syncKey.trim() };
    const space_password = newVault || spacePassword === "" ? undefined : spacePassword;
    await submit.run(() =>
      backend.dispatch({
        command: "sync_join",
        source,
        password,
        device_name: deviceName,
        space_password,
      }),
    );
    setPassword("");
    setSpacePassword("");
  };
  const error = submit.error === undefined ? undefined : errorText(t, submit.error);
  // A space that does not open with the other password typed in says so there.
  const spaceError = !newVault && spacePassword !== "" && submit.error === "sync_wrong_credentials";
  return (
    <form
      onSubmit={(e) => void onSubmit(e)}
      className="flex flex-col gap-3"
      data-testid="sync-join">
      <Segmented
        label={t("sync.off.joinTitle")}
        value={mode}
        onChange={setMode}
        options={[
          { value: "invite", label: t("sync.join.fromInvite") },
          { value: "key", label: t("sync.join.fromKey") },
        ]}
        className="self-start"
      />
      {mode === "invite" ? (
        <Textarea
          label={t("sync.join.invite")}
          value={invite}
          onChange={(e) => setInvite(e.target.value)}
          placeholder="lockra-invite:1:…"
          mono
          rows={3}
          spellCheck={false}
        />
      ) : (
        <>
          <StorageFields
            form={storage}
            onChange={(patch) => setStorage((form) => ({ ...form, ...patch }))}
          />
          <Input
            label={t("sync.join.syncKey")}
            value={syncKey}
            onChange={(e) => setSyncKey(e.target.value)}
            placeholder="LKS1-XXXX-XXXX-…"
            mono
            spellCheck={false}
            autoComplete="off"
          />
        </>
      )}
      {mode === "invite" && (
        <p className="-mt-1 text-[12px] text-fg-subtle">{t("sync.join.inviteHint")}</p>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <Input
          label={t("sync.deviceName")}
          value={deviceName}
          onChange={(e) => setDeviceName(e.target.value)}
          help={t("sync.deviceNameHint")}
          maxLength={64}
          // With a vault, the two passwords share the next row.
          className={newVault ? undefined : "sm:col-span-2"}
        />
        <PasswordField
          label={t(newVault ? "sync.spacePassword" : "sync.join.vaultPassword")}
          value={password}
          onChange={setPassword}
          help={t(newVault ? "sync.spacePasswordHint" : "sync.join.vaultPasswordHint")}
          autoComplete="current-password"
          error={spaceError ? undefined : error}
        />
        {!newVault && (
          <PasswordField
            label={t("sync.join.otherPassword")}
            value={spacePassword}
            onChange={setSpacePassword}
            help={t("sync.join.otherPasswordHint")}
            autoComplete="off"
            error={spaceError ? error : undefined}
          />
        )}
      </div>
      {newVault && <p className="text-[12px] text-fg-subtle">{t("sync.join.newVault")}</p>}
      <div className="flex items-center gap-2">
        <Button variant="primary" type="submit" icon="link" loading={submit.busy} disabled={!ready}>
          {t("sync.join.submit")}
        </Button>
        {onCancel && (
          <Button variant="ghost" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
        )}
      </div>
    </form>
  );
}
