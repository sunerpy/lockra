import {
  type Backend,
  type CodeView,
  type Notice,
  type UiEvent,
  type UiState,
} from "@lockra/shared";
import { type ReactNode, createContext, useContext, useEffect, useMemo, useState } from "react";

export interface BackendContextValue {
  backend: Backend;
  /** `undefined` until the first state arrived. */
  state: UiState | undefined;
  error: string | undefined;
}

const BackendContext = createContext<BackendContextValue | undefined>(undefined);

export interface BackendProviderProps {
  backend: Backend;
  children: ReactNode;
  /** Every notice, in arrival order (toasts). */
  onNotice?: (notice: Notice) => void;
}

/** Loads the state, replaces it with every `state` event and passes notices on. */
export function BackendProvider({ backend, children, onNotice }: BackendProviderProps) {
  const [state, setState] = useState<UiState | undefined>(undefined);
  const [error, setError] = useState<string | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    const off = backend.on((event: UiEvent) => {
      if (!alive) return;
      if (event.type === "state") setState(event.state);
      else onNotice?.(event.notice);
    });
    backend
      .getState()
      .then((s) => {
        if (alive) setState((prev) => prev ?? s);
      })
      .catch((e: unknown) => {
        if (alive) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      alive = false;
      off();
    };
  }, [backend, onNotice]);

  const value = useMemo(() => ({ backend, state, error }), [backend, state, error]);
  return <BackendContext.Provider value={value}>{children}</BackendContext.Provider>;
}

export function useBackend(): BackendContextValue {
  const value = useContext(BackendContext);
  if (!value) throw new Error("useBackend outside <BackendProvider>");
  return value;
}

/** The state; only call under a component that renders once the state arrived. */
export function useUiState(): UiState {
  const { state } = useBackend();
  if (!state) throw new Error("useUiState before the first state");
  return state;
}

/** The codes of the last frame, by entry id; empty until the first frame. */
export function useCodes(): ReadonlyMap<string, CodeView> {
  const { backend } = useBackend();
  const [codes, setCodes] = useState<ReadonlyMap<string, CodeView>>(new Map());
  useEffect(() => {
    let off: (() => void) | undefined;
    let disposed = false;
    void backend
      .subscribeCodes((frame) => {
        if (!disposed) setCodes(new Map(frame.codes.map((c) => [c.entry_id, c])));
      })
      .then((unsubscribe) => {
        if (disposed) unsubscribe();
        else off = unsubscribe;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      off?.();
    };
  }, [backend]);
  return codes;
}
