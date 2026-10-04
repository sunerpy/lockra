// Joining a sync space on the phone: the invitation another device shows, scanned with the camera
// (its text goes from the camera to the core and never through here) or pasted (a sealed one with
// its code), or the storage and the sync key typed in. On the welcome screen (`newVault`) the
// master password of the space's devices becomes this phone's; with a vault, its own master
// password is checked and opens the space, and only when the space's devices use another one does
// the form ask for that too.
import {
  type JoinSource,
  emptyStorageForm,
  errorText,
  isLockraError,
  isSealedInvite,
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
  useSubmit,
  useT,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { overPhoneScreen } from "../app/phone-screen";

type Mode = "scan" | "invite" | "key";

export function JoinSync({
  newVault = false,
  onJoined,
}: {
  newVault?: boolean;
  onJoined?: () => void;
}) {
  const t = useT();
  const { backend } = useBackend();
  const [mode, setMode] = useState<Mode>("scan");
  const [invite, setInvite] = useState("");
  const [code, setCode] = useState("");
  // The core said this vault's password opens nothing in the space: the space's is asked for.
  const [askSpace, setAskSpace] = useState(false);
  const [storage, setStorage] = useState(emptyStorageForm);
  const [syncKey, setSyncKey] = useState("");
  const [password, setPassword] = useState("");
  const [spacePassword, setSpacePassword] = useState("");
  const [deviceName, setDeviceName] = useState(() => t("sync.platformDevice.android"));
  const submit = useSubmit();
  const sealed = mode === "invite" && isSealedInvite(invite);
  const sourceReady =
    mode === "scan" ||
    (mode === "invite"
      ? invite.trim() !== "" && (!sealed || code.trim() !== "")
      : storageComplete(storage) && syncKey.trim() !== "");
  const ready = sourceReady && password !== "";
  const space_password = newVault || spacePassword === "" ? undefined : spacePassword;
  const forget = () => {
    setPassword("");
    setSpacePassword("");
  };
  /** Asked for the space's password, the form shows its field. */
  const watch = async <T,>(work: () => Promise<T>): Promise<T> => {
    try {
      return await work();
    } catch (failure: unknown) {
      if (isLockraError(failure) && failure.code === "sync_space_password_needed")
        setAskSpace(true);
      throw failure;
    }
  };
  const scan = async () => {
    const joined = await submit.run(() =>
      watch(() =>
        overPhoneScreen(() =>
          backend.scanJoin(
            { prompt: t("mobile.sync.scanPrompt"), cancel: t("common.cancel") },
            { password, deviceName, spacePassword: space_password },
          ),
        ),
      ),
    );
    // Left without a code, nothing was tried: the passwords stay.
    if (joined === false) return;
    forget();
    if (joined === true) onJoined?.();
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    if (mode === "scan") {
      await scan();
      return;
    }
    const source: JoinSource =
      mode === "invite"
        ? { type: "invite", text: invite.trim(), code: sealed ? code.trim() : undefined }
        : { type: "manual", storage: storageConfig(storage), sync_key: syncKey.trim() };
    const done = await submit.run(() =>
      watch(() =>
        backend.dispatch({
          command: "sync_join",
          source,
          password,
          device_name: deviceName,
          space_password,
        }),
      ),
    );
    forget();
    if (done !== undefined) onJoined?.();
  };
  return (
    <form
      onSubmit={(e) => void onSubmit(e)}
      className="flex flex-col gap-4"
      data-testid="sync-join">
      <Segmented
        size="lg"
        label={t("sync.off.joinTitle")}
        value={mode}
        onChange={setMode}
        options={[
          { value: "scan", label: t("mobile.sync.fromScan") },
          { value: "invite", label: t("sync.join.fromInvite") },
          { value: "key", label: t("sync.join.fromKey") },
        ]}
        className="self-start"
      />
      {mode === "scan" && <p className="text-[14px] text-fg-muted">{t("mobile.sync.scanHint")}</p>}
      {mode === "invite" && (
        <>
          <Textarea
            label={t("sync.join.invite")}
            value={invite}
            onChange={(e) => setInvite(e.target.value)}
            placeholder="lockra-invite:…"
            mono
            rows={4}
            spellCheck={false}
          />
          <p className="-mt-2 text-[13px] text-fg-muted">{t("sync.join.inviteHint")}</p>
          {sealed && (
            <Input
              size="lg"
              label={t("sync.join.code")}
              value={code}
              onChange={(e) => setCode(e.target.value)}
              help={t("sync.join.codeHint")}
              placeholder="XXXXX-XXXXX"
              mono
              spellCheck={false}
              autoComplete="off"
            />
          )}
        </>
      )}
      {mode === "key" && (
        <>
          <StorageFields
            failure={submit.error}
            size="lg"
            form={storage}
            onChange={(patch) => setStorage((form) => ({ ...form, ...patch }))}
          />
          <Input
            size="lg"
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
        label={t(newVault ? "sync.spacePassword" : "sync.join.vaultPassword")}
        value={password}
        onChange={setPassword}
        help={t(newVault ? "sync.spacePasswordHint" : "sync.join.vaultPasswordHint")}
        autoComplete="current-password"
      />
      {newVault && <p className="text-[13px] text-fg-muted">{t("mobile.sync.newVault")}</p>}
      {!newVault && askSpace && (
        <PasswordField
          size="lg"
          label={t("sync.join.otherPassword")}
          value={spacePassword}
          onChange={setSpacePassword}
          help={t("sync.join.otherPasswordHint")}
          autoComplete="off"
        />
      )}
      {submit.error !== undefined && (
        <p role="alert" className="text-[13px] text-danger">
          {errorText(t, submit.error)}
        </p>
      )}
      <Button
        variant="primary"
        size="lg"
        type="submit"
        icon={mode === "scan" ? "scan" : "link"}
        loading={submit.busy}
        disabled={!ready}>
        {t(mode === "scan" ? "mobile.sync.scan" : "sync.join.submit")}
      </Button>
    </form>
  );
}
