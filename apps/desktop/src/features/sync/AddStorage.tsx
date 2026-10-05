// Settings › Sync, a space on the LAN alone: a storage of the user's own beside it, from its
// settings or from another device's invitation of the same space.
import {
  type StorageSource,
  emptyStorageForm,
  errorText,
  isSealedInvite,
  storageComplete,
  storageConfig,
} from "@lockra/shared";
import {
  Button,
  Input,
  PasswordField,
  Segmented,
  SettingsSection,
  StorageFields,
  Textarea,
  useBackend,
  useT,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useSubmit } from "../../app/dispatch";

type Mode = "storage" | "invite";

export function AddStorageSection() {
  const t = useT();
  const { backend } = useBackend();
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<Mode>("storage");
  const [storage, setStorage] = useState(emptyStorageForm);
  const [invite, setInvite] = useState("");
  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const sealed = mode === "invite" && isSealedInvite(invite);
  const sourceReady =
    mode === "storage"
      ? storageComplete(storage)
      : invite.trim() !== "" && (!sealed || code.trim() !== "");
  const ready = sourceReady && password !== "";
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const source: StorageSource =
      mode === "storage"
        ? { type: "storage", storage: storageConfig(storage) }
        : { type: "invite", text: invite.trim(), code: sealed ? code.trim() : undefined };
    const done = await submit.run(() =>
      backend.dispatch({ command: "sync_add_storage", source, password }),
    );
    setPassword("");
    if (done !== undefined) setOpen(false);
  };
  const error = submit.error === undefined ? undefined : errorText(t, submit.error);
  const codeError = submit.error === "sync_invite_code_wrong";
  return (
    <SettingsSection
      title={t("sync.addStorage.title")}
      description={t("sync.addStorage.body")}
      data-testid="sync-add-storage">
      {open ? (
        <form
          onSubmit={(e) => void onSubmit(e)}
          className="flex flex-col gap-3"
          data-testid="sync-add-storage-form">
          <Segmented
            label={t("sync.addStorage.title")}
            value={mode}
            onChange={setMode}
            options={[
              { value: "storage", label: t("sync.addStorage.fromStorage") },
              { value: "invite", label: t("sync.addStorage.fromInvite") },
            ]}
            className="self-start"
          />
          {mode === "storage" ? (
            <StorageFields
              failure={submit.error}
              form={storage}
              onChange={(patch) => setStorage((form) => ({ ...form, ...patch }))}
            />
          ) : (
            <>
              <Textarea
                label={t("sync.join.invite")}
                value={invite}
                onChange={(e) => setInvite(e.target.value)}
                placeholder="lockra-invite:…"
                mono
                rows={3}
                spellCheck={false}
              />
              <p className="-mt-1 text-[12px] text-fg-subtle">{t("sync.addStorage.inviteHint")}</p>
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
            </>
          )}
          <PasswordField
            label={t("sync.masterPassword")}
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            className="max-w-[24rem]"
            error={codeError ? undefined : error}
          />
          <div className="flex items-center gap-2">
            <Button
              variant="primary"
              type="submit"
              icon="cloud"
              loading={submit.busy}
              disabled={!ready}>
              {t("sync.addStorage.submit")}
            </Button>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              {t("common.cancel")}
            </Button>
          </div>
        </form>
      ) : (
        <Button
          icon="cloud"
          className="self-start"
          onClick={() => setOpen(true)}
          data-testid="sync-add-storage-open">
          {t("sync.addStorage.open")}
        </Button>
      )}
    </SettingsSection>
  );
}
