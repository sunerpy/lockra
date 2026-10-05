// Settings › Sync over the local network: this computer as the hub its devices pair with, or
// connected to another computer's hub; the request of a device asking to pair, and the code this
// device shows while it asks.
import {
  type ErrorCode,
  type LanView,
  type PairRequestView,
  type SyncSpaceView,
  errorText,
  groupCode,
  isPairOffer,
  platformLabel,
} from "@lockra/shared";
import {
  Badge,
  Banner,
  Button,
  Dialog,
  IconButton,
  Input,
  PasswordField,
  SettingsSection,
  StatusRow,
  Textarea,
  Toggle,
  useBackend,
  useT,
  useUiState,
  useUpdateSettings,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useDispatch, useSubmit } from "../../app/dispatch";
import { PresenceDialog } from "./Presence";
import { LanOfferDialog } from "./SecretDialogs";

type HubView = Extract<LanView, { role: "hub" }>;

/** Turning the hub on also keeps Lockra running when its window closes, where a tray can hold it:
 *  the paired devices' changes go on arriving (the switch in the hub's section turns it off). */
function useKeepRunning(): () => void {
  const { platform } = useUiState();
  const update = useUpdateSettings();
  return () => {
    if (platform !== "linux") update({ run_in_background: true });
  };
}

/** The failures about the password typed; the others are about the pairing code or the hub. */
export function passwordFailure(code: ErrorCode): boolean {
  return code === "wrong_password" || code === "password_too_short" || code === "rate_limited";
}

/** This device asking a hub to pair: the code to compare there, while the hub's user decides. */
export function JoiningPanel() {
  const t = useT();
  const { sync } = useUiState();
  if (sync.joining === null) return null;
  return (
    <div
      className="flex flex-col gap-1 rounded-10 bg-inset p-3 hairline"
      role="status"
      data-testid="sync-lan-joining">
      <div className="text-[13px] text-fg">
        {t("sync.lan.joining.title", { hub: sync.joining.hub_name })}
      </div>
      <p className="text-[12px] text-fg-muted">{t("sync.lan.joining.body")}</p>
      <div
        className="mono text-[24px] tracking-[0.2em] text-fg"
        data-testid="sync-lan-joining-code">
        {groupCode(sync.joining.code)}
      </div>
    </div>
  );
}

/** A new space on the LAN alone, with this computer its hub: the master password seals it. */
export function LanCreateForm({ onCancel }: { onCancel: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const { platform } = useUiState();
  const [deviceName, setDeviceName] = useState(() => t(`sync.platformDevice.${platform}`));
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const keepRunning = useKeepRunning();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    const done = await submit.run(() =>
      backend.dispatch({ command: "sync_lan_enable", password, device_name: deviceName }),
    );
    setPassword("");
    if (done !== undefined) keepRunning();
  };
  return (
    <form
      onSubmit={(e) => void onSubmit(e)}
      className="flex flex-col gap-3"
      data-testid="sync-lan-create">
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
      {platform === "windows" && (
        <p className="text-[12px] text-fg-subtle">{t("sync.lan.firewall")}</p>
      )}
      <div className="flex items-center gap-2">
        <Button
          variant="primary"
          type="submit"
          icon="monitor"
          loading={submit.busy}
          disabled={password === ""}>
          {t("sync.off.lanSubmit")}
        </Button>
        <Button variant="ghost" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
      </div>
    </form>
  );
}

/** A space of this device connecting to another computer's hub: its pairing code, and this
 *  vault's master password (this device's keyring goes in under it). */
function ConnectForm({ space, onCancel }: { space: SyncSpaceView; onCancel: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const [text, setText] = useState("");
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const ready = isPairOffer(text) && password !== "";
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    await submit.run(() =>
      backend.dispatch({
        command: "sync_lan_join",
        text: text.trim(),
        password,
        device_name: space.device_name,
      }),
    );
    setPassword("");
  };
  const error = submit.error;
  return (
    <form
      onSubmit={(e) => void onSubmit(e)}
      className="flex flex-col gap-3"
      data-testid="sync-lan-connect-form">
      <p className="text-[12px] text-fg-muted">{t("sync.lan.connectBody")}</p>
      <Textarea
        label={t("sync.lan.offer.text")}
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="lockra-pair:1:…"
        mono
        rows={3}
        spellCheck={false}
      />
      {error !== undefined && !passwordFailure(error) && (
        <p className="-mt-1 text-[12px] text-danger" role="alert">
          {errorText(t, error)}
        </p>
      )}
      <PasswordField
        label={t("sync.join.vaultPassword")}
        value={password}
        onChange={setPassword}
        help={t("sync.join.vaultPasswordHint")}
        autoComplete="current-password"
        className="max-w-[24rem]"
        error={error !== undefined && passwordFailure(error) ? errorText(t, error) : undefined}
      />
      <JoiningPanel />
      <div className="flex items-center gap-2">
        <Button variant="primary" type="submit" icon="link" loading={submit.busy} disabled={!ready}>
          {t("sync.lan.connectSubmit")}
        </Button>
        <Button variant="ghost" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
      </div>
    </form>
  );
}

/** A confirmation with a danger button; confirming closes it first. */
function Confirm({
  title,
  body,
  confirm,
  onConfirm,
  onClose,
}: {
  title: string;
  body: string;
  confirm: string;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const t = useT();
  return (
    <Dialog
      open
      title={title}
      onClose={onClose}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="danger"
            onClick={() => {
              onClose();
              onConfirm();
            }}
            data-autofocus>
            {confirm}
          </Button>
        </>
      }>
      <p>{body}</p>
    </Dialog>
  );
}

/** A device asking to pair, with the code both screens show. Refusing is the default answer: the
 *  focus starts there, and Esc refuses. */
function PairRequestDialog({
  request,
  busy,
  onAnswer,
}: {
  request: PairRequestView;
  busy: boolean;
  onAnswer: (approve: boolean) => void;
}) {
  const t = useT();
  return (
    <Dialog
      open
      title={t("sync.lan.request.title", { name: request.name })}
      onClose={() => onAnswer(false)}
      width={440}
      actions={
        <>
          <Button variant="ghost" onClick={() => onAnswer(false)} data-autofocus>
            {t("sync.lan.request.refuse")}
          </Button>
          <Button variant="primary" loading={busy} onClick={() => onAnswer(true)}>
            {t("sync.lan.request.approve")}
          </Button>
        </>
      }>
      <div className="flex flex-col gap-3" data-testid="lan-request">
        <p>{t("sync.lan.request.body")}</p>
        <div
          className="mono text-center text-[28px] tracking-[0.2em] text-fg"
          data-testid="lan-request-code">
          {groupCode(request.code)}
        </div>
        <p className="text-[12px] text-fg-subtle">
          {`${platformLabel(t, request.platform)} · ${t("sync.lan.request.hint")}`}
        </p>
      </div>
    </Dialog>
  );
}

function LanHub({ space, hub }: { space: SyncSpaceView; hub: HubView }) {
  const t = useT();
  const { backend } = useBackend();
  const { platform, settings } = useUiState();
  const update = useUpdateSettings();
  const dispatch = useDispatch();
  const answering = useSubmit();
  const [dialog, setDialog] = useState<"offer" | "disable" | { unpair: string } | null>(null);
  const unpairing =
    dialog !== null && typeof dialog === "object"
      ? hub.peers.find((peer) => peer.peer_id === dialog.unpair)
      : undefined;
  const answer = (approve: boolean) =>
    void answering.run(() => backend.dispatch({ command: "sync_lan_answer", approve }));
  const close = () => setDialog(null);
  return (
    <SettingsSection
      title={t("sync.lan.title")}
      description={t("sync.lan.hubBody")}
      aside={<Badge mono>{t("sync.lan.port", { port: hub.port })}</Badge>}
      data-testid="sync-lan">
      {!hub.serving && (
        <Banner tone="warn" marker="icon">
          {t("sync.lan.notServing")}
        </Banner>
      )}
      {answering.error !== undefined && (
        <div role="alert">
          <Banner tone="danger" marker="icon">
            {errorText(t, answering.error)}
          </Banner>
        </div>
      )}
      <div>
        <div className="text-[12px] text-fg-muted">{t("sync.lan.peers")}</div>
        {hub.peers.length === 0 ? (
          <p className="py-2 text-[12px] text-fg-subtle">{t("sync.lan.peersEmpty")}</p>
        ) : (
          <ul className="flex flex-col">
            {hub.peers.map((peer) => (
              <li
                key={peer.peer_id}
                className="flex items-center gap-3 border-b border-border py-2 last:border-b-0"
                data-testid="sync-lan-peer">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13px] text-fg">{peer.name}</div>
                  <div className="text-[12px] text-fg-subtle">
                    {platformLabel(t, peer.platform)}
                  </div>
                </div>
                <IconButton
                  icon="trash"
                  label={t("sync.lan.unpair")}
                  onClick={() => setDialog({ unpair: peer.peer_id })}
                />
              </li>
            ))}
          </ul>
        )}
      </div>
      {platform !== "linux" && (
        <StatusRow
          label={t("sync.lan.background")}
          help={t("sync.lan.backgroundHint")}
          data-testid="sync-lan-background">
          <Toggle
            checked={settings.run_in_background}
            onChange={(run_in_background) => update({ run_in_background })}
            ariaLabel={t("sync.lan.background")}
          />
        </StatusRow>
      )}
      {platform === "windows" && (
        <p className="text-[12px] text-fg-subtle">{t("sync.lan.firewall")}</p>
      )}
      <div className="flex flex-wrap gap-2">
        <Button
          icon="qr"
          disabled={!hub.serving}
          onClick={() => setDialog("offer")}
          data-testid="sync-lan-pair">
          {t("sync.lan.pair")}
        </Button>
        <Button
          variant="text-danger"
          onClick={() => setDialog("disable")}
          data-testid="sync-lan-disable">
          {t("sync.lan.disable")}
        </Button>
      </div>
      {dialog === "offer" && <LanOfferDialog onClose={close} />}
      {hub.request !== null && (
        <PairRequestDialog request={hub.request} busy={answering.busy} onAnswer={answer} />
      )}
      {dialog === "disable" && (
        <Confirm
          title={t("sync.lan.disableTitle")}
          body={t(space.storage === null ? "sync.lan.disableHubOnly" : "sync.lan.disableHub")}
          confirm={t("sync.lan.disable")}
          onConfirm={() => void dispatch({ command: "sync_lan_disable" })}
          onClose={close}
        />
      )}
      {unpairing !== undefined && (
        <Confirm
          title={t("sync.lan.unpairTitle", { name: unpairing.name })}
          body={t("sync.lan.unpairBody")}
          confirm={t("sync.lan.unpair")}
          onConfirm={() =>
            void dispatch({ command: "sync_lan_remove_peer", peer_id: unpairing.peer_id })
          }
          onClose={close}
        />
      )}
    </SettingsSection>
  );
}

function LanClient({ space, hubName }: { space: SyncSpaceView; hubName: string }) {
  const t = useT();
  const dispatch = useDispatch();
  const [confirming, setConfirming] = useState(false);
  return (
    <SettingsSection
      title={t("sync.lan.title")}
      description={t("sync.lan.clientBody", { hub: hubName })}
      data-testid="sync-lan">
      <Button
        variant="text-danger"
        className="self-start"
        onClick={() => setConfirming(true)}
        data-testid="sync-lan-disable">
        {t("sync.lan.disable")}
      </Button>
      {confirming && (
        <Confirm
          title={t("sync.lan.disableTitle")}
          body={t(
            space.storage === null ? "sync.lan.disableClientOnly" : "sync.lan.disableClient",
            { hub: hubName },
          )}
          confirm={t("sync.lan.disable")}
          onConfirm={() => void dispatch({ command: "sync_lan_disable" })}
          onClose={() => setConfirming(false)}
        />
      )}
    </SettingsSection>
  );
}

function LanOff({ space }: { space: SyncSpaceView }) {
  const t = useT();
  const { backend } = useBackend();
  const keepRunning = useKeepRunning();
  const [open, setOpen] = useState<"enable" | "connect" | null>(null);
  return (
    <SettingsSection
      title={t("sync.lan.title")}
      description={t("sync.lan.offBody")}
      data-testid="sync-lan">
      {open === "connect" ? (
        <ConnectForm space={space} onCancel={() => setOpen(null)} />
      ) : (
        <div className="flex flex-wrap gap-2">
          <Button icon="monitor" onClick={() => setOpen("enable")} data-testid="sync-lan-enable">
            {t("sync.lan.enable")}
          </Button>
          <Button
            variant="ghost"
            icon="link"
            onClick={() => setOpen("connect")}
            data-testid="sync-lan-connect">
            {t("sync.lan.connect")}
          </Button>
        </div>
      )}
      {open === "enable" && (
        <PresenceDialog
          title={t("sync.lan.title")}
          prompt={t("sync.lan.enablePrompt")}
          promptBiometric={t("sync.lan.enablePromptBiometric")}
          reason={t("sync.lan.enableReason")}
          submitLabel={t("sync.lan.enableSubmit")}
          onConfirm={async (presence) => {
            await backend.dispatch({ command: "sync_lan_enable", ...presence });
            keepRunning();
            setOpen(null);
          }}
          onClose={() => setOpen(null)}
        />
      )}
    </SettingsSection>
  );
}

/** The local network part of a space. */
export function LanSection({ space }: { space: SyncSpaceView }) {
  const { lan } = space;
  if (lan === null) return <LanOff space={space} />;
  if (lan.role === "client") return <LanClient space={space} hubName={lan.hub_name} />;
  return <LanHub space={space} hub={lan} />;
}
