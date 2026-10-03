// The pages of an unlocked vault, one over another: the codes at the bottom and each page opened
// on top. Every page opened adds a history entry that records how deep it is, so the phone's back
// gesture (the shell goes back in the webview's history) and the page's own back button close the
// top page the same way, and going back several entries at once closes as many pages.
import type { ExportStarted } from "@lockra/shared";
import {
  type ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

export type Route =
  | { name: "add" }
  | { name: "manual" }
  | { name: "preview" }
  | { name: "settings" }
  | { name: "password" }
  | { name: "backup" }
  | { name: "restore" }
  | { name: "export" }
  | { name: "exportView"; started: ExportStarted }
  | { name: "account"; id: string }
  | { name: "edit"; id: string }
  | { name: "reveal"; id: string };

export interface Nav {
  /** The top page; `undefined` while the codes show. */
  route: Route | undefined;
  open: (route: Route) => void;
  /** Close the top page. */
  back: () => void;
  /** Close every page: back to the codes. */
  home: () => void;
  /** Put `route` in place of the top page (its history entry stays). */
  replace: (route: Route) => void;
}

const NavContext = createContext<Nav | null>(null);

/** How deep a history entry is: 0 for the codes, the entries this app did not add included. */
export function depthOf(state: unknown): number {
  if (typeof state !== "object" || state === null || !("lockraDepth" in state)) return 0;
  return typeof state.lockraDepth === "number" ? state.lockraDepth : 0;
}

export function NavProvider({ children }: { children: ReactNode }) {
  const [stack, setStack] = useState<Route[]>([]);
  const depth = useRef(0);
  useEffect(() => {
    const onPop = (event: PopStateEvent) => {
      const reached = depthOf(event.state);
      depth.current = reached;
      setStack((pages) => pages.slice(0, reached));
    };
    window.addEventListener("popstate", onPop);
    return () => {
      window.removeEventListener("popstate", onPop);
      // The pages close with the vault: their history entries go too, so the next back gesture
      // leaves the app rather than step through pages that are gone.
      if (depth.current > 0) history.go(-depth.current);
      depth.current = 0;
    };
  }, []);
  const open = useCallback((route: Route) => {
    depth.current += 1;
    history.pushState({ lockraDepth: depth.current }, "");
    setStack((pages) => [...pages, route]);
  }, []);
  // The depth moves at once, so a second call before the history answers asks for nothing more;
  // the entry reached sets it for good (onPop).
  const back = useCallback(() => {
    if (depth.current === 0) return;
    depth.current -= 1;
    history.back();
  }, []);
  const home = useCallback(() => {
    const pages = depth.current;
    if (pages === 0) return;
    depth.current = 0;
    history.go(-pages);
  }, []);
  const replace = useCallback((route: Route) => {
    setStack((pages) => (pages.length === 0 ? pages : [...pages.slice(0, -1), route]));
  }, []);
  const nav = useMemo<Nav>(
    () => ({ route: stack.at(-1), open, back, home, replace }),
    [stack, open, back, home, replace],
  );
  return <NavContext.Provider value={nav}>{children}</NavContext.Provider>;
}

export function useNav(): Nav {
  const nav = useContext(NavContext);
  if (nav === null) throw new Error("useNav outside <NavProvider>");
  return nav;
}
