import {
  type JoinSource,
  emptyStorageForm,
  errorText,
  isLockraError,
  isSealedInvite,
  storageComplete,
  storageConfig,
  withPreset,
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

/** Joining a space: another device's invitation (a sealed one with its code), or the storage and
 *  the sync key typed in. On the welcome screen (`newVault`) the master password of a device in
 *  the space becomes this vault's; with a vault, its own master password is checked and opens the
 *  space, and only when the space's devices use another one does the form ask for that too. An
 *  invitation with the sync key alone (a space in a cloud drive folder of the other computer)
 *  asks how this computer reaches the space: the same drive's folder, or its WebDAV. */
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
  const [code, setCode] = useState("");
  // The core said this vault's password opens nothing in the space: the space's is asked for.
  const [askSpace, setAskSpace] = useState(false);
  // The invitation holds the sync key alone: this computer's way to the space is asked for.
  const [askStorage, setAskStorage] = useState(false);
  const [storage, setStorage] = useState(emptyStorageForm);
  const [syncKey, setSyncKey] = useState("");
  const [password, setPassword] = useState("");
  const [spacePassword, setSpacePassword] = useState("");
  const [deviceName, setDeviceName] = useState(() => t(`sync.platformDevice.${platform}`));
  const submit = useSubmit();
  const sealed = mode === "invite" && isSealedInvite(invite);
  const withStorage = mode === "key" || askStorage;
  const sourceReady =
    (mode === "invite"
      ? invite.trim() !== "" && (!sealed || code.trim() !== "")
      : syncKey.trim() !== "") &&
    (!withStorage || storageComplete(storage));
  const ready = sourceReady && password !== "";
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const source: JoinSource =
      mode === "invite"
        ? {
            type: "invite",
            text: invite.trim(),
            code: sealed ? code.trim() : undefined,
            storage: askStorage ? storageConfig(storage) : undefined,
          }
        : { type: "manual", storage: storageConfig(storage), sync_key: syncKey.trim() };
    const space_password = newVault || spacePassword === "" ? undefined : spacePassword;
    await submit.run(async () => {
      try {
        return await backend.dispatch({
          command: "sync_join",
          source,
          password,
          device_name: deviceName,
          space_password,
        });
      } catch (failure: unknown) {
        if (isLockraError(failure) && failure.code === "sync_space_password_needed")
          setAskSpace(true);
        if (isLockraError(failure) && failure.code === "sync_invite_needs_storage" && !askStorage) {
          setAskStorage(true);
          // Most likely the same cloud drive's folder on this computer.
          setStorage((form) => withPreset(form, { kind: "folder" }));
        }
        throw failure;
      }
    });
    setPassword("");
    setSpacePassword("");
  };
  // Asked for this computer's storage, the section that asks says why.
  const error =
    submit.error === undefined || submit.error === "sync_invite_needs_storage"
      ? undefined
      : errorText(t, submit.error);
  const showSpace = !newVault && askSpace;
  // What the space's password is asked for, or refused for, shows under it.
  const spaceError =
    showSpace &&
    (submit.error === "sync_space_password_needed" ||
      (spacePassword !== "" && submit.error === "sync_wrong_credentials"));
  const codeError = submit.error === "sync_invite_code_wrong";
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
          placeholder="lockra-invite:…"
          mono
          rows={3}
          spellCheck={false}
        />
      ) : (
        <>
          <StorageFields
            failure={submit.error}
            form={storage}
            onChange={(patch) => setStorage((form) => ({ ...form, ...patch }))}
            pickFolder={() => backend.pickSyncFolder()}
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
      {mode === "invite" && askStorage && (
        <div className="flex flex-col gap-3" data-testid="sync-join-storage">
          <p className="text-[12px] text-fg-muted">{t("sync.join.needsStorage")}</p>
          <StorageFields
            failure={submit.error}
            form={storage}
            onChange={(patch) => setStorage((form) => ({ ...form, ...patch }))}
            pickFolder={() => backend.pickSyncFolder()}
          />
        </div>
      )}
      {sealed && (
        <Input
          label={t("sync.join.code")}
          value={code}
          onChange={(e) => setCode(e.target.value)}
          help={t("sync.join.codeHint")}
          error={codeError ? error : undefined}
          placeholder="XXXXX-XXXXX"
          mono
          spellCheck={false}
          autoComplete="off"
          className="sm:max-w-xs"
        />
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <Input
          label={t("sync.deviceName")}
          value={deviceName}
          onChange={(e) => setDeviceName(e.target.value)}
          help={t("sync.deviceNameHint")}
          maxLength={64}
          // Asked for the space's password too, the two passwords share the next row.
          className={showSpace ? "sm:col-span-2" : undefined}
        />
        <PasswordField
          label={t(newVault ? "sync.spacePassword" : "sync.join.vaultPassword")}
          value={password}
          onChange={setPassword}
          help={t(newVault ? "sync.spacePasswordHint" : "sync.join.vaultPasswordHint")}
          autoComplete="current-password"
          error={spaceError || codeError ? undefined : error}
        />
        {showSpace && (
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
