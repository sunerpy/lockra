import {
  type SyncSpaceView,
  emptyStorageForm,
  errorText,
  relativeTime,
  storageComplete,
  storageConfig,
  storageFormFrom,
  storageSummary,
  syncStatusLine,
} from "@lockra/shared";
import {
  Badge,
  Banner,
  Button,
  Dialog,
  IconButton,
  Input,
  LampText,
  PasswordField,
  SettingsPane,
  SettingsRows,
  SettingsSection,
  StatusRow,
  StorageFields,
  SyncKeyReminder,
  useBackend,
  useClock,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useDispatch, useSubmit } from "../../app/dispatch";
import { JoinForm } from "../../features/sync/JoinForm";
import { InviteDialog, RecoveryKeyDialog } from "../../features/sync/SecretDialogs";

/** Settings › Sync: set up a space on storage of the user's own or join one; with a space, its
 *  status, storage, recovery key, devices, invitations, and turning it off here. */
export function Sync() {
  const t = useT();
  const { sync } = useUiState();
  return (
    <SettingsPane title={t("settings.section.sync")} lede={t("sync.lede")}>
      {sync.space === null ? <SyncOff /> : <SyncOn space={sync.space} />}
    </SettingsPane>
  );
}

function SyncOff() {
  const t = useT();
  const [open, setOpen] = useState<"create" | "join" | null>(null);
  return (
    <>
      <SettingsSection
        title={t("sync.off.createTitle")}
        description={t("sync.off.createBody")}
        data-testid="sync-create-section">
        {open === "create" ? (
          <CreateForm onCancel={() => setOpen(null)} />
        ) : (
          <Button
            variant="primary"
            icon="cloud"
            className="self-start"
            onClick={() => setOpen("create")}
            data-testid="sync-create-open">
            {t("sync.off.createSubmit")}
          </Button>
        )}
      </SettingsSection>
      <SettingsSection
        title={t("sync.off.joinTitle")}
        description={t("sync.off.joinBody")}
        data-testid="sync-join-section">
        {open === "join" ? (
          <JoinForm onCancel={() => setOpen(null)} />
        ) : (
          <Button
            icon="link"
            className="self-start"
            onClick={() => setOpen("join")}
            data-testid="sync-join-open">
            {t("sync.join.submit")}
          </Button>
        )}
      </SettingsSection>
    </>
  );
}

/** Starting to sync: the storage, this device's name and the master password. The space's
 *  recovery key is not shown here: Settings › Sync reminds of it until it is saved. */
function CreateForm({ onCancel }: { onCancel: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const { platform } = useUiState();
  const [storage, setStorage] = useState(emptyStorageForm);
  const [deviceName, setDeviceName] = useState(() => t(`sync.platformDevice.${platform}`));
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const ready = storageComplete(storage) && password !== "";
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    await submit.run(() =>
      backend.dispatch({
        command: "sync_create",
        storage: storageConfig(storage),
        password,
        device_name: deviceName,
      }),
    );
    setPassword("");
  };
  return (
    <form
      onSubmit={(e) => void onSubmit(e)}
      className="flex flex-col gap-3"
      data-testid="sync-create">
      <StorageFields
        failure={submit.error}
        form={storage}
        onChange={(patch) => setStorage((form) => ({ ...form, ...patch }))}
        pickFolder={() => backend.pickSyncFolder()}
      />
      <div className="grid gap-3 sm:grid-cols-2">
        <Input
          label={t("sync.deviceName")}
          value={deviceName}
          onChange={(e) => setDeviceName(e.target.value)}
          help={t("sync.deviceNameHint")}
          maxLength={64}
        />
        <PasswordField
          label={t("sync.masterPassword")}
          value={password}
          onChange={setPassword}
          autoComplete="current-password"
          error={submit.error === undefined ? undefined : errorText(t, submit.error)}
        />
      </div>
      <div className="flex items-center gap-2">
        <Button
          variant="primary"
          type="submit"
          icon="cloud"
          loading={submit.busy}
          disabled={!ready}>
          {t("sync.off.createSubmit")}
        </Button>
        <Button variant="ghost" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
      </div>
    </form>
  );
}

/** A device's name, or the start of its tag when the space does not know it. */
function deviceLabel(space: SyncSpaceView, tag: string): string {
  return space.devices.find((d) => d.tag === tag)?.name ?? `${tag.slice(0, 8)}…`;
}

function SyncOn({ space }: { space: SyncSpaceView }) {
  const t = useT();
  const now = useClock();
  const dispatch = useDispatch();
  const [dialog, setDialog] = useState<"invite" | "key" | "disable" | { remove: string } | null>(
    null,
  );
  const line = syncStatusLine(space.status, t, now);
  const syncing = space.status.state === "syncing";
  const removing =
    dialog !== null && typeof dialog === "object"
      ? space.devices.find((d) => d.tag === dialog.remove)
      : undefined;
  return (
    <>
      <SyncKeyReminder onShow={() => setDialog("key")} />
      <SettingsRows>
        <StatusRow
          label={t("sync.status.label")}
          data-testid="sync-status-row"
          note={
            space.keyring_pending ? (
              <span className="text-fg-muted">{t("sync.status.keyringPending")}</span>
            ) : undefined
          }>
          <div className="flex flex-wrap items-center gap-3">
            <LampText tone={line.tone} size="sm" pulse={syncing}>
              <span data-testid="sync-status">{line.text}</span>
            </LampText>
            <Button
              size="sm"
              variant="outline"
              icon="refresh"
              disabled={syncing}
              onClick={() => void dispatch({ command: "sync_now" })}
              data-testid="sync-now">
              {t("sync.status.now")}
            </Button>
          </div>
        </StatusRow>
        <StorageRow space={space} />
        <StatusRow
          label={t("sync.recoveryKey.row")}
          help={t("sync.recoveryKey.rowHint")}
          data-testid="sync-recovery-row">
          <Button size="sm" variant="ghost" icon="key" onClick={() => setDialog("key")}>
            {t("sync.recoveryKey.open")}
          </Button>
        </StatusRow>
        <DeviceNameRow space={space} />
      </SettingsRows>
      {space.rolled_back.length > 0 && (
        <div data-testid="sync-rolled-back">
          <Banner tone="warn" marker="icon">
            {t("sync.problems.rolledBack", {
              names: space.rolled_back.map((tag) => deviceLabel(space, tag)).join(", "),
            })}
          </Banner>
        </div>
      )}
      {space.unreadable.length > 0 && (
        <div data-testid="sync-unreadable">
          <Banner
            tone="warn"
            marker="icon"
            actions={
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  for (const tag of space.unreadable)
                    void dispatch({ command: "sync_remove_device", tag });
                }}>
                {t("sync.problems.removeUnreadable")}
              </Button>
            }>
            {t("sync.problems.unreadable", { n: space.unreadable.length })}
          </Banner>
        </div>
      )}
      <SettingsSection
        title={t("sync.devices.title")}
        description={t("sync.devices.description")}
        data-testid="sync-devices">
        <ul className="flex flex-col">
          {space.devices.map((device) => (
            <li
              key={device.tag}
              className="flex items-center gap-3 border-b border-border py-2 last:border-b-0"
              data-testid="sync-device">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 text-[13px] text-fg">
                  <span className="truncate">{device.name}</span>
                  {device.this_device && (
                    <Badge tone="accent">{t("sync.devices.thisDevice")}</Badge>
                  )}
                </div>
                <div className="text-[12px] text-fg-subtle">
                  {device.written_at_ms === null
                    ? t("sync.devices.never")
                    : t("sync.devices.written", {
                        when: relativeTime(t, device.written_at_ms, now),
                      })}
                </div>
              </div>
              {!device.this_device && (
                <IconButton
                  icon="trash"
                  label={t("sync.devices.remove")}
                  onClick={() => setDialog({ remove: device.tag })}
                />
              )}
            </li>
          ))}
        </ul>
      </SettingsSection>
      <div className="flex flex-wrap gap-2">
        <Button icon="qr" onClick={() => setDialog("invite")} data-testid="sync-invite-open">
          {t("sync.invite.open")}
        </Button>
        <Button
          variant="text-danger"
          onClick={() => setDialog("disable")}
          data-testid="sync-disable-open">
          {t("sync.disable.open")}
        </Button>
      </div>
      {dialog === "invite" && <InviteDialog onClose={() => setDialog(null)} />}
      {dialog === "key" && <RecoveryKeyDialog onClose={() => setDialog(null)} />}
      {dialog === "disable" && (
        <Dialog
          open
          title={t("sync.disable.title")}
          onClose={() => setDialog(null)}
          actions={
            <>
              <Button variant="ghost" onClick={() => setDialog(null)}>
                {t("common.cancel")}
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setDialog(null);
                  void dispatch({ command: "sync_disable" });
                }}
                data-autofocus>
                {t("sync.disable.confirm")}
              </Button>
            </>
          }>
          <p>{t("sync.disable.body")}</p>
        </Dialog>
      )}
      {removing !== undefined && (
        <Dialog
          open
          title={t("sync.devices.removeTitle", { name: removing.name })}
          onClose={() => setDialog(null)}
          actions={
            <>
              <Button variant="ghost" onClick={() => setDialog(null)}>
                {t("common.cancel")}
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setDialog(null);
                  void dispatch({ command: "sync_remove_device", tag: removing.tag });
                }}
                data-autofocus>
                {t("sync.devices.remove")}
              </Button>
            </>
          }>
          <p>{t("sync.devices.removeBody")}</p>
        </Dialog>
      )}
    </>
  );
}

function StorageRow({ space }: { space: SyncSpaceView }) {
  const t = useT();
  const { backend } = useBackend();
  const [editing, setEditing] = useState(false);
  const [form, setForm] = useState(() => storageFormFrom(space.storage));
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const ready = storageComplete(form) && password !== "";
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const done = await submit.run(() =>
      backend.dispatch({ command: "sync_set_storage", storage: storageConfig(form), password }),
    );
    setPassword("");
    if (done !== undefined) setEditing(false);
  };
  return (
    <>
      <StatusRow
        label={t("sync.storageRow")}
        help={
          <span
            className="mono"
            data-testid="sync-storage">{`${t(`sync.storage.${space.storage.kind}`)} · ${storageSummary(space.storage)}`}</span>
        }>
        {!editing && (
          <Button
            size="sm"
            variant="ghost"
            icon="edit"
            onClick={() => {
              setForm(storageFormFrom(space.storage));
              setEditing(true);
            }}
            data-testid="sync-storage-edit">
            {t("sync.storageEdit")}
          </Button>
        )}
      </StatusRow>
      {editing && (
        <form
          onSubmit={(e) => void onSubmit(e)}
          className="flex flex-col gap-3 border-b border-border py-3"
          data-testid="sync-storage-form">
          <p className="text-[12px] text-fg-muted">{t("sync.storageEditBody")}</p>
          <StorageFields
            failure={submit.error}
            form={form}
            onChange={(patch) => setForm((current) => ({ ...current, ...patch }))}
            pickFolder={() => backend.pickSyncFolder()}
          />
          <PasswordField
            label={t("sync.masterPassword")}
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            className="max-w-[24rem]"
            error={submit.error === undefined ? undefined : errorText(t, submit.error)}
          />
          <div className="flex items-center gap-2">
            <Button variant="primary" type="submit" loading={submit.busy} disabled={!ready}>
              {t("common.save")}
            </Button>
            <Button variant="ghost" onClick={() => setEditing(false)}>
              {t("common.cancel")}
            </Button>
          </div>
        </form>
      )}
    </>
  );
}

function DeviceNameRow({ space }: { space: SyncSpaceView }) {
  const t = useT();
  const { backend } = useBackend();
  const [name, setName] = useState<string | null>(null);
  const submit = useSubmit();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (name === null) return;
    const done = await submit.run(() => backend.dispatch({ command: "sync_rename_device", name }));
    if (done !== undefined) setName(null);
  };
  return (
    <StatusRow
      label={t("sync.device.label")}
      help={
        name === null ? <span data-testid="sync-device-name">{space.device_name}</span> : undefined
      }
      note={
        submit.error === undefined ? undefined : (
          <span className="text-danger">{errorText(t, submit.error)}</span>
        )
      }>
      {name === null ? (
        <Button
          size="sm"
          variant="ghost"
          icon="edit"
          onClick={() => setName(space.device_name)}
          data-testid="sync-rename">
          {t("sync.device.rename")}
        </Button>
      ) : (
        <form onSubmit={(e) => void onSubmit(e)} className="flex items-center gap-2">
          <Input
            aria-label={t("sync.deviceName")}
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={64}
            size="sm"
            autoFocus
          />
          <Button size="sm" variant="primary" type="submit" loading={submit.busy}>
            {t("common.save")}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setName(null)}>
            {t("common.cancel")}
          </Button>
        </form>
      )}
    </StatusRow>
  );
}
