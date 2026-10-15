// Where the unlocked app is: the page, the open overlay (one at a time, the command palette aside)
// and whether a file drag is over the window. Pages and the palette open overlays through this.
import type { ExportStarted } from "@lockra/shared";
import { type ReactNode, createContext, useCallback, useContext, useMemo, useState } from "react";

export const PAGES = ["codes", "passwords", "import", "export", "backup"] as const;
export type PageId = (typeof PAGES)[number];

export function isPageId(value: string): value is PageId {
  return PAGES.some((page) => page === value);
}

export type SettingsSection = "general" | "appearance" | "security" | "sync" | "browser" | "about";

export type Overlay =
  | { type: "add_manual" }
  | { type: "add_uri" }
  | { type: "edit"; id: string }
  | { type: "delete"; id: string }
  | { type: "reveal"; id: string }
  | { type: "export"; started: ExportStarted }
  | { type: "settings"; section: SettingsSection }
  // Password management (the passwords page and the palette).
  | { type: "item_new"; title?: string }
  | { type: "item_edit"; id: string }
  | { type: "item_delete"; id: string }
  | { type: "item_move"; id: string }
  | { type: "item_reveal"; id: string }
  | { type: "generator" };

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
  /** Bumped to ask the page's search field (the codes', the passwords') to take the focus. */
  searchFocus: number;
  focusSearch: () => void;
  /** The login the passwords page shows in its detail pane (a new one is selected once saved). */
  selectedItem: string | null;
  selectItem: (id: string | null) => void;
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
  const [selectedItem, selectItem] = useState<string | null>(null);
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
    // The passwords page has a search of its own; any other page goes to the codes'.
    setPage((current) => (current === "passwords" ? current : "codes"));
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
      selectedItem,
      selectItem,
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
      selectedItem,
    ],
  );
  return <ShellContext.Provider value={value}>{children}</ShellContext.Provider>;
}

export function useShell(): ShellState {
  const value = useContext(ShellContext);
  if (!value) throw new Error("useShell outside <ShellStateProvider>");
  return value;
}
