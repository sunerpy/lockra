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

/** Joining a space: another device's invitation (a sealed one with its code), which hands the
 *  space over, or a recovery with the storage and the recovery key typed in. An invitation needs
 *  this computer's own master password alone: on the welcome screen (`newVault`) a new one, typed
 *  twice; with a vault, the vault's. A recovery opens the space with the master password of a
 *  device in it: on the welcome screen it becomes this vault's; with a vault, its own is tried
 *  first, and only when the space's devices use another one does the form ask for that too. An
 *  invitation without storage (a space in a cloud drive folder of the other computer) asks how
 *  this computer reaches the space: the same drive's folder, or its WebDAV. */
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
  const [repeat, setRepeat] = useState("");
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
  // A new vault's own password, typed twice: an invitation asks no other.
  const choosing = newVault && mode === "invite";
  const mismatch = choosing && repeat !== "" && repeat !== password;
  const ready = sourceReady && password !== "" && (!choosing || repeat === password);
  // A recovery whose space's devices use another password asks for it too.
  const showSpace = !newVault && askSpace && mode === "key";
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
    const space_password = showSpace && spacePassword !== "" ? spacePassword : undefined;
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
    setRepeat("");
    setSpacePassword("");
  };
  // Asked for this computer's storage, the section that asks says why.
  const error =
    submit.error === undefined || submit.error === "sync_invite_needs_storage"
      ? undefined
      : errorText(t, submit.error);
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
          // Two passwords share the next row: the space's besides this vault's, or a new one twice.
          className={showSpace || choosing ? "sm:col-span-2" : undefined}
        />
        <PasswordField
          label={t(
            choosing
              ? "sync.join.newPassword"
              : newVault
                ? "sync.spacePassword"
                : "sync.join.vaultPassword",
          )}
          value={password}
          onChange={setPassword}
          help={t(
            choosing
              ? "sync.join.newPasswordHint"
              : newVault
                ? "sync.spacePasswordHint"
                : "sync.join.vaultPasswordHint",
          )}
          autoComplete={choosing ? "new-password" : "current-password"}
          error={spaceError || codeError ? undefined : error}
        />
        {choosing && (
          <PasswordField
            label={t("welcome.create.repeat")}
            value={repeat}
            onChange={setRepeat}
            autoComplete="new-password"
            error={mismatch ? t("welcome.create.mismatch") : undefined}
          />
        )}
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
