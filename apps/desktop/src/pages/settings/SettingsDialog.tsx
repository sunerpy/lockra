// Settings as a modal over the app (Voltip's layout): the group list on the left, the group's
// pane on the right. Esc and the scrim close it; ↑ ↓ move between groups.
import { IconButton, Keycap, useT, useUiState } from "@lockra/ui";
import { useEffect } from "react";
import { type SettingsSection, useShell } from "../../app/shell-state";
import { About } from "./About";
import { Appearance } from "./Appearance";
import { General } from "./General";
import { Security } from "./Security";
import { Sync } from "./Sync";

export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  "general",
  "appearance",
  "security",
  "sync",
  "about",
];

const TITLE_ID = "lk-settings-title";

function tabId(section: SettingsSection): string {
  return `lk-settings-tab-${section}`;
}

export function SettingsDialog({ section }: { section: SettingsSection }) {
  const t = useT();
  const { app_version: version } = useUiState();
  const { open, close } = useShell();
  const show = (next: SettingsSection) => open({ type: "settings", section: next });

  // Opening the dialog, and moving between groups, puts the focus on the group's tab.
  useEffect(() => {
    document.getElementById(tabId(section))?.focus();
  }, [section]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // A dialog over this one handles its own Esc first and marks it.
      if (e.key !== "Escape" || e.defaultPrevented) return;
      e.stopPropagation();
      close();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [close]);

  const move = (delta: number) => {
    const at = SETTINGS_SECTIONS.indexOf(section);
    const next =
      SETTINGS_SECTIONS[(at + delta + SETTINGS_SECTIONS.length) % SETTINGS_SECTIONS.length];
    if (next !== undefined) show(next);
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center scrim"
      onClick={close}
      data-testid="settings-scrim">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={TITLE_ID}
        onClick={(e) => e.stopPropagation()}
        className="flex h-[min(640px,calc(100vh-48px))] w-[min(1040px,calc(100vw-48px))] overflow-hidden rounded-14 bg-surface hairline shadow-win">
        <nav className="flex w-[200px] shrink-0 flex-col border-r border-border bg-nav py-3">
          <div className="px-4 pb-3">
            <h2 id={TITLE_ID} className="text-[14px] font-semibold text-fg">
              {t("settings.title")}
            </h2>
          </div>
          <ul
            role="tablist"
            aria-label={t("settings.groupsLabel")}
            aria-orientation="vertical"
            className="flex flex-col"
            onKeyDown={(e) => {
              if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
              e.preventDefault();
              move(e.key === "ArrowDown" ? 1 : -1);
            }}>
            {SETTINGS_SECTIONS.map((id) => {
              const active = id === section;
              return (
                <li key={id}>
                  <button
                    id={tabId(id)}
                    type="button"
                    role="tab"
                    aria-selected={active}
                    tabIndex={active ? 0 : -1}
                    onClick={() => show(id)}
                    className={`flex h-9 w-full items-center px-4 text-[14px] outline-none hover:bg-nav-active focus-visible:bg-nav-active ${
                      active
                        ? "bg-nav-active font-semibold text-fg shadow-[inset_3px_0_0_var(--primary)]"
                        : "text-fg"
                    }`}>
                    {t(`settings.section.${id}`)}
                  </button>
                </li>
              );
            })}
          </ul>
          <div
            className="mono mt-auto px-4 text-[11px] text-fg-subtle"
            data-testid="settings-version">
            Lockra {version}
          </div>
        </nav>
        <div className="flex min-w-0 flex-1 flex-col">
          <header className="flex h-12 shrink-0 items-center gap-4 border-b border-border px-6">
            <span className="text-[16px] font-semibold text-fg">
              {t(`settings.section.${section}`)}
            </span>
            <span className="mono ml-auto flex items-center gap-1.5 text-[10px] text-fg-subtle">
              <Keycap>Esc</Keycap> {t("common.close")}
            </span>
            <IconButton icon="close" label={t("common.close")} onClick={close} />
          </header>
          {/* One scroll area per group (the key): a group always opens at its top. */}
          <div
            key={section}
            role="tabpanel"
            aria-labelledby={tabId(section)}
            className="min-h-0 flex-1 overflow-auto p-6"
            data-testid="settings-content"
            data-section={section}>
            {section === "general" && <General />}
            {section === "appearance" && <Appearance />}
            {section === "security" && <Security />}
            {section === "sync" && <Sync />}
            {section === "about" && <About />}
          </div>
        </div>
      </div>
    </div>
  );
}
