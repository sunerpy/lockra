import {
  type CSSProperties,
  type KeyboardEvent,
  type MutableRefObject,
  type ReactNode,
  type Ref,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { cx } from "../cx";
import { Icon, type IconName } from "./Icon";

/** One row of a `Menu`: a choice of a set (`radio`) or a command (`action`). */
export type MenuItem =
  | {
      kind: "radio";
      id: string;
      label: string;
      checked: boolean;
      /** The label is the user's own text (a name they typed), not interface copy. */
      userText?: boolean;
    }
  | { kind: "action"; id: string; label: string; icon?: IconName };

/** Items under an optional heading; sections are separated by a rule. */
export interface MenuSection {
  label?: string;
  items: readonly MenuItem[];
}

export interface MenuProps {
  /** What the trigger button shows. */
  trigger: ReactNode;
  /** The menu's accessible name, and the trigger's unless `triggerLabel` is given. */
  label: string;
  triggerLabel?: string;
  sections: readonly MenuSection[];
  onSelect: (id: string) => void;
  /** Which edge of the trigger the menu lines up with. */
  align?: "start" | "end";
  triggerClassName?: string;
  title?: string;
  disabled?: boolean;
  "data-testid"?: string;
}

/** A button that opens a list of choices under it (the WAI-ARIA menu button): Enter, Space or ↓
 *  open it with the checked choice (else the first) focused; ↑ ↓ Home End move; Enter or a click
 *  picks; Esc, Tab or a click elsewhere close it and give the focus back to the button. Rows never
 *  wrap: the list is as wide as its longest row (docs/frontend.md, one line when there is room). */
export function Menu({
  trigger,
  label,
  triggerLabel,
  sections,
  onSelect,
  align = "start",
  triggerClassName,
  title,
  disabled = false,
  "data-testid": testId,
}: MenuProps) {
  const [open, setOpen] = useState(false);
  const menuId = useId();
  const wrapper = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const items = useRef<(HTMLButtonElement | null)[]>([]);
  /** The row the menu focuses once it is open: the checked choice, else the first. */
  const focusOnOpen = useRef(0);
  const flat = sections.flatMap((s) => s.items);

  const close = useCallback((refocus: boolean) => {
    setOpen(false);
    if (refocus) button.current?.focus();
  }, []);

  const openMenu = () => {
    const checked = flat.findIndex((i) => i.kind === "radio" && i.checked);
    focusOnOpen.current = checked >= 0 ? checked : 0;
    setOpen(true);
  };

  useEffect(() => {
    if (open) items.current[focusOnOpen.current]?.focus();
  }, [open]);

  // A press anywhere else closes it (without taking the focus back).
  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!(e.target instanceof Node) || !wrapper.current?.contains(e.target)) close(false);
    };
    document.addEventListener("pointerdown", onPointer, true);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
    };
  }, [open, close]);

  return (
    <div ref={wrapper} className="relative inline-flex">
      <button
        ref={button}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-label={triggerLabel ?? label}
        title={title}
        disabled={disabled}
        data-testid={testId}
        onClick={() => {
          if (open) close(false);
          else openMenu();
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" && !open) {
            e.preventDefault();
            openMenu();
          }
        }}
        className={triggerClassName}>
        {trigger}
      </button>
      {open && (
        <MenuList
          id={menuId}
          label={label}
          sections={sections}
          items={items}
          onPick={(id) => {
            close(true);
            onSelect(id);
          }}
          onClose={close}
          testId={testId === undefined ? undefined : `${testId}-menu`}
          className={cx(
            "absolute top-full mt-1 min-w-full",
            align === "end" ? "right-0" : "left-0",
          )}
        />
      )}
    </div>
  );
}

interface MenuListProps {
  id?: string;
  label: string;
  sections: readonly MenuSection[];
  /** The rows' buttons, in order, for the focus. */
  items: MutableRefObject<(HTMLButtonElement | null)[]>;
  onPick: (id: string) => void;
  /** Esc (`true`: the focus goes back) or Tab (`false`). */
  onClose: (refocus: boolean) => void;
  testId?: string;
  className?: string;
  style?: CSSProperties;
  listRef?: Ref<HTMLDivElement>;
}

/** The rows of a menu, shared by the menu button and the context menu: ↑ ↓ Home End move (across
 *  sections, wrapping), Enter or a click picks, Esc and Tab close. */
function MenuList({
  id,
  label,
  sections,
  items,
  onPick,
  onClose,
  testId,
  className,
  style,
  listRef,
}: MenuListProps) {
  const flat = sections.flatMap((s) => s.items);
  const move = (from: number, delta: number) => {
    const n = flat.length;
    if (n === 0) return;
    items.current[(from + delta + n) % n]?.focus();
  };

  const onMenuKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const index = items.current.findIndex((el) => el === document.activeElement);
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        move(index, 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        move(index < 0 ? 0 : index, -1);
        break;
      case "Home":
        e.preventDefault();
        items.current[0]?.focus();
        break;
      case "End":
        e.preventDefault();
        items.current[flat.length - 1]?.focus();
        break;
      case "Escape":
        // Handled here: a dialog under the menu must not close with it.
        e.preventDefault();
        e.stopPropagation();
        onClose(true);
        break;
      case "Tab":
        onClose(false);
        break;
    }
  };

  // Each section's first row in `flat` (the arrow keys move across sections).
  const starts = sections.map((_, s) =>
    sections.slice(0, s).reduce((n, section) => n + section.items.length, 0),
  );
  return (
    <div
      ref={listRef}
      id={id}
      role="menu"
      aria-label={label}
      // Inside the title bar's drag region: a press on a heading or the padding must not move
      // the window (Tauri's drag script stops at "false").
      data-tauri-drag-region="false"
      onKeyDown={onMenuKey}
      data-testid={testId}
      style={style}
      className={cx(
        "z-50 flex w-max flex-col rounded-10 bg-surface py-1 whitespace-nowrap shadow-win hairline",
        className,
      )}>
      {sections.map((section, s) => (
        <div
          key={section.label ?? `section-${s}`}
          role="group"
          aria-label={section.label}
          className={cx(s > 0 && "mt-1 border-t border-border pt-1")}>
          {section.label !== undefined && (
            <div aria-hidden className="px-3 pt-1 pb-0.5 text-[11px] text-fg-subtle">
              {section.label}
            </div>
          )}
          {section.items.map((item, i) => {
            const at = (starts[s] ?? 0) + i;
            return (
              <button
                key={item.id}
                ref={(el) => {
                  items.current[at] = el;
                }}
                type="button"
                role={item.kind === "radio" ? "menuitemradio" : "menuitem"}
                aria-checked={item.kind === "radio" ? item.checked : undefined}
                tabIndex={-1}
                onClick={() => onPick(item.id)}
                className="flex h-8 w-full items-center gap-2 px-3 text-left text-[13px] text-fg outline-none hover:bg-inset focus-visible:bg-inset focus:bg-inset">
                <span className="flex w-4 shrink-0 justify-center text-accent-text">
                  {item.kind === "radio" && item.checked && <Icon name="check" size={14} />}
                  {item.kind === "action" && item.icon !== undefined && (
                    <Icon name={item.icon} size={14} className="text-fg-muted" />
                  )}
                </span>
                <span {...(item.kind === "radio" && item.userText ? { "data-user-text": "" } : {})}>
                  {item.label}
                </span>
              </button>
            );
          })}
        </div>
      ))}
    </div>
  );
}

/** A point in the window, in CSS pixels. */
export interface MenuPoint {
  x: number;
  y: number;
}

export interface ContextMenuProps {
  /** Where the menu opens (the pointer of a right click); `null` keeps it closed. */
  at: MenuPoint | null;
  label: string;
  sections: readonly MenuSection[];
  onSelect: (id: string) => void;
  /** Closed by a pick, Esc, Tab, a press elsewhere, a scroll, a resize or the window's blur. */
  onClose: () => void;
  "data-testid"?: string;
}

/** Room kept between a context menu and the window's edges. */
const EDGE = 8;

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), Math.max(low, high));
}

/** A menu opened at a point (a right click, or beside a row from the keyboard), the same rows and
 *  keys as `Menu`'s. It stays inside the window, starts on its first row, and gives the focus back
 *  to where it was when it closes with Esc or a pick. */
export function ContextMenu({
  at,
  label,
  sections,
  onSelect,
  onClose,
  "data-testid": testId,
}: ContextMenuProps) {
  const list = useRef<HTMLDivElement>(null);
  const items = useRef<(HTMLButtonElement | null)[]>([]);
  const opener = useRef<Element | null>(null);

  // Measured where it was asked to open, moved inside the window before it is painted, then
  // focused on its first row.
  useLayoutEffect(() => {
    const menu = list.current;
    if (at === null || menu === null) return;
    opener.current = document.activeElement;
    const below = at.y + menu.offsetHeight + EDGE <= window.innerHeight;
    const left = clamp(at.x, EDGE, window.innerWidth - menu.offsetWidth - EDGE);
    const top = clamp(
      below ? at.y : at.y - menu.offsetHeight,
      EDGE,
      window.innerHeight - menu.offsetHeight - EDGE,
    );
    menu.style.left = `${left}px`;
    menu.style.top = `${top}px`;
    items.current[0]?.focus();
  }, [at]);

  const close = useCallback(
    (refocus: boolean) => {
      onClose();
      if (refocus && opener.current instanceof HTMLElement) opener.current.focus();
    },
    [onClose],
  );

  useEffect(() => {
    if (at === null) return;
    const onPointer = (e: PointerEvent) => {
      if (!(e.target instanceof Node) || !list.current?.contains(e.target)) close(false);
    };
    const away = () => close(false);
    document.addEventListener("pointerdown", onPointer, true);
    document.addEventListener("scroll", away, true);
    window.addEventListener("resize", away);
    window.addEventListener("blur", away);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
      document.removeEventListener("scroll", away, true);
      window.removeEventListener("resize", away);
      window.removeEventListener("blur", away);
    };
  }, [at, close]);

  if (at === null) return null;
  return createPortal(
    <MenuList
      listRef={list}
      label={label}
      sections={sections}
      items={items}
      onPick={(id) => {
        close(true);
        onSelect(id);
      }}
      onClose={close}
      testId={testId}
      className="fixed"
      style={{ left: at.x, top: at.y }}
    />,
    document.body,
  );
}
