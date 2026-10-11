import type { ReactNode } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Logo } from "./Logo";
import { Icon, type IconName } from "./Icon";
import { Lamp, type LampTone } from "./Lamp";

export interface SidebarItem {
  id: string;
  label: string;
  icon: IconName;
  count?: number | string;
}

export interface SidebarGroup {
  title: string;
  items: SidebarItem[];
}

/** Width of the expanded sidebar (board A1) and of the collapsed icon rail, in px. */
export const SIDEBAR_WIDTH = 232;
export const SIDEBAR_RAIL_WIDTH = 56;

/** macOS draws its native traffic lights over the window's top-left corner (`titleBarStyle:
 *  "Overlay"`, `trafficLightPosition` x 12, y 14 in tauri.macos.conf.json): three 12 px buttons
 *  20 px apart, ending 64 px from the left edge. Whatever sits next to them starts 16 px later, in
 *  px so the 字号 setting (which scales rem) cannot slide it back under them (user feedback
 *  2026-09-29: with 8 px of air the lights crowded the app mark). */
export const TRAFFIC_LIGHTS_CLEARANCE = 80;
/** The brand row's left padding beside the traffic lights: the clearance less the nav's `px-3`. */
export const TRAFFIC_LIGHTS_BRAND_INSET = "pl-[calc(80px_-_0.75rem)]";
/** The collapsed rail on macOS holds the lights with 12 px on either side; the 56 px rail is
 *  narrower than they are, and its border cut through the green button. */
export const SIDEBAR_RAIL_WIDTH_TRAFFIC_LIGHTS = 76;

/** One sidebar row, 40 px at the default 字号 (board A1 draws 44; 40 keeps the footer on a 600 px
 *  window), shared by the nav items, the footer entries and the theme switch so their glyphs and
 *  labels line up (user feedback 2026-09-28: the theme row's glyph sat 2 px and its name 10 px
 *  right of the others, with two separate hover patches). */
export const SIDEBAR_ROW_CLASS =
  "flex h-11.5 w-full items-center rounded-6 text-[14px] font-medium transition-colors";
/** An expanded row: the glyph 0.875 rem from the edge, the label 0.625 rem after the glyph. */
export const SIDEBAR_ROW_EXPANDED_CLASS = "gap-2.5 px-3.5";
/** A control that covers exactly the glyph's slot of an expanded row: the row's `px-3.5` inset
 *  (0.875 rem), the 18 px glyph and the `gap-2.5` (0.625 rem), so whatever follows it starts where
 *  a row's label does. Spacing is in rem (it follows 设置 › 外观 › 字号), the glyph in px. */
export const SIDEBAR_GLYPH_SLOT_CLASS = "w-[calc(1.5rem_+_18px)] pl-3.5";
/** The sidebar's glyphs, in px. */
export const SIDEBAR_GLYPH_SIZE = 18;

export interface SidebarProps {
  groups: readonly SidebarGroup[];
  activeId: string;
  onNavigate: (id: string) => void;
  /** Items greyed out (onboarding keeps only the home item live). */
  disabledIds?: readonly string[];
  /** A lamp after the brand (green when the core is ready); none when the footer says it. */
  statusTone?: LampTone;
  /** The 56 px icon rail: glyphs only; every entry keeps its label as its name and tooltip. */
  collapsed?: boolean;
  /** Layout controls (collapse, hide, pin): in the brand row, or stacked under it in the rail. */
  controls?: ReactNode;
  /** Entries pinned to the bottom (feedback, the theme switch, settings), drawn with
   *  {@link SidebarEntry} so they follow `collapsed` like the nav items. */
  footer?: ReactNode;
  /** Drawn over the page (the edge preview of a hidden sidebar): a shadow instead of the border. */
  floating?: boolean;
  brand?: string;
  /** macOS keeps its native traffic lights over the top-left corner (`titleBarStyle: "Overlay"`):
   *  the brand row starts {@link TRAFFIC_LIGHTS_CLEARANCE} px from the window's edge and shows the
   *  wordmark without the mark (the Dock, the menu bar and the tray already show it), and the
   *  collapsed rail widens to {@link SIDEBAR_RAIL_WIDTH_TRAFFIC_LIGHTS} px. Keep in sync with
   *  `trafficLightPosition` in tauri.macos.conf.json. */
  trafficLights?: boolean;
  className?: string;
}

export interface SidebarEntryProps {
  icon: IconName;
  label: string;
  onClick: () => void;
  collapsed?: boolean;
  /** The current page (nav items only): `aria-current="page"` and the active surface. */
  current?: boolean;
  disabled?: boolean;
  /** Right-aligned text while expanded (a count). */
  trailing?: ReactNode;
  /** The entry opens a dialog rather than navigating (settings, feedback). */
  opensDialog?: boolean;
  "data-testid"?: string;
}

/** One sidebar row: a glyph and, while expanded, the label. Collapsed, the label stays as the
 *  accessible name and the tooltip, since nothing else on screen says what the glyph is. */
export function SidebarEntry({
  icon,
  label,
  onClick,
  collapsed = false,
  current = false,
  disabled = false,
  trailing,
  opensDialog = false,
  "data-testid": testId,
}: SidebarEntryProps) {
  return (
    <button
      type="button"
      aria-current={current ? "page" : undefined}
      aria-haspopup={opensDialog ? "dialog" : undefined}
      aria-label={collapsed ? label : undefined}
      title={collapsed ? label : undefined}
      disabled={disabled}
      onClick={onClick}
      data-testid={testId}
      className={cx(
        SIDEBAR_ROW_CLASS,
        collapsed ? "justify-center" : SIDEBAR_ROW_EXPANDED_CLASS,
        // The current page is raised: the theme's card shadow (澄空) on the active surface.
        current
          ? "bg-nav-active font-medium text-fg shadow-[var(--card-shadow)]"
          : "text-fg-muted hover:bg-nav-active hover:text-fg",
        disabled && "cursor-not-allowed opacity-40 hover:bg-transparent hover:text-fg-muted",
      )}>
      <Icon
        name={icon}
        size={SIDEBAR_GLYPH_SIZE}
        className={current ? "text-accent-text" : "text-fg-subtle"}
      />
      {!collapsed && (
        <>
          <span className="flex-1 truncate text-left">{label}</span>
          {trailing !== undefined && (
            <span className="inline-flex h-5 min-w-7 items-center justify-center rounded-pill bg-accent-soft px-2 text-[12px] font-semibold text-accent-text tabular-nums">
              {trailing}
            </span>
          )}
        </>
      )}
    </button>
  );
}

/** The nav rail: a 40 px brand row (part of the window drag strip) with the layout controls,
 *  the grouped items and the footer pinned to the bottom. {@link SIDEBAR_WIDTH} px wide, or the
 *  56 px icon rail when `collapsed`. */
export function Sidebar({
  groups,
  activeId,
  onNavigate,
  disabledIds = [],
  statusTone,
  collapsed = false,
  controls,
  footer,
  floating = false,
  brand = "Voltip",
  trafficLights = false,
  className,
}: SidebarProps) {
  const t = useT();
  const rail = trafficLights ? SIDEBAR_RAIL_WIDTH_TRAFFIC_LIGHTS : SIDEBAR_RAIL_WIDTH;
  const width = collapsed ? rail : SIDEBAR_WIDTH;
  return (
    <nav
      aria-label={t("ui.sidebar.nav")}
      data-collapsed={collapsed}
      data-floating={floating}
      // A flex item's automatic minimum is its content: pin all three so a long label can never
      // hold the rail open wider than 56 px. 澄空 floats it inside the window: `sidebar-shell` adds
      // the theme's inset around it, and the shell's column grows by that much.
      style={{ width, minWidth: width, maxWidth: width }}
      className={cx(
        "flex shrink-0 flex-col bg-nav pb-4",
        floating && "h-full",
        collapsed ? "px-2" : "px-3",
        floating ? "shadow-win" : "sidebar-shell",
        className,
      )}>
      {/* Brand row: exactly 40 px, flush with the top edge, so it and the TitleBar form one
          continuous drag strip. `deep` lets the brand text drag too (Tauri's drag script leaves
          buttons alone); the lamp is not clickable. */}
      <div
        data-tauri-drag-region="deep"
        data-testid="sidebar-brand"
        className={cx(
          "flex h-10 shrink-0 items-center gap-2.5 select-none",
          collapsed
            ? "justify-center"
            : trafficLights
              ? cx(TRAFFIC_LIGHTS_BRAND_INSET, "pr-2")
              : "px-2",
          collapsed && trafficLights && "invisible",
        )}>
        {!trafficLights && <Logo size={28} className="shrink-0" />}
        {!collapsed && (
          <>
            <span className="text-[17px] font-semibold text-fg">{brand}</span>
            {statusTone !== undefined && (
              <Lamp tone={statusTone} size={6} label={t("ui.sidebar.coreStatus")} />
            )}
            {controls !== undefined && (
              <div
                role="group"
                aria-label={t("ui.sidebar.layout")}
                data-testid="sidebar-controls"
                className="ml-auto flex items-center gap-0.5">
                {controls}
              </div>
            )}
          </>
        )}
      </div>
      {collapsed && controls !== undefined && (
        <div
          role="group"
          aria-label={t("ui.sidebar.layout")}
          data-testid="sidebar-controls"
          className="flex flex-col items-center gap-0.5 border-b border-border pb-2">
          {controls}
        </div>
      )}
      <div
        className={cx("flex flex-1 flex-col gap-5 overflow-y-auto", collapsed ? "mt-3" : "mt-6")}>
        {groups.map((g) => (
          <div key={g.title} role="group" aria-label={g.title}>
            {collapsed ? (
              <div aria-hidden className="mx-2 mb-1 h-px bg-border first:hidden" />
            ) : (
              <div className="mb-1 px-2 text-[11px] font-medium text-fg-subtle">{g.title}</div>
            )}
            <ul className="flex flex-col gap-0.5">
              {g.items.map((item) => (
                <li key={item.id}>
                  <SidebarEntry
                    icon={item.icon}
                    label={
                      collapsed && item.count !== undefined
                        ? `${item.label} · ${item.count}`
                        : item.label
                    }
                    collapsed={collapsed}
                    current={item.id === activeId}
                    disabled={disabledIds.includes(item.id)}
                    trailing={item.count}
                    onClick={() => {
                      onNavigate(item.id);
                    }}
                  />
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
      {footer !== undefined && (
        <div data-testid="sidebar-footer" className="mt-4 flex flex-col gap-2">
          {footer}
        </div>
      )}
    </nav>
  );
}

export interface SidebarStatusProps {
  /** The first line, after the lamp: 已解锁. */
  title: string;
  lamp: LampTone;
  /** The facts under it: when the vault locks, how sync stands. */
  lines: readonly { icon?: IconName; text: string }[];
}

/** The sidebar's status card above its buttons (board A1): the lamp and the vault's state, then a
 *  line or two of facts. Only in the expanded sidebar. */
export function SidebarStatus({ title, lamp, lines }: SidebarStatusProps) {
  return (
    <div
      data-testid="sidebar-status"
      className="flex flex-col gap-2 rounded-10 bg-surface px-3.5 py-3 text-[12px] text-fg-muted hairline panel">
      <span className="flex items-center gap-2 text-[13px] font-medium text-fg">
        <Lamp tone={lamp} size={8} />
        {title}
      </span>
      {lines.map((line) => (
        <span key={line.text} className="flex min-w-0 items-center gap-2">
          {line.icon && <Icon name={line.icon} size={14} className="shrink-0 text-fg-subtle" />}
          <span className="truncate">{line.text}</span>
        </span>
      ))}
    </div>
  );
}

export interface SidebarButtonProps {
  icon: IconName;
  /** The name and the tooltip: the glyph alone says nothing. */
  label: string;
  onClick: () => void;
  /** It opens a dialog (settings). */
  opensDialog?: boolean;
  /** Its key, added to the tooltip (the shortcut footer is gone). */
  shortcut?: string;
  "data-testid"?: string;
}

/** One of the square buttons at the foot of the sidebar (board A1: lock, theme, settings), side by
 *  side in the expanded sidebar and stacked in the icon rail. */
export function SidebarButton({
  icon,
  label,
  onClick,
  opensDialog = false,
  shortcut,
  "data-testid": testId,
}: SidebarButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      title={shortcut === undefined ? label : `${label} · ${shortcut}`}
      aria-haspopup={opensDialog ? "dialog" : undefined}
      onClick={onClick}
      data-testid={testId}
      className={SIDEBAR_BUTTON_CLASS}>
      <Icon name={icon} size={SIDEBAR_GLYPH_SIZE} />
    </button>
  );
}

/** The square footer button's box, shared with the theme switch's button form. */
export const SIDEBAR_BUTTON_CLASS =
  "inline-flex h-12 min-w-0 flex-1 items-center justify-center rounded-6 bg-surface text-fg-muted outline-none hairline panel transition-colors hover:text-fg focus-visible:text-fg";

/** The footer's buttons: in a row while expanded, stacked in the rail. */
export function SidebarButtons({
  collapsed = false,
  children,
}: {
  collapsed?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      role="group"
      data-testid="sidebar-buttons"
      className={cx("flex gap-2", collapsed ? "flex-col" : "flex-row")}>
      {children}
    </div>
  );
}
