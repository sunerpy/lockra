// Settings › Sync on the phone: set up a space on storage of the user's own, or join one; with a
// space, how it is doing, where it is stored, this phone's name, the space's devices, invitations
// for more, and turning it off here (the desktop's Settings › Sync).
import {
  type SyncSpaceView,
  errorText,
  relativeTime,
  storageSummary,
  syncStatusLine,
} from "@lockra/shared";
import {
  Badge,
  Banner,
  Button,
  Card,
  Dialog,
  IconButton,
  Input,
  LampText,
  SyncKeyReminder,
  useBackend,
  useDispatch,
  useNow,
  useSubmit,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { ActionRow } from "../components/ActionRow";
import { Page } from "../components/Page";
import { Section } from "../components/Rows";

export function Sync() {
  const t = useT();
  const { sync } = useUiState();
  return (
    <Page title={t("settings.section.sync")} testId="page-sync">
      {sync.space === null ? <SyncOff /> : <SyncOn space={sync.space} />}
    </Page>
  );
}

function SyncOff() {
  const t = useT();
  const nav = useNav();
  return (
    <div className="flex flex-col gap-4">
      <p className="text-[14px] text-fg-muted">{t("sync.lede")}</p>
      <Card padding="none" className="p-1">
        <ActionRow
          icon="cloud"
          label={t("sync.off.createTitle")}
          hint={t("sync.off.createBody")}
          opensPage
          onClick={() => nav.open({ name: "syncSetup" })}
          testId="sync-setup-open"
        />
        <ActionRow
          icon="link"
          label={t("sync.off.joinTitle")}
          hint={t("sync.off.joinBody")}
          opensPage
          onClick={() => nav.open({ name: "syncJoin" })}
          testId="sync-join-open"
        />
      </Card>
    </div>
  );
}

/** A device's name, or the start of its tag when the space does not know it. */
function deviceLabel(space: SyncSpaceView, tag: string): string {
  return space.devices.find((d) => d.tag === tag)?.name ?? `${tag.slice(0, 8)}…`;
}

function SyncOn({ space }: { space: SyncSpaceView }) {
  const t = useT();
  const nav = useNav();
  const now = useNow();
  const dispatch = useDispatch();
  const [dialog, setDialog] = useState<"rename" | "disable" | { remove: string } | null>(null);
  const line = syncStatusLine(space.status, t, now);
  const syncing = space.status.state === "syncing";
  const removing =
    dialog !== null && typeof dialog === "object"
      ? space.devices.find((d) => d.tag === dialog.remove)
      : undefined;
  const close = () => setDialog(null);
  return (
    <div className="flex flex-col gap-5">
      <SyncKeyReminder size="lg" />
      <Card className="flex flex-col gap-3">
        <LampText tone={line.tone} pulse={syncing}>
          <span data-testid="sync-status">{line.text}</span>
        </LampText>
        {space.keyring_pending && (
          <p className="text-[13px] text-fg-muted">{t("sync.status.keyringPending")}</p>
        )}
        <Button
          size="lg"
          icon="refresh"
          disabled={syncing}
          onClick={() => void dispatch({ command: "sync_now" })}
          data-testid="sync-now">
          {t("sync.status.now")}
        </Button>
      </Card>
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
      <Section title={t("sync.storageRow")}>
        <p className="mono px-4 py-3 text-[13px] break-all text-fg" data-testid="sync-storage">
          {`${t(`sync.storage.${space.storage.kind}`)} · ${storageSummary(space.storage)}`}
        </p>
        <div className="p-1">
          <ActionRow
            icon="edit"
            label={t("sync.storageEdit")}
            opensPage
            onClick={() => nav.open({ name: "syncStorage" })}
            testId="sync-storage-edit"
          />
        </div>
      </Section>
      <Section title={t("sync.device.label")}>
        <p className="px-4 py-3 text-[15px] text-fg" data-testid="sync-device-name">
          {space.device_name}
        </p>
        <div className="p-1">
          <ActionRow
            icon="edit"
            label={t("sync.device.rename")}
            onClick={() => setDialog("rename")}
            testId="sync-rename"
          />
        </div>
      </Section>
      <Section title={t("sync.devices.title")}>
        {space.devices.map((device) => (
          <div
            key={device.tag}
            className="flex min-h-14 items-center gap-3 px-4 py-2"
            data-testid="sync-device">
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="flex items-center gap-2 text-[15px] text-fg">
                <span className="truncate">{device.name}</span>
                {device.this_device && <Badge tone="accent">{t("sync.devices.thisDevice")}</Badge>}
              </span>
              <span className="text-[13px] text-fg-muted">
                {device.written_at_ms === null
                  ? t("sync.devices.never")
                  : t("sync.devices.written", {
                      when: relativeTime(t, device.written_at_ms, now),
                    })}
              </span>
            </div>
            {!device.this_device && (
              <IconButton
                icon="trash"
                label={t("sync.devices.remove")}
                size={40}
                onClick={() => setDialog({ remove: device.tag })}
              />
            )}
          </div>
        ))}
      </Section>
      <p className="-mt-3 px-1 text-[13px] text-fg-muted">{t("sync.devices.description")}</p>
      <Card padding="none" className="p-1">
        <ActionRow
          icon="qr"
          label={t("sync.invite.open")}
          opensPage
          onClick={() => nav.open({ name: "syncInvite" })}
          testId="sync-invite-open"
        />
        <ActionRow
          icon="close"
          label={t("sync.disable.open")}
          danger
          onClick={() => setDialog("disable")}
          testId="sync-disable-open"
        />
      </Card>
      {dialog === "rename" && <RenameDialog name={space.device_name} onClose={close} />}
      {dialog === "disable" && (
        <Dialog
          open
          title={t("sync.disable.title")}
          onClose={close}
          actions={
            <>
              <Button variant="ghost" size="lg" onClick={close}>
                {t("common.cancel")}
              </Button>
              <Button
                variant="danger"
                size="lg"
                onClick={() => {
                  close();
                  void dispatch({ command: "sync_disable" });
                }}>
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
          onClose={close}
          actions={
            <>
              <Button variant="ghost" size="lg" onClick={close}>
                {t("common.cancel")}
              </Button>
              <Button
                variant="danger"
                size="lg"
                onClick={() => {
                  close();
                  void dispatch({ command: "sync_remove_device", tag: removing.tag });
                }}>
                {t("sync.devices.remove")}
              </Button>
            </>
          }>
          <p>{t("sync.devices.removeBody")}</p>
        </Dialog>
      )}
    </div>
  );
}

function RenameDialog({ name: current, onClose }: { name: string; onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const [name, setName] = useState(current);
  const submit = useSubmit();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    const done = await submit.run(() => backend.dispatch({ command: "sync_rename_device", name }));
    if (done !== undefined) onClose();
  };
  return (
    <Dialog
      open
      title={t("mobile.sync.renameTitle")}
      onClose={onClose}
      actions={
        <>
          <Button variant="ghost" size="lg" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            type="submit"
            form="sync-rename"
            loading={submit.busy}>
            {t("common.save")}
          </Button>
        </>
      }>
      <form id="sync-rename" onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
        <Input
          size="lg"
          label={t("sync.deviceName")}
          value={name}
          onChange={(e) => setName(e.target.value)}
          help={t("sync.deviceNameHint")}
          maxLength={64}
          error={submit.error === undefined ? undefined : errorText(t, submit.error)}
        />
      </form>
    </Dialog>
  );
}
