import { type ReactNode, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Icon, type IconName } from "./Icon";
import { Keycap, Keycaps } from "./Keycap";

/** One of an item's own actions (a login: copy the password, the user name, the code, show it). */
export interface CommandAction {
  id: string;
  label: string;
  run: () => void;
}

export interface CommandItem {
  id: string;
  group: string;
  label: string;
  icon?: IconName;
  /** Keyboard shortcut rendered as keycaps, e.g. `Ctrl Alt Space`. */
  keys?: string;
  /** Mono hint on the right when there is no shortcut (`当前`, `系统：明亮`). */
  hint?: string;
  disabled?: boolean;
  disabledHint?: string;
  /** What Enter does on it, the first by default; Tab moves to the next one. Without them `run`. */
  actions?: readonly CommandAction[];
  run: () => void;
}

export interface CommandPaletteProps {
  open: boolean;
  items: readonly CommandItem[];
  onClose: () => void;
  /** Called as the highlighted item changes (theme preview). */
  onHighlight?: (item: CommandItem | undefined) => void;
  placeholder?: string;
}

export function filterCommands(items: readonly CommandItem[], query: string): CommandItem[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return [...items];
  return items.filter((i) => `${i.group} ${i.label} ${i.hint ?? ""}`.toLowerCase().includes(q));
}

function Highlight({ text, query }: { text: string; query: string }): ReactNode {
  const q = query.trim();
  if (q.length === 0) return text;
  const idx = text.toLowerCase().indexOf(q.toLowerCase());
  if (idx < 0) return text;
  return (
    <>
      {text.slice(0, idx)}
      <span className="underline decoration-fg decoration-1 underline-offset-2">
        {text.slice(idx, idx + q.length)}
      </span>
      {text.slice(idx + q.length)}
    </>
  );
}

/** The keys the palette answers, wherever the focus is while it is open. */
const PALETTE_KEYS = new Set(["ArrowDown", "ArrowUp", "Tab", "Enter", "Escape"]);

/** Ctrl K palette: 560 wide, grouped items, ↑↓ Enter Tab Esc, 28 % scrim. Remounts on open so
 *  query and cursor start fresh every time. */
export function CommandPalette(props: CommandPaletteProps) {
  if (!props.open) return null;
  return <PaletteBody {...props} />;
}

function PaletteBody({ items, onClose, onHighlight, placeholder }: CommandPaletteProps) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  // The action Enter runs on the highlighted item, among its own (a login's).
  const [action, setAction] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const results = useMemo(() => filterCommands(items, query), [items, query]);
  const groups = useMemo(() => {
    const order: string[] = [];
    for (const r of results) if (!order.includes(r.group)) order.push(r.group);
    return order;
  }, [results]);
  const current = results[Math.min(cursor, Math.max(0, results.length - 1))];

  // The input has the focus before anything can be typed: an effect runs a few frames after the
  // palette is in the page, and an Esc in between closed nothing (the quick search, measured).
  useLayoutEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    onHighlight?.(current);
  }, [current, onHighlight]);

  const move = (delta: number) => {
    if (results.length === 0) return;
    setAction(0);
    setCursor((c) => (c + delta + results.length) % results.length);
  };
  const actions = current?.actions ?? [];
  const chosen = actions[Math.min(action, Math.max(0, actions.length - 1))];
  const cycle = (delta: number) => setAction((a) => (a + delta + actions.length) % actions.length);
  const jumpGroup = () => {
    if (!current) return;
    const idx = groups.indexOf(current.group);
    const nextGroup = groups[(idx + 1) % groups.length];
    const nextIndex = results.findIndex((r) => r.group === nextGroup);
    if (nextIndex >= 0) setCursor(nextIndex);
  };
  const run = (item: CommandItem | undefined, choice?: CommandAction) => {
    if (!item || item.disabled) return;
    if (choice) choice.run();
    else item.run();
    onClose();
  };
  const onKey = (e: { key: string; shiftKey: boolean; preventDefault: () => void }) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "Tab") {
      e.preventDefault();
      // On a login, Tab goes through its actions; elsewhere it jumps to the next group.
      if (actions.length > 1) cycle(e.shiftKey ? -1 : 1);
      else jumpGroup();
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(current, chosen);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };
  const latestKey = useRef(onKey);
  useLayoutEffect(() => {
    latestKey.current = onKey;
  });
  // A key that lands outside the dialog (on <body> after a click on a part of the palette that
  // takes no focus) still drives it, and the focus goes back to the input.
  useEffect(() => {
    const outside = (e: KeyboardEvent) => {
      if (e.defaultPrevented || !PALETTE_KEYS.has(e.key)) return;
      if (e.target instanceof Node && dialogRef.current?.contains(e.target)) return;
      inputRef.current?.focus();
      latestKey.current(e);
    };
    document.addEventListener("keydown", outside);
    return () => document.removeEventListener("keydown", outside);
  }, []);

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-24 scrim"
      onClick={onClose}
      data-testid="palette-scrim">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={t("ui.palette.label")}
        onClick={(e) => {
          e.stopPropagation();
        }}
        className="flex w-[560px] max-h-[424px] flex-col overflow-hidden rounded-14 bg-overlay hairline shadow-pop overlay-frost"
        onKeyDown={onKey}>
        <div className="flex h-14 items-center gap-3 border-b border-border px-4">
          <Icon name="search" size={16} className="text-fg-subtle" />
          <input
            ref={inputRef}
            role="combobox"
            aria-expanded="true"
            aria-controls="vt-palette-list"
            aria-activedescendant={current ? `vt-cmd-${current.id}` : undefined}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setCursor(0);
            }}
            placeholder={placeholder ?? t("ui.palette.placeholder")}
            className="flex-1 bg-transparent text-[15px] outline-none placeholder:text-fg-subtle"
          />
          <Keycap>Esc</Keycap>
        </div>
        <ul id="vt-palette-list" role="listbox" className="flex-1 overflow-y-auto py-1">
          {results.length === 0 && (
            <li className="flex flex-col items-center gap-1 py-10 text-center">
              <span className="text-[13px] text-fg-subtle">{t("ui.palette.noMatch")}</span>
              <span className="mono text-[11px] text-fg-subtle">{t("ui.palette.tryHint")}</span>
            </li>
          )}
          {groups.map((g) => (
            <li key={g}>
              <div className="eyebrow px-4 pt-2 pb-1 text-[10px]">{g}</div>
              <ul>
                {results
                  .filter((r) => r.group === g)
                  .map((item) => {
                    const active = item.id === current?.id;
                    return (
                      <li
                        key={item.id}
                        id={`vt-cmd-${item.id}`}
                        role="option"
                        aria-selected={active}
                        aria-disabled={item.disabled || undefined}
                        onMouseEnter={() => {
                          if (results.indexOf(item) !== cursor) setAction(0);
                          setCursor(results.indexOf(item));
                        }}
                        onClick={() => {
                          run(item, active ? chosen : item.actions?.[0]);
                        }}
                        className={cx(
                          "mx-2 flex h-10 cursor-pointer items-center gap-3 rounded-6 px-2 text-[13px]",
                          active && "bg-canvas shadow-[inset_2px_0_0_var(--primary)]",
                          item.disabled ? "cursor-not-allowed text-fg-subtle" : "text-fg",
                        )}>
                        {item.icon && <Icon name={item.icon} size={15} className="text-fg-muted" />}
                        {/* One line; cut with an ellipsis only when the palette is too narrow,
                            the whole label on hover. */}
                        <span className="min-w-0 flex-1 truncate" title={item.label}>
                          <Highlight text={item.label} query={query} />
                        </span>
                        {active && item.actions !== undefined && item.actions.length > 0 ? (
                          <span
                            className="flex shrink-0 items-center gap-1"
                            data-testid="palette-actions">
                            {item.actions.map((a) => {
                              const on = a.id === chosen?.id;
                              return (
                                <span
                                  key={a.id}
                                  data-testid="palette-action"
                                  data-active={on || undefined}
                                  onClick={(event) => {
                                    event.stopPropagation();
                                    run(item, a);
                                  }}
                                  className={cx(
                                    "inline-flex h-[22px] items-center gap-1 rounded-6 px-2 text-[11.5px] whitespace-nowrap",
                                    on
                                      ? "bg-accent-soft text-accent-text shadow-[inset_0_0_0_1.5px_var(--accent-text)]"
                                      : "bg-inset2 text-fg-muted",
                                  )}>
                                  {a.label}
                                  {on && <Keycap>↵</Keycap>}
                                </span>
                              );
                            })}
                          </span>
                        ) : item.disabled && item.disabledHint ? (
                          <span className="mono shrink-0 text-[11px] whitespace-nowrap text-fg-subtle">
                            {item.disabledHint}
                          </span>
                        ) : item.keys ? (
                          <Keycaps keys={item.keys} />
                        ) : item.hint ? (
                          <span className="mono shrink-0 text-[11px] whitespace-nowrap text-fg-muted">
                            {item.hint}
                          </span>
                        ) : null}
                      </li>
                    );
                  })}
              </ul>
            </li>
          ))}
        </ul>
        <div className="mono flex h-8 items-center justify-between border-t border-border px-4 text-[10px] text-fg-subtle">
          <span>{t(actions.length > 1 ? "ui.palette.footerActions" : "ui.palette.footer")}</span>
          <span>{t("ui.palette.results", { n: results.length })}</span>
        </div>
      </div>
    </div>
  );
}
