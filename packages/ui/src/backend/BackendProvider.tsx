import {
  type Backend,
  type CodeView,
  type ItemView,
  type Notice,
  type UiEvent,
  type UiState,
} from "@lockra/shared";
import {
  type ReactNode,
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
} from "react";

const NO_ITEMS: ItemView[] = [];

/** The logins as `items_list` last answered, asked for once per `items_rev`, whoever asks. */
class ItemsStore {
  private rev: number | null = null;
  private value: ItemView[] | undefined = undefined;
  private readonly listeners = new Set<() => void>();

  constructor(private readonly backend: Backend) {}

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  readonly snapshot = (): ItemView[] | undefined => this.value;

  /** The list of revision `rev`, asked for unless it was already; `null`: none (locked, or not
   *  offered by this build). */
  want(rev: number | null): void {
    if (rev === null) {
      this.rev = null;
      this.set(NO_ITEMS);
      return;
    }
    if (this.rev === rev) return;
    this.rev = rev;
    this.backend.dispatch({ command: "items_list" }).then(
      (list) => {
        if (this.rev === rev) this.set(list);
      },
      () => {
        if (this.rev === rev) this.set(NO_ITEMS);
      },
    );
  }

  private set(value: ItemView[]): void {
    if (this.value === value) return;
    this.value = value;
    for (const listener of this.listeners) listener();
  }
}

export interface BackendContextValue {
  backend: Backend;
  /** `undefined` until the first state arrived. */
  state: UiState | undefined;
  error: string | undefined;
  items: ItemsStore;
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

  const items = useMemo(() => new ItemsStore(backend), [backend]);
  const value = useMemo(() => ({ backend, state, error, items }), [backend, state, error, items]);
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

/** The logins, from `items_list`: asked for again whenever the state's `items_rev` moves, once
 *  per revision however many components ask. `undefined` until the first list arrives; empty
 *  while locked, or where this build does not offer password management. The last list stays
 *  shown while the next one is on its way. */
export function useItems(): ItemView[] | undefined {
  const { state, items } = useBackend();
  const rev = state?.phase === "unlocked" && state.features.passwords ? state.items_rev : null;
  const list = useSyncExternalStore(items.subscribe, items.snapshot);
  useEffect(() => {
    items.want(rev);
  }, [items, rev]);
  return state === undefined ? undefined : list;
}
