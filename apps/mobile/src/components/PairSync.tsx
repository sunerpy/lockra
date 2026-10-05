// Pairing the phone with a computer's LAN hub: the pairing code it shows, scanned with the camera
// (its text goes from the camera to the core and never through here) or pasted. On the welcome
// screen (`newVault`) the master password is the new vault's, typed twice; with a vault, its own
// master password is checked. While the computer's user decides, the code to compare shows.
import { errorText, groupCode, isPairOffer, passwordLongEnough } from "@lockra/shared";
import {
  Button,
  Input,
  PasswordField,
  Segmented,
  Textarea,
  useBackend,
  useSubmit,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useEffect, useRef, useState } from "react";
import { overPhoneScreen } from "../app/phone-screen";

type Mode = "scan" | "paste";

/** The code this phone shows while the computer's user decides. */
function JoiningCard() {
  const t = useT();
  const { sync } = useUiState();
  if (sync.joining === null) return null;
  return (
    <div
      className="flex flex-col gap-2 rounded-14 bg-inset p-4 hairline"
      role="status"
      data-testid="sync-lan-joining">
      <p className="text-[15px] text-fg">
        {t("sync.lan.joining.title", { hub: sync.joining.hub_name })}
      </p>
      <p className="text-[13px] text-fg-muted">{t("sync.lan.joining.body")}</p>
      <p
        className="mono text-center text-[32px] tracking-[0.2em] text-fg"
        data-testid="sync-lan-joining-code">
        {groupCode(sync.joining.code)}
      </p>
    </div>
  );
}

export function PairSync({
  newVault = false,
  onPaired,
}: {
  newVault?: boolean;
  onPaired?: () => void;
}) {
  const t = useT();
  const { backend } = useBackend();
  const { sync } = useUiState();
  const [mode, setMode] = useState<Mode>("scan");
  const [text, setText] = useState("");
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [deviceName, setDeviceName] = useState(() => t("sync.platformDevice.android"));
  const submit = useSubmit();
  // The camera is gone once the request reached the computer: from then, leaving the app locks
  // the vault again while the pairing waits for the answer.
  const cameraClosed = useRef<(() => void) | undefined>(undefined);
  const asking = sync.joining !== null;
  useEffect(() => {
    if (asking) cameraClosed.current?.();
  }, [asking]);
  const mismatch = newVault && repeat !== "" && repeat !== password;
  const passwordReady = newVault
    ? passwordLongEnough(password) && repeat === password
    : password !== "";
  const ready = passwordReady && (mode === "scan" || isPairOffer(text));
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const paired = await submit.run(() =>
      mode === "scan"
        ? overPhoneScreen((closed) => {
            cameraClosed.current = closed;
            return backend.scanPair(
              { prompt: t("mobile.sync.pairPrompt"), cancel: t("common.cancel") },
              { password, deviceName },
            );
          })
        : backend
            .dispatch({
              command: "sync_lan_join",
              text: text.trim(),
              password,
              device_name: deviceName,
            })
            .then(() => true),
    );
    cameraClosed.current = undefined;
    // Left without a code, nothing was tried: the passwords stay.
    if (paired === false) return;
    setPassword("");
    setRepeat("");
    if (paired === true) onPaired?.();
  };
  return (
    <form
      onSubmit={(e) => void onSubmit(e)}
      className="flex flex-col gap-4"
      data-testid="sync-pair">
      <Segmented
        size="lg"
        label={t("mobile.sync.pairTitle")}
        value={mode}
        onChange={setMode}
        options={[
          { value: "scan", label: t("mobile.sync.fromScan") },
          { value: "paste", label: t("mobile.sync.fromPaste") },
        ]}
        className="self-start"
      />
      {mode === "paste" && (
        <Textarea
          label={t("sync.lan.offer.text")}
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="lockra-pair:1:…"
          mono
          rows={4}
          spellCheck={false}
        />
      )}
      <Input
        size="lg"
        label={t("sync.deviceName")}
        value={deviceName}
        onChange={(e) => setDeviceName(e.target.value)}
        help={t("sync.deviceNameHint")}
        maxLength={64}
      />
      {newVault ? (
        <>
          <PasswordField
            size="lg"
            label={t("welcome.create.password")}
            value={password}
            onChange={setPassword}
            strength
            help={t("welcome.create.hint")}
            autoComplete="new-password"
          />
          <PasswordField
            size="lg"
            label={t("welcome.create.repeat")}
            value={repeat}
            onChange={setRepeat}
            autoComplete="new-password"
            error={mismatch ? t("welcome.create.mismatch") : undefined}
          />
          <p className="text-[13px] text-fg-muted">{t("mobile.sync.pairNewVault")}</p>
        </>
      ) : (
        <PasswordField
          size="lg"
          label={t("sync.join.vaultPassword")}
          value={password}
          onChange={setPassword}
          help={t("sync.join.vaultPasswordHint")}
          autoComplete="current-password"
        />
      )}
      <JoiningCard />
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
        {t(mode === "scan" ? "mobile.sync.pairScan" : "mobile.sync.pairSubmit")}
      </Button>
    </form>
  );
}
