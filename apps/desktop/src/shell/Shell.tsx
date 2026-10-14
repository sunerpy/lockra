import { syncSummary } from "@lockra/shared";
import {
  CommandPalette,
  Icon,
  IconButton,
  SIDEBAR_RAIL_WIDTH,
  SIDEBAR_WIDTH,
  Sidebar,
  SidebarButton,
  SidebarButtons,
  type SidebarGroup,
  SidebarStatus,
  type SidebarStatusProps,
  ThemeSwitch,
  type ThemeChoice,
  noteUserLock,
  useBackend,
  useClock,
  useItems,
  useT,
  useUiState,
} from "@lockra/ui";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useActivityPing } from "../app/activity";
import { useCommands } from "../app/commands";
import { useDispatch } from "../app/dispatch";
import { useFileDrag } from "../app/drag";
import { type PageId, ShellStateProvider, isPageId, useShell } from "../app/shell-state";
import { useShortcuts } from "../app/shortcuts";
import { BrowserRequests } from "../features/browser/BrowserRequests";
import { EntryDialogs } from "../features/entries/EntryDialogs";
import { ExportViewer } from "../features/export/ExportViewer";
import { ItemDialogs } from "../features/items/ItemDialogs";
import { VerifyProvider } from "../features/items/verify";
import { UpdateBadge, UpdateDialog } from "../features/update/UpdateDialog";
import { Backup } from "../pages/Backup";
import { Codes } from "../pages/Codes";
import { Export } from "../pages/Export";
import { Import } from "../pages/Import";
import { Passwords } from "../pages/Passwords";
import { SettingsDialog } from "../pages/settings/SettingsDialog";
import { WindowFrame } from "./WindowFrame";

const COLLAPSE_KEY = "lockra.sidebar.collapsed";

export function Shell({ initialPage }: { initialPage?: PageId }) {
  return (
    <ShellStateProvider initialPage={initialPage}>
      <VerifyProvider>
        <ShellLayout />
      </VerifyProvider>
    </ShellStateProvider>
  );
}

function ShellLayout() {
  const t = useT();
  const state = useUiState();
  const { backend } = useBackend();
  const shell = useShell();
  const dispatch = useDispatch();
  const now = useClock();
  const dragging = useFileDrag();
  const passwords = state.features.passwords;
  const logins = useItems();
  // Without password management (a release build, for now) the page is not offered.
  const page = shell.page === "passwords" && !passwords ? "codes" : shell.page;
  const [collapsed, setCollapsed] = useState(
    () => globalThis.localStorage?.getItem(COLLAPSE_KEY) === "1",
  );
  useActivityPing(backend, true);

  // A drop or a restore that fills the import preview takes the user there.
  const importing = state.import !== null;
  const wasImporting = useRef(importing);
  useEffect(() => {
    if (importing && !wasImporting.current) shell.navigate("import");
    wasImporting.current = importing;
  }, [importing, shell]);

  const lock = useCallback(() => {
    // The user's own lock: the lock screen does not ask for Touch ID until they come back.
    noteUserLock();
    void dispatch({ command: "vault_lock" });
  }, [dispatch]);
  const handlers = useMemo(
    () => ({
      palette: () => shell.setPaletteOpen(true),
      // Ctrl N adds what the page lists: a login on the passwords page, an account elsewhere.
      add: () => shell.open(page === "passwords" ? { type: "item_new" } : { type: "add_manual" }),
      generate: () => {
        if (passwords) shell.open({ type: "generator" });
      },
      lock,
      settings: () => shell.open({ type: "settings", section: "general" }),
      search: shell.focusSearch,
    }),
    [shell, lock, page, passwords],
  );
  useShortcuts(handlers, shell.overlay === null && !shell.paletteOpen);
  const commands = useCommands();

  const groups: SidebarGroup[] = [
    {
      title: t("shell.group.vault"),
      items: [
        { id: "codes", label: t("shell.nav.codes"), icon: "key", count: state.entries.length },
        ...(passwords
          ? [
              {
                id: "passwords",
                label: t("shell.nav.passwords"),
                icon: "lock" as const,
                count: logins?.length,
              },
            ]
          : []),
      ],
    },
    {
      title: t("shell.group.transfer"),
      items: [
        {
          id: "import",
          label: t("shell.nav.import"),
          icon: "download",
          count: state.import?.candidates.length || undefined,
        },
        { id: "export", label: t("shell.nav.export"), icon: "upload" },
      ],
    },
    {
      title: t("shell.group.data"),
      items: [
        { id: "backup", label: t("shell.nav.backup"), icon: "archive" },
        // Not a page: it opens Settings › 同步 (board A1 lists it with the data).
        { id: "sync", label: t("shell.nav.sync"), icon: "cloud" },
      ],
    },
  ];
  const themeChoice: ThemeChoice = state.settings.follow_system_theme
    ? "system"
    : state.settings.theme;
  const setTheme = (choice: ThemeChoice) => {
    const settings =
      choice === "system"
        ? { ...state.settings, follow_system_theme: true }
        : { ...state.settings, theme: choice, follow_system_theme: false };
    void dispatch({ command: "settings_set", settings });
  };
  const toggleCollapsed = () => {
    setCollapsed((c) => {
      globalThis.localStorage?.setItem(COLLAPSE_KEY, c ? "0" : "1");
      return !c;
    });
  };
  // The sidebar's status card (board A1): the vault, when it locks by itself, and the sync.
  const statusLines: SidebarStatusProps["lines"][number][] = [];
  if (state.auto_lock_at_ms !== null) {
    const minutes = Math.max(1, Math.ceil((state.auto_lock_at_ms - now) / 60_000));
    statusLines.push({
      text: t("codes.autoLockIn", { time: t("common.minutes", { n: minutes }) }),
    });
  }
  statusLines.push({ icon: "cloud", text: syncSummary(t, state.sync.space) });
  const openSettings = () => shell.open({ type: "settings", section: "general" });
  const footer = (
    <>
      {!collapsed && (
        <SidebarStatus title={t("shell.status.unlocked")} lamp="ok" lines={statusLines} />
      )}
      <SidebarButtons collapsed={collapsed}>
        <SidebarButton
          icon="lock"
          label={t("shell.nav.lock")}
          shortcut="Ctrl L"
          onClick={lock}
          data-testid="sidebar-lock"
        />
        <ThemeSwitch value={themeChoice} onChange={setTheme} variant="button" />
        <SidebarButton
          icon="settings"
          label={t("shell.nav.settings")}
          shortcut="Ctrl ,"
          opensDialog
          onClick={openSettings}
          data-testid="sidebar-settings"
        />
      </SidebarButtons>
    </>
  );
  return (
    <div
      className="grid h-full overflow-hidden app-canvas text-fg"
      data-testid="shell"
      // One explicit `minmax(0, 1fr)` row: an implicit `auto` row grows to the sidebar's content
      // height, and a 600 px window then lost its footer below the screen (smoke run 2026-10-01).
      style={{
        // The sidebar's own width, and the inset a floating sidebar (澄空) keeps around it.
        gridTemplateColumns: `calc(${collapsed ? SIDEBAR_RAIL_WIDTH : SIDEBAR_WIDTH}px + 2 * var(--sidebar-inset)) minmax(0, 1fr)`,
        gridTemplateRows: "minmax(0, 1fr)",
      }}>
      <Sidebar
        groups={groups}
        activeId={page}
        onNavigate={(id) => {
          if (id === "sync") shell.open({ type: "settings", section: "sync" });
          else if (isPageId(id)) shell.navigate(id);
        }}
        collapsed={collapsed}
        brand={t("shell.product")}
        trafficLights={state.platform === "macos"}
        controls={
          <IconButton
            icon={collapsed ? "railExpand" : "railCollapse"}
            label={collapsed ? t("ui.sidebar.expand") : t("ui.sidebar.collapse")}
            onClick={toggleCollapsed}
          />
        }
        footer={footer}
      />
      {/* The title bar is the drag strip, the update note and the window buttons; each page
          starts with its own header (board A1). */}
      <WindowFrame
        platform={state.platform}
        right={<UpdateBadge onOpen={() => shell.setUpdateOpen(true)} />}>
        <main className="relative min-h-0 overflow-y-auto" data-testid="page-body">
          <div className="mx-auto w-full max-w-[1040px] px-6 pt-1 pb-8">
            {page === "codes" && <Codes />}
            {page === "passwords" && <Passwords />}
            {page === "import" && <Import dragging={dragging} />}
            {page === "export" && <Export />}
            {page === "backup" && <Backup />}
          </div>
          {dragging && page !== "import" && (
            <div
              className="pointer-events-none absolute inset-3 flex items-center justify-center rounded-14 border border-dashed border-accent bg-accent-soft/80 text-[14px] font-medium text-accent-text"
              data-testid="drop-overlay">
              <Icon name="download" size={18} className="mr-2" />
              {t("import.drop")}
            </div>
          )}
        </main>
      </WindowFrame>
      <CommandPalette
        open={shell.paletteOpen}
        items={commands}
        onClose={() => shell.setPaletteOpen(false)}
        placeholder={t("ui.palette.placeholder")}
      />
      <EntryDialogs />
      <ItemDialogs />
      {shell.overlay?.type === "export" && <ExportViewer started={shell.overlay.started} />}
      {shell.overlay?.type === "settings" && <SettingsDialog section={shell.overlay.section} />}
      <UpdateDialog open={shell.updateOpen} onClose={() => shell.setUpdateOpen(false)} />
      {/* Last: an extension's request comes up over anything open. */}
      {state.browser.available && <BrowserRequests />}
    </div>
  );
}
