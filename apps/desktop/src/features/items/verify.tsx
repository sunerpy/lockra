// Proving it is the user before a login of the 「填充前验证」 level shows or copies a secret, and
// before the generator's history opens (board 12): the system check (Touch ID, Windows Hello) where
// it unlocks this vault, or the master password. The dialog runs the command itself, so a wrong
// password stays under its field and the dialog stays open.
import { errorText } from "@lockra/shared";
import {
  Button,
  Dialog,
  PasswordField,
  type Proof,
  type Verify,
  type VerifyAction,
  biometricFellThrough,
  unlockBiometric,
  useT,
  useUiState,
} from "@lockra/ui";
import {
  type ReactNode,
  type SubmitEvent,
  createContext,
  useCallback,
  useContext,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { useSubmit } from "../../app/dispatch";

interface Request {
  action: VerifyAction;
  name: string;
  /** Runs the command with `proof`; its answer settles the verification. Its error is thrown, for
   *  the dialog to show. */
  attempt: (proof: Proof) => Promise<void>;
  cancel: () => void;
}

const VerifyContext = createContext<Verify | undefined>(undefined);

/** Holds the one verification dialog of the window. */
export function VerifyProvider({ children }: { children: ReactNode }) {
  const [request, setRequest] = useState<Request | null>(null);
  const pending = useRef<Request | null>(null);
  const verify = useCallback<Verify>(
    (action, name, run) =>
      new Promise((resolve) => {
        // A second request while one is open: the first is cancelled.
        pending.current?.cancel();
        const finish = () => {
          pending.current = null;
          setRequest(null);
        };
        const next: Request = {
          action,
          name,
          attempt: (proof) =>
            run(proof).then((answer) => {
              finish();
              resolve(answer);
            }),
          cancel: () => {
            finish();
            resolve(undefined);
          },
        };
        pending.current = next;
        setRequest(next);
      }),
    [],
  );
  return (
    <VerifyContext.Provider value={verify}>
      {children}
      {request !== null && <VerifyDialog request={request} />}
    </VerifyContext.Provider>
  );
}

/** Ask the user to prove it is them, then run `run` with the proof; its answer, or `undefined`
 *  when the user cancelled. */
export function useVerify(): Verify {
  const verify = useContext(VerifyContext);
  if (verify === undefined) throw new Error("useVerify outside <VerifyProvider>");
  return verify;
}

function VerifyDialog({ request }: { request: Request }) {
  const t = useT();
  const { lock } = useUiState();
  const biometric = unlockBiometric(lock);
  const formId = useId();
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const action = t(`item.verify.action.${request.action}`);
  // After the system check failed or was cancelled, the master password is what is left.
  const offerBiometric = biometric !== null && !biometricFellThrough(submit.error);
  const ask = async (proof: Proof) => {
    await submit.run(() => request.attempt(proof));
    setPassword("");
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    await ask({ password });
  };
  const reason = useMemo(
    () => t("item.verify.reason", { action, name: request.name }),
    [t, action, request.name],
  );
  return (
    <Dialog
      open
      title={t("item.verify.title")}
      onClose={request.cancel}
      width={440}
      actions={
        <>
          <Button variant="ghost" onClick={request.cancel}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            type="submit"
            form={formId}
            loading={submit.busy}
            disabled={password === ""}
            data-testid="verify-submit">
            {action}
          </Button>
        </>
      }>
      <form
        id={formId}
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-3"
        data-testid="verify-dialog">
        <p>
          {t(offerBiometric ? "item.verify.prompt" : "item.verify.promptPassword", {
            action,
            name: request.name,
          })}
        </p>
        {offerBiometric && (
          <Button
            variant="outline"
            icon="fingerprint"
            className="self-start"
            loading={submit.busy}
            onClick={() => void ask({ reason })}
            data-testid="verify-biometric">
            {t(`sync.invite.verifyWith.${biometric}`)}
          </Button>
        )}
        <PasswordField
          label={t("sync.masterPassword")}
          value={password}
          onChange={setPassword}
          autoComplete="current-password"
          error={
            submit.error === undefined || biometricFellThrough(submit.error)
              ? undefined
              : errorText(t, submit.error)
          }
          data-autofocus
        />
      </form>
    </Dialog>
  );
}
