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
import {
  type PageId,
  ShellStateProvider,
  isPageId,
  passwordsPage,
  useShell,
} from "../app/shell-state";
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
import { Generator } from "../pages/Generator";
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
  // Without password management its pages (密码, 生成器) are not offered.
  const page = passwordsPage(shell.page) && !passwords ? "codes" : shell.page;
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
      // Ctrl G: the generator page.
      generate: () => {
        if (passwords) shell.navigate("generator");
      },
      lock,
      settings: () => shell.open({ type: "settings", section: "general" }),
      search: shell.focusSearch,
    }),
    [shell, lock, page, passwords],
  );
  useShortcuts(handlers, shell.overlay === null && !shell.paletteOpen);

  // The quick search (its global shortcut, the tray's 「快捷搜索」, `lockra-desktop --palette`): the
  // shell has brought the window up and asks for the palette, never over a dialog (the window then
  // shows what is open). When that palette closes the shell hears whether something was copied from
  // it, and after a copy the window goes back where it was.
  const quick = useRef(false);
  const blocked = useRef(false);
  const busy =
    shell.overlay !== null ||
    shell.updateOpen ||
    state.browser.pairing.length > 0 ||
    state.browser.confirms.length > 0;
  useEffect(() => {
    blocked.current = busy;
  }, [busy]);
  const { setPaletteOpen, open: openOverlay, setUpdateOpen } = shell;
  // The tray's 「设置…」 and 「检查更新…」: Settings never over a dialog (the window then shows what is
  // open), the update dialog over anything, as the title bar's badge opens it.
  useEffect(
    () =>
      backend.on((event) => {
        if (event.type !== "notice") return;
        if (event.notice.type === "open_settings" && !blocked.current)
          openOverlay({ type: "settings", section: "general" });
        else if (event.notice.type === "open_updates") setUpdateOpen(true);
      }),
    [backend, openOverlay, setUpdateOpen],
  );
  const endQuick = useCallback(
    (copied: boolean) => void backend.quickAccessDone(copied).catch(() => undefined),
    [backend],
  );
  useEffect(
    () =>
      backend.on((event) => {
        if (event.type !== "notice" || event.notice.type !== "open_palette") return;
        if (blocked.current) {
          endQuick(false);
          return;
        }
        quick.current = true;
        setPaletteOpen(true);
      }),
    [backend, setPaletteOpen, endQuick],
  );
  const onCopy = useCallback(
    (work: Promise<boolean>) => {
      const fromQuick = quick.current;
      // The palette closes next: the copy, not the close, ends this quick search.
      quick.current = false;
      if (!fromQuick) return;
      void work.then(endQuick, () => endQuick(false));
    },
    [endQuick],
  );
  const closePalette = useCallback(() => {
    setPaletteOpen(false);
    if (quick.current) {
      quick.current = false;
      endQuick(false);
    }
  }, [setPaletteOpen, endQuick]);
  // An extension's save prompt took a one-time code and the user chose 「关联验证码…」 there: the
  // shell has brought the window up, and the passwords page shows the login with its code picker
  // open (one open on another login gives way), never over a dialog (the window then shows what
  // is open; the toast says what was asked).
  const { navigate, selectItem, setCodePicker } = shell;
  useEffect(
    () =>
      backend.on((event) => {
        if (event.type !== "notice" || event.notice.type !== "browser_link_code_requested") return;
        const login = event.notice.item_id;
        if (blocked.current) return;
        closePalette();
        navigate("passwords");
        selectItem(login);
        setCodePicker(login);
      }),
    [backend, closePalette, navigate, selectItem, setCodePicker],
  );
  const commands = useCommands(onCopy);

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
        // 生成器 is part of 密码: no row of its own, so 密码 stays the current one.
        activeId={page === "generator" ? "passwords" : page}
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
            {page === "generator" && <Generator />}
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
        onClose={closePalette}
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
