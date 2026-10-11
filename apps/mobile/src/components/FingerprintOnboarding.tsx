// The fingerprint by default. A vault made, restored or joined on this phone turns it on with one
// check, as the welcome page's switch says (on unless turned off; off counts as declining). An
// existing vault unlocked with the master password offers it once, until 「暂不」 (settings
// `biometric_offer`). Either way only where a fingerprint is enrolled and the phone can keep the
// key, and never while the fingerprint is already on.
import { errorText, isLockraError } from "@lockra/shared";
import {
  Button,
  Dialog,
  useBackend,
  useSubmit,
  useT,
  useUiState,
  useUpdateSettings,
} from "@lockra/ui";
import {
  type ReactNode,
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

interface Choice {
  /** The welcome page's switch: turn the fingerprint on once the new vault is open. */
  on: boolean;
  setOn: (on: boolean) => void;
}

const ChoiceContext = createContext<Choice | null>(null);

/** Holds the welcome page's choice past the welcome page, which goes as the vault opens. */
export function FingerprintChoiceProvider({ children }: { children: ReactNode }) {
  const [on, setOn] = useState(true);
  const choice = useMemo(() => ({ on, setOn }), [on]);
  return <ChoiceContext.Provider value={choice}>{children}</ChoiceContext.Provider>;
}

/** The welcome page's side of the choice. */
export function useNewVaultFingerprint(): Choice {
  const choice = useContext(ChoiceContext);
  if (choice === null) throw new Error("useNewVaultFingerprint outside FingerprintChoiceProvider");
  return choice;
}

/** Whether this phone could turn the fingerprint on now: one enrolled, the key keepable, not on. */
export function fingerprintOffered(lock: {
  device_unlock: {
    available: boolean;
    enabled: boolean;
    biometric: { kind: string | null; enabled: boolean };
  };
}): boolean {
  const { available, enabled, biometric } = lock.device_unlock;
  return biometric.kind === "fingerprint" && available && !(enabled && biometric.enabled);
}

export function FingerprintOnboarding() {
  const { state } = useBackend();
  return state === undefined ? null : <Onboarding />;
}

function Onboarding() {
  const t = useT();
  const { backend } = useBackend();
  const state = useUiState();
  const update = useUpdateSettings();
  const choice = useNewVaultFingerprint();
  const submit = useSubmit();
  const { phase } = state;
  // The phase of the last render: only a change to unlocked decides, so the first state does not.
  const [seen, setSeen] = useState(phase);
  const [offer, setOffer] = useState(false);
  // What the welcome page asked for the vault that just opened, numbered so it is done once.
  const [newVault, setNewVault] = useState<{ n: number; on: boolean } | null>(null);
  if (phase !== seen) {
    setSeen(phase);
    const offered = phase === "unlocked" && fingerprintOffered(state.lock);
    // Unlocked with the master password; locked again before answering, the offer waits.
    setOffer(offered && seen === "locked" && state.settings.biometric_offer);
    if (offered && seen === "no_vault") setNewVault({ n: (newVault?.n ?? 0) + 1, on: choice.on });
  }
  const done = useRef(0);
  useEffect(() => {
    if (newVault === null || done.current === newVault.n) return;
    done.current = newVault.n;
    if (newVault.on)
      void backend
        .dispatch({
          command: "device_biometric_enable",
          reason: t("settings.security.biometricReason.fingerprint"),
        })
        .catch(() => undefined);
    else update({ biometric_offer: false });
  }, [newVault, backend, t, update]);
  if (!offer) return null;
  const later = () => {
    update({ biometric_offer: false });
    setOffer(false);
  };
  const turnOn = () =>
    void submit.run(async () => {
      try {
        await backend.dispatch({
          command: "device_biometric_enable",
          reason: t("settings.security.biometricReason.fingerprint"),
        });
      } catch (failure: unknown) {
        // A cancelled check is the user's own answer for now: offered again after the next unlock.
        if (!isLockraError(failure) || failure.code !== "biometric_cancelled") throw failure;
      }
      setOffer(false);
    });
  return (
    <Dialog
      open
      title={t("mobile.fingerprint.offerTitle")}
      onClose={() => setOffer(false)}
      actions={
        <>
          <Button variant="ghost" size="lg" onClick={later}>
            {t("mobile.fingerprint.offerLater")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            icon="fingerprint"
            loading={submit.busy}
            onClick={turnOn}>
            {t("mobile.fingerprint.offerEnable")}
          </Button>
        </>
      }>
      <p>{t("mobile.fingerprint.offerBody")}</p>
      {submit.error !== undefined && (
        <p role="alert" className="text-[13px] text-danger">
          {errorText(t, submit.error)}
        </p>
      )}
    </Dialog>
  );
}
