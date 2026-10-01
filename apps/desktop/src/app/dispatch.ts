// Run a core command from an event handler: the error becomes a toast unless the caller handles it.
import {
  type CommandName,
  type CommandOf,
  type ErrorCode,
  type ResultOf,
  isLockraError,
} from "@lockra/shared";
import { useBackend } from "@lockra/ui";
import { useCallback, useState } from "react";
import { useToaster } from "./notices";

export function useDispatch(): <C extends CommandName>(
  command: CommandOf<C>,
) => Promise<ResultOf<C> | undefined> {
  const { backend } = useBackend();
  const toaster = useToaster();
  return useCallback(
    async <C extends CommandName>(command: CommandOf<C>) => {
      try {
        return await backend.dispatch(command);
      } catch (error: unknown) {
        toaster.error(error);
        return undefined;
      }
    },
    [backend, toaster],
  );
}

/** Run any other backend call (a native dialog) from an event handler: a failure becomes a toast
 *  and the call resolves `undefined`. */
export function useGuarded(): <T>(work: () => Promise<T>) => Promise<T | undefined> {
  const toaster = useToaster();
  return useCallback(
    async <T>(work: () => Promise<T>) => {
      try {
        return await work();
      } catch (error: unknown) {
        toaster.error(error);
        return undefined;
      }
    },
    [toaster],
  );
}

export interface Submit {
  /** The work is running (the submit button shows its spinner). */
  busy: boolean;
  /** What the last run failed with, shown in place by the form. */
  error: ErrorCode | undefined;
  setError: (code: ErrorCode | undefined) => void;
  /** Run `work`; its answer, or `undefined` when it failed (no command answers `undefined`). */
  run: <T>(work: () => Promise<T>) => Promise<T | undefined>;
}

/** A form's submit: the failure stays in the form (a wrong password under its field) instead of
 *  becoming a toast. */
export function useSubmit(): Submit {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ErrorCode | undefined>(undefined);
  const run = useCallback(async <T>(work: () => Promise<T>) => {
    setBusy(true);
    setError(undefined);
    try {
      return await work();
    } catch (failure: unknown) {
      setError(isLockraError(failure) ? failure.code : "internal");
      return undefined;
    } finally {
      setBusy(false);
    }
  }, []);
  return { busy, error, setError, run };
}
