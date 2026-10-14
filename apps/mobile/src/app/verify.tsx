// Proving it is the user before a login of the 「填充前验证」 level copies or shows a secret (board
// 29), in a sheet from the bottom: the fingerprint that unlocks this vault (asked by itself where
// it is the default unlock, as the lock screen does) or the master password. The sheet runs the
// command itself, so a wrong password stays under its field and the sheet stays up.
import { errorText } from "@lockra/shared";
import {
  Button,
  type LoginActions,
  PasswordField,
  type Proof,
  type Verify,
  type VerifyAction,
  useLoginActions,
  useSubmit,
  useT,
} from "@lockra/ui";
import {
  type ReactNode,
  type SubmitEvent,
  createContext,
  useCallback,
  useContext,
  useRef,
  useState,
} from "react";
import { BottomSheet } from "../components/BottomSheet";
import { shown, useAutoFingerprint } from "./presence";

interface Request {
  action: VerifyAction;
  name: string;
  /** Runs the command with `proof`; its answer settles the verification. Its error is thrown, for
   *  the sheet to show. */
  attempt: (proof: Proof) => Promise<void>;
  cancel: () => void;
}

const VerifyContext = createContext<Verify | undefined>(undefined);

/** Holds the one verification sheet of the unlocked app. */
export function VerifyProvider({ children }: { children: ReactNode }) {
  const [request, setRequest] = useState<Request | null>(null);
  const pending = useRef<Request | null>(null);
  const verify = useCallback<Verify>(
    (action, name, run) =>
      new Promise((resolve) => {
        // A second request while one is up: the first is cancelled.
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
      {request !== null && (
        <VerifySheet key={`${request.action}:${request.name}`} request={request} />
      )}
    </VerifyContext.Provider>
  );
}

export function useVerify(): Verify {
  const verify = useContext(VerifyContext);
  if (verify === undefined) throw new Error("useVerify outside <VerifyProvider>");
  return verify;
}

/** Copying and showing a login's parts, the proof from the sheet. */
export function useItemActions(): LoginActions {
  return useLoginActions(useVerify());
}

function VerifySheet({ request }: { request: Request }) {
  const t = useT();
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const action = t(`item.verify.action.${request.action}`);
  const ask = async (proof: Proof) => {
    await submit.run(() => request.attempt(proof));
    setPassword("");
  };
  const reason = t("item.verify.reason", { action, name: request.name });
  const fingerprint = useAutoFingerprint(() => ask({ reason }));
  const failure = shown(submit.error);
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    // Without the password, the fingerprint (as the phone's other checks do).
    if (password !== "") await ask({ password });
    else if (fingerprint !== null) await ask({ reason });
  };
  return (
    <BottomSheet title={t("item.verify.title")} onClose={request.cancel} data-testid="verify-sheet">
      <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4 px-2 pb-2">
        <p className="text-[14px] text-fg">
          {t(fingerprint === null ? "item.verify.promptPassword" : "item.verify.prompt", {
            action,
            name: request.name,
          })}
        </p>
        {fingerprint !== null && (
          <Button
            variant="outline"
            size="lg"
            icon="fingerprint"
            loading={submit.busy}
            onClick={() => void ask({ reason })}
            data-testid="verify-fingerprint">
            {t("mobile.verifyWith")}
          </Button>
        )}
        <PasswordField
          size="lg"
          label={t("unlock.password")}
          value={password}
          onChange={setPassword}
          autoComplete="current-password"
          help={fingerprint === null ? undefined : t("mobile.passwordOrFingerprint")}
          error={failure === undefined ? undefined : errorText(t, failure)}
        />
        <Button
          variant="primary"
          size="lg"
          type="submit"
          loading={submit.busy}
          disabled={password === "" && fingerprint === null}
          data-testid="verify-submit">
          {action}
        </Button>
      </form>
    </BottomSheet>
  );
}
