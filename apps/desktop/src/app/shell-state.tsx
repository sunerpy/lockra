// Where the unlocked app is: the page, the open overlay (one at a time, the command palette aside)
// and whether a file drag is over the window. Pages and the palette open overlays through this.
import type { ExportStarted } from "@lockra/shared";
import { type ReactNode, createContext, useCallback, useContext, useMemo, useState } from "react";

export const PAGES = ["codes", "import", "export", "backup"] as const;
export type PageId = (typeof PAGES)[number];

export function isPageId(value: string): value is PageId {
  return PAGES.some((page) => page === value);
}

export type SettingsSection = "general" | "appearance" | "security" | "about";

export type Overlay =
  | { type: "add_manual" }
  | { type: "add_uri" }
  | { type: "edit"; id: string }
  | { type: "delete"; id: string }
  | { type: "reveal"; id: string }
  | { type: "export"; started: ExportStarted }
  | { type: "settings"; section: SettingsSection };

export interface ShellState {
  page: PageId;
  navigate: (page: PageId) => void;
  overlay: Overlay | null;
  open: (overlay: Overlay) => void;
  close: () => void;
  paletteOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
  /** The update dialog: over any overlay (Settings opens it), opened from the title bar too. */
  updateOpen: boolean;
  setUpdateOpen: (open: boolean) => void;
  /** Bumped to ask the codes page to focus its search field. */
  searchFocus: number;
  focusSearch: () => void;
}

const ShellContext = createContext<ShellState | undefined>(undefined);

export function ShellStateProvider({
  children,
  initialPage = "codes",
}: {
  children: ReactNode;
  initialPage?: PageId;
}) {
  const [page, setPage] = useState<PageId>(initialPage);
  const [overlay, setOverlay] = useState<Overlay | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [updateOpen, setUpdateOpenState] = useState(false);
  const [searchFocus, setSearchFocus] = useState(0);
  const navigate = useCallback((next: PageId) => setPage(next), []);
  const open = useCallback((next: Overlay) => {
    setPaletteOpen(false);
    setOverlay(next);
  }, []);
  const close = useCallback(() => setOverlay(null), []);
  const setUpdateOpen = useCallback((next: boolean) => {
    if (next) setPaletteOpen(false);
    setUpdateOpenState(next);
  }, []);
  const focusSearch = useCallback(() => {
    setPage("codes");
    setSearchFocus((n) => n + 1);
  }, []);
  const value = useMemo(
    () => ({
      page,
      navigate,
      overlay,
      open,
      close,
      paletteOpen,
      setPaletteOpen,
      updateOpen,
      setUpdateOpen,
      searchFocus,
      focusSearch,
    }),
    [
      page,
      navigate,
      overlay,
      open,
      close,
      paletteOpen,
      updateOpen,
      setUpdateOpen,
      searchFocus,
      focusSearch,
    ],
  );
  return <ShellContext.Provider value={value}>{children}</ShellContext.Provider>;
}

export function useShell(): ShellState {
  const value = useContext(ShellContext);
  if (!value) throw new Error("useShell outside <ShellStateProvider>");
  return value;
}
