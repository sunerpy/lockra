import { relativeTime } from "@lockra/shared";
import {
  CommandPalette,
  Icon,
  IconButton,
  Keycaps,
  SIDEBAR_RAIL_WIDTH,
  SIDEBAR_WIDTH,
  Sidebar,
  SidebarEntry,
  type SidebarGroup,
  ThemeSwitch,
  type ThemeChoice,
  type ToolbarReadout,
  cx,
  useBackend,
  useClock,
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
import { EntryDialogs } from "../features/entries/EntryDialogs";
import { ExportViewer } from "../features/export/ExportViewer";
import { Backup } from "../pages/Backup";
import { Codes } from "../pages/Codes";
import { Export } from "../pages/Export";
import { Import } from "../pages/Import";
import { SettingsDialog } from "../pages/settings/SettingsDialog";
import { WindowFrame } from "./WindowFrame";

const COLLAPSE_KEY = "lockra.sidebar.collapsed";

export function Shell({ initialPage }: { initialPage?: PageId }) {
  return (
    <ShellStateProvider initialPage={initialPage}>
      <ShellLayout />
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
    void dispatch({ command: "vault_lock" });
  }, [dispatch]);
  const handlers = useMemo(
    () => ({
      palette: () => shell.setPaletteOpen(true),
      add: () => shell.open({ type: "add_manual" }),
      lock,
      settings: () => shell.open({ type: "settings", section: "general" }),
      search: shell.focusSearch,
    }),
    [shell, lock],
  );
  useShortcuts(handlers, shell.overlay === null && !shell.paletteOpen);
  const commands = useCommands();

  const groups: SidebarGroup[] = [
    {
      title: t("shell.group.vault"),
      items: [
        { id: "codes", label: t("shell.nav.codes"), icon: "key", count: state.entries.length },
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
      items: [{ id: "backup", label: t("shell.nav.backup"), icon: "archive" }],
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
  const readouts: ToolbarReadout[] = [
    {
      label: t("ui.sidebar.coreStatus"),
      value: `${t("shell.status.unlocked")} · ${t("common.accounts", { n: state.entries.length })}`,
      lamp: "ok",
    },
  ];
  if (state.auto_lock_at_ms !== null) {
    const minutes = Math.max(1, Math.ceil((state.auto_lock_at_ms - now) / 60_000));
    readouts.push({
      label: t("settings.security.autoLock"),
      value: t("codes.autoLockIn", { time: t("common.minutes", { n: minutes }) }),
    });
  }
  const pageTitle = t(`shell.nav.${shell.page}`);
  const footer = (
    <>
      <SidebarEntry
        icon="lock"
        label={t("shell.nav.lock")}
        onClick={lock}
        collapsed={collapsed}
        data-testid="sidebar-lock"
      />
      <ThemeSwitch value={themeChoice} onChange={setTheme} collapsed={collapsed} />
      <SidebarEntry
        icon="settings"
        label={t("shell.nav.settings")}
        onClick={() => shell.open({ type: "settings", section: "general" })}
        collapsed={collapsed}
        opensDialog
      />
    </>
  );
  return (
    <div
      className="grid h-full overflow-hidden bg-canvas text-fg"
      data-testid="shell"
      // One explicit `minmax(0, 1fr)` row: an implicit `auto` row grows to the sidebar's content
      // height, and a 600 px window then lost its footer below the screen (smoke run 2026-10-01).
      style={{
        gridTemplateColumns: `${collapsed ? SIDEBAR_RAIL_WIDTH : SIDEBAR_WIDTH}px minmax(0, 1fr)`,
        gridTemplateRows: "minmax(0, 1fr)",
      }}>
      <Sidebar
        groups={groups}
        activeId={shell.page}
        onNavigate={(id) => {
          if (isPageId(id)) shell.navigate(id);
        }}
        statusTone="ok"
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
      <WindowFrame
        title={pageTitle}
        platform={state.platform}
        readouts={readouts}
        onSearch={() => shell.setPaletteOpen(true)}>
        <div className="grid min-h-0 grid-rows-[minmax(0,1fr)_auto]">
          <main className="relative min-h-0 overflow-y-auto" data-testid="page-body">
            <div className="mx-auto w-full max-w-[1040px] px-6 py-5">
              {shell.page === "codes" && <Codes />}
              {shell.page === "import" && <Import dragging={dragging} />}
              {shell.page === "export" && <Export />}
              {shell.page === "backup" && <Backup />}
            </div>
            {dragging && shell.page !== "import" && (
              <div
                className="pointer-events-none absolute inset-3 flex items-center justify-center rounded-14 border border-dashed border-accent bg-accent-soft/80 text-[14px] font-medium text-accent-text"
                data-testid="drop-overlay">
                <Icon name="download" size={18} className="mr-2" />
                {t("import.drop")}
              </div>
            )}
          </main>
          <footer className="mono flex h-7 shrink-0 items-center gap-1 overflow-hidden border-t border-border px-6 text-[11px] whitespace-nowrap text-fg-subtle">
            {(
              [
                ["Ctrl K", t("shell.footer.palette")],
                ["Ctrl N", t("shell.footer.add")],
                ["Ctrl L", t("shell.footer.lock")],
                ["Ctrl ,", t("shell.footer.settings")],
              ] as const
            ).map(([keys, label], i) => (
              <span key={keys} className="flex items-center gap-1.5">
                {i > 0 && <span className="px-1">·</span>}
                <Keycaps keys={keys} />
                <span>{label}</span>
              </span>
            ))}
            {state.backup.last_backup_ms !== null && (
              <span className={cx("ml-auto")}>
                {t("backup.manual.last", {
                  when: relativeTime(t, state.backup.last_backup_ms, now),
                })}
              </span>
            )}
          </footer>
        </div>
      </WindowFrame>
      <CommandPalette
        open={shell.paletteOpen}
        items={commands}
        onClose={() => shell.setPaletteOpen(false)}
        placeholder={t("ui.palette.placeholder")}
      />
      <EntryDialogs />
      {shell.overlay?.type === "export" && <ExportViewer started={shell.overlay.started} />}
      {shell.overlay?.type === "settings" && <SettingsDialog section={shell.overlay.section} />}
    </div>
  );
}
