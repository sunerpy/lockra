import { AUTO_LOCK_CHOICES, CLIPBOARD_CHOICES, errorText } from "@lockra/shared";
import {
  Button,
  PasswordField,
  Select,
  SettingsPane,
  SettingsRows,
  SettingsSection,
  StatusRow,
  Toggle,
  useBackend,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useSubmit } from "../../app/dispatch";
import { useToaster } from "../../app/notices";
import { passwordLongEnough } from "../../app/password";
import { useUpdateSettings } from "../../app/settings";

/** Settings › Security: auto-lock, clipboard clearing, hidden codes, "remember on this device"
 *  with Touch ID or Windows Hello before it, and the master password. */
export function Security() {
  const t = useT();
  const { settings } = useUiState();
  const update = useUpdateSettings();
  const never = t("common.never");
  return (
    <SettingsPane title={t("settings.section.security")} lede={t("settings.security.lede")}>
      <SettingsRows>
        <StatusRow
          label={t("settings.security.autoLock")}
          help={t("settings.security.autoLockHint")}>
          <Select
            size="sm"
            aria-label={t("settings.security.autoLock")}
            value={String(settings.auto_lock_minutes)}
            onChange={(v) => update({ auto_lock_minutes: Number(v) })}
            options={AUTO_LOCK_CHOICES.map((n) => ({
              value: String(n),
              label: n === 0 ? never : t("common.minutes", { n }),
            }))}
          />
        </StatusRow>
        <StatusRow
          label={t("settings.security.clipboard")}
          help={t("settings.security.clipboardHint")}>
          <Select
            size="sm"
            aria-label={t("settings.security.clipboard")}
            value={String(settings.clipboard_clear_seconds)}
            onChange={(v) => update({ clipboard_clear_seconds: Number(v) })}
            options={CLIPBOARD_CHOICES.map((n) => ({
              value: String(n),
              label: n === 0 ? never : t("common.seconds", { n }),
            }))}
          />
        </StatusRow>
        <StatusRow
          label={t("settings.security.hideCodes")}
          help={t("settings.security.hideCodesHint")}>
          <Toggle
            checked={settings.hide_codes}
            onChange={(hide_codes) => update({ hide_codes })}
            ariaLabel={t("settings.security.hideCodes")}
          />
        </StatusRow>
        <DeviceUnlock />
        <BiometricUnlock />
      </SettingsRows>
      <ChangePassword />
    </SettingsPane>
  );
}

/** "Remember on this device": on at once; off asks for the master password (the vault's key is
 *  rotated and the keychain entry deleted). */
function DeviceUnlock() {
  const t = useT();
  const { backend } = useBackend();
  const { lock } = useUiState();
  const { available, enabled } = lock.device_unlock;
  const [confirming, setConfirming] = useState(false);
  const [password, setPassword] = useState("");
  const toggle = useSubmit();
  const disable = useSubmit();
  const onChange = (on: boolean) => {
    if (on) void toggle.run(() => backend.dispatch({ command: "device_unlock_enable" }));
    else setConfirming(true);
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    const done = await disable.run(() =>
      backend.dispatch({ command: "device_unlock_disable", password }),
    );
    setPassword("");
    if (done !== undefined) setConfirming(false);
  };
  return (
    <>
      <StatusRow
        label={t("settings.security.device")}
        help={
          available ? t("settings.security.deviceHint") : t("settings.security.deviceUnavailable")
        }
        data-testid="device-unlock"
        note={
          toggle.error === undefined ? undefined : (
            <span className="text-danger">{errorText(t, toggle.error)}</span>
          )
        }>
        <Toggle
          checked={enabled}
          disabled={!available || toggle.busy}
          onChange={onChange}
          ariaLabel={t("settings.security.device")}
        />
      </StatusRow>
      {confirming && enabled && (
        <form
          onSubmit={(e) => void onSubmit(e)}
          className="flex flex-col gap-2 border-b border-border py-3"
          data-testid="device-disable">
          <p className="text-[12px] text-fg-muted">{t("settings.security.deviceDisablePrompt")}</p>
          <div className="flex flex-wrap items-start gap-2">
            <PasswordField
              label={t("unlock.password")}
              value={password}
              onChange={setPassword}
              autoFocus
              autoComplete="current-password"
              className="min-w-[16rem] flex-1"
              error={disable.error === undefined ? undefined : errorText(t, disable.error)}
            />
            <div className="flex gap-2 pt-5">
              <Button variant="ghost" onClick={() => setConfirming(false)}>
                {t("common.cancel")}
              </Button>
              <Button
                variant="danger"
                type="submit"
                loading={disable.busy}
                disabled={password === ""}>
                {t("settings.security.deviceDisableTitle")}
              </Button>
            </div>
          </div>
        </form>
      )}
    </>
  );
}

function ChangePassword() {
  const t = useT();
  const { backend } = useBackend();
  const toaster = useToaster();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [repeat, setRepeat] = useState("");
  const submit = useSubmit();
  const mismatch = repeat !== "" && repeat !== next;
  const ready = current !== "" && passwordLongEnough(next) && repeat === next;
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    const done = await submit.run(() =>
      backend.dispatch({ command: "vault_change_password", current, new: next }),
    );
    setCurrent("");
    if (done === undefined) return;
    setNext("");
    setRepeat("");
    toaster.info(t("settings.security.changed"));
  };
  const error = submit.error === undefined ? undefined : errorText(t, submit.error);
  return (
    <SettingsSection
      title={t("settings.security.changePassword")}
      description={t("settings.security.changePasswordHint")}
      data-testid="change-password">
      <form onSubmit={(e) => void onSubmit(e)} className="grid max-w-[36rem] gap-3 sm:grid-cols-2">
        <PasswordField
          label={t("settings.security.current")}
          value={current}
          onChange={setCurrent}
          autoComplete="current-password"
          className="sm:col-span-2"
          error={submit.error === "wrong_password" ? error : undefined}
        />
        <PasswordField
          label={t("settings.security.newPassword")}
          value={next}
          onChange={setNext}
          strength
          autoComplete="new-password"
          error={
            submit.error !== undefined && submit.error !== "wrong_password" ? error : undefined
          }
        />
        <PasswordField
          label={t("settings.security.repeat")}
          value={repeat}
          onChange={setRepeat}
          autoComplete="new-password"
          error={mismatch ? t("welcome.create.mismatch") : undefined}
        />
        <Button
          variant="primary"
          type="submit"
          loading={submit.busy}
          disabled={!ready}
          className="justify-self-start">
          {t("settings.security.changePassword")}
        </Button>
      </form>
    </SettingsSection>
  );
}

/** Touch ID or Windows Hello before "remember on this device" unlocks: on after one check passes,
 *  off with the master password. Offered only where the computer has it and the vault is
 *  remembered. */
function BiometricUnlock() {
  const t = useT();
  const { backend } = useBackend();
  const { lock } = useUiState();
  const { enabled: remembered, biometric } = lock.device_unlock;
  const [confirming, setConfirming] = useState(false);
  const [password, setPassword] = useState("");
  const toggle = useSubmit();
  const disable = useSubmit();
  const kind = biometric.kind;
  if (!remembered || kind === null) return null;
  const onChange = (on: boolean) => {
    if (on)
      void toggle.run(() =>
        backend.dispatch({
          command: "device_biometric_enable",
          reason: t(`settings.security.biometricReason.${kind}`),
        }),
      );
    else setConfirming(true);
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    const done = await disable.run(() =>
      backend.dispatch({ command: "device_biometric_disable", password }),
    );
    setPassword("");
    if (done !== undefined) setConfirming(false);
  };
  return (
    <>
      <StatusRow
        label={t(`settings.security.biometric.${kind}`)}
        help={t(`settings.security.biometricHint.${kind}`)}
        data-testid="biometric-unlock"
        note={
          toggle.error === undefined || toggle.error === "biometric_cancelled" ? undefined : (
            <span className="text-danger">{errorText(t, toggle.error)}</span>
          )
        }>
        <Toggle
          checked={biometric.enabled}
          disabled={toggle.busy}
          onChange={onChange}
          ariaLabel={t(`settings.security.biometric.${kind}`)}
        />
      </StatusRow>
      {confirming && biometric.enabled && (
        <form
          onSubmit={(e) => void onSubmit(e)}
          className="flex flex-col gap-2 border-b border-border py-3"
          data-testid="biometric-disable">
          <p className="text-[12px] text-fg-muted">
            {t("settings.security.biometricDisablePrompt")}
          </p>
          <div className="flex flex-wrap items-start gap-2">
            <PasswordField
              label={t("unlock.password")}
              value={password}
              onChange={setPassword}
              autoFocus
              autoComplete="current-password"
              className="min-w-[16rem] flex-1"
              error={disable.error === undefined ? undefined : errorText(t, disable.error)}
            />
            <div className="flex gap-2 pt-5">
              <Button variant="ghost" onClick={() => setConfirming(false)}>
                {t("common.cancel")}
              </Button>
              <Button
                variant="danger"
                type="submit"
                loading={disable.busy}
                disabled={password === ""}>
                {t("settings.security.biometricDisableTitle")}
              </Button>
            </div>
          </div>
        </form>
      )}
    </>
  );
}
