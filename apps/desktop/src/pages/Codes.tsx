// The home page: every account with its current code. Click or Enter copies; ↑ ↓ move between
// rows; "/" or Ctrl F searches. Favourites come first, then the chosen order. With groups, the
// accounts are in sections that fold; a right click (or the context-menu key) opens a row's menu.
// "Select" ticks several accounts (one by one, a section, or all that the search shows) to move
// them to a group at once. "Reorder" shows the whole list with handles: the groups and the accounts
// inside them move by dragging or with ↑ ↓ on a handle, and the order is this device's.
import {
  ALL_GROUPS,
  type EntryView,
  NO_GROUP,
  SORT_ORDERS,
  type SortOrder,
  entryGroups,
  entryLabel,
  filterEntries,
  groupSections,
  relativeTime,
  sortEntries,
} from "@lockra/shared";
import {
  Button,
  Card,
  ContextMenu,
  EmptyState,
  EntryRow,
  Icon,
  IconButton,
  Input,
  Lamp,
  Menu,
  type MenuPoint,
  type MenuSection,
  ReorderList,
  Select,
  useBackend,
  useClock,
  useCodes,
  useNow,
  useT,
  useUiState,
} from "@lockra/ui";
import {
  type KeyboardEvent,
  type MouseEvent,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { motionReduced } from "../app/appearance";
import { useDispatch, useGuarded } from "../app/dispatch";
import { useShell } from "../app/shell-state";
import { MoveGroupDialog } from "../features/entries/MoveGroupDialog";

/** A row's menu: what it offers and what each choice does, shared by the "⋯" button, the right
 *  click and the buttons beside them. `select` starts a selection with the row ticked. */
function useRowActions(select?: (entry: EntryView) => void) {
  const t = useT();
  const shell = useShell();
  const dispatch = useDispatch();
  const sections = (entry: EntryView): MenuSection[] => [
    {
      items: [
        {
          kind: "action",
          id: "favorite",
          label: entry.favorite ? t("codes.unfavorite") : t("codes.favorite"),
          icon: "star",
        },
        { kind: "action", id: "edit", label: t("codes.edit"), icon: "edit" },
        { kind: "action", id: "reveal", label: t("codes.reveal"), icon: "qr" },
      ],
    },
    ...(select
      ? [
          {
            items: [
              {
                kind: "action" as const,
                id: "select",
                label: t("codes.select"),
                icon: "check" as const,
              },
            ],
          },
        ]
      : []),
    { items: [{ kind: "action", id: "delete", label: t("codes.remove"), icon: "trash" }] },
  ];
  const run = (entry: EntryView, id: string) => {
    if (id === "select") select?.(entry);
    else if (id === "favorite")
      void dispatch({
        command: "entry_update",
        id: entry.id,
        patch: { favorite: !entry.favorite },
      });
    else if (id === "edit") shell.open({ type: "edit", id: entry.id });
    else if (id === "reveal") shell.open({ type: "reveal", id: entry.id });
    else shell.open({ type: "delete", id: entry.id });
  };
  return { sections, run };
}

export function Codes() {
  const t = useT();
  const state = useUiState();
  const shell = useShell();
  const dispatch = useDispatch();
  const searchId = useId();
  const list = useRef<HTMLDivElement>(null);
  const codes = useCodes();
  const now = useClock();
  const [query, setQuery] = useState("");
  const [group, setGroup] = useState(ALL_GROUPS);
  const [context, setContext] = useState<{ entry: EntryView; at: MenuPoint } | null>(null);
  const [selecting, setSelecting] = useState(false);
  const [ticked, setTicked] = useState<ReadonlySet<string>>(new Set());
  const [moving, setMoving] = useState(false);
  const [reordering, setReordering] = useState(false);
  const startSelection = (entry?: EntryView) => {
    setTicked(new Set(entry ? [entry.id] : []));
    setSelecting(true);
  };
  const stopSelection = () => {
    setSelecting(false);
    setMoving(false);
    setTicked(new Set());
  };
  const tick = (ids: readonly string[], on: boolean) =>
    setTicked((current) => {
      const next = new Set(current);
      for (const id of ids) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return next;
    });
  const actions = useRowActions(startSelection);
  const {
    settings,
    entries,
    collapsed_groups: collapsedGroups,
    entry_order: entryOrder,
    group_order: groupOrder,
  } = state;
  const groups = useMemo(() => entryGroups(entries), [entries]);
  // A group that no longer exists (its last account moved) shows everything again.
  const activeGroup = groups.includes(group) ? group : ALL_GROUPS;
  const visible = useMemo(
    () => filterEntries(sortEntries(entries, settings.sort, entryOrder), query, activeGroup),
    [entries, settings.sort, entryOrder, query, activeGroup],
  );
  // What a move takes: the ticked accounts the search and the group above still show.
  const chosen = useMemo(
    () => visible.filter((e) => ticked.has(e.id)).map((e) => e.id),
    [visible, ticked],
  );
  const grouped = settings.group_codes && groups.length > 0;
  const searching = query.trim() !== "";
  const folded = useMemo(() => new Set(collapsedGroups), [collapsedGroups]);
  const sections = useMemo(
    () => (grouped ? groupSections(visible, groupOrder) : [{ key: NO_GROUP, entries: visible }]),
    [grouped, visible, groupOrder],
  );
  // Every section of the whole list, whatever the search or the group chosen above it.
  const allSections = useMemo(() => groupSections(entries).map((s) => s.key), [entries]);
  const startReorder = () => {
    stopSelection();
    setQuery("");
    setGroup(ALL_GROUPS);
    setReordering(true);
  };
  const fold = (keys: readonly string[]) =>
    void dispatch({ command: "view_collapse_groups", groups: [...keys] });
  const toggleFold = (key: string) =>
    fold(folded.has(key) ? [...folded].filter((k) => k !== key) : [...folded, key]);

  useEffect(() => {
    if (shell.searchFocus > 0) document.getElementById(searchId)?.focus();
  }, [shell.searchFocus, searchId]);

  const rows = () =>
    Array.from(list.current?.querySelectorAll<HTMLElement>('[data-testid="entry-row"]') ?? []);
  const copy = (entry: EntryView) => void dispatch({ command: "entry_copy", id: entry.id });

  /** The row's menu at the pointer of a right click, or beside the row from the keyboard. */
  const openContext = (row: HTMLElement, entry: EntryView, pointer?: MenuPoint) => {
    row.focus();
    const box = row.getBoundingClientRect();
    setContext({ entry, at: pointer ?? { x: box.right - 48, y: box.top + box.height / 2 } });
  };
  const onRowContextMenu = (event: MouseEvent<HTMLDivElement>, entry: EntryView) => {
    event.preventDefault();
    // The context-menu key fires this too, at no point of its own.
    const keyboard = event.clientX === 0 && event.clientY === 0;
    openContext(
      event.currentTarget,
      entry,
      keyboard ? undefined : { x: event.clientX, y: event.clientY },
    );
  };

  const onListKey = (event: KeyboardEvent<HTMLDivElement>) => {
    if (selecting && event.key === "Escape") {
      event.preventDefault();
      stopSelection();
      return;
    }
    if (selecting && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a") {
      event.preventDefault();
      tick(
        visible.map((e) => e.id),
        true,
      );
      return;
    }
    const all = rows();
    // Only from a row itself: a menu or a button inside a row keeps its own arrow keys.
    const at = all.findIndex((row) => row === event.target);
    if (at < 0) return;
    const row = all[at];
    if (row && (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10"))) {
      event.preventDefault();
      const entry = entries.find((e) => e.id === row.dataset.entry);
      if (entry) openContext(row, entry);
      return;
    }
    const next =
      event.key === "ArrowDown"
        ? at + 1
        : event.key === "ArrowUp"
          ? at - 1
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? all.length - 1
              : undefined;
    if (next === undefined) return;
    event.preventDefault();
    if (next < 0) document.getElementById(searchId)?.focus();
    else all[Math.min(next, all.length - 1)]?.focus();
  };

  const onSearchKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      rows()[0]?.focus();
    } else if (event.key === "Enter") {
      event.preventDefault();
      const first = visible[0];
      if (first) copy(first);
    } else if (event.key === "Escape" && query !== "") {
      event.preventDefault();
      setQuery("");
    }
  };

  const setSort = (sort: SortOrder) =>
    void dispatch({ command: "settings_set", settings: { ...settings, sort } });

  return (
    <div className="flex flex-col gap-4" data-testid="page-codes">
      <StatusCard />
      {entries.length === 0 ? (
        <Card>
          <EmptyState
            icon="key"
            title={t("codes.emptyTitle")}
            actions={
              <>
                <Button variant="primary" icon="download" onClick={() => shell.navigate("import")}>
                  {t("codes.emptyImport")}
                </Button>
                <Button icon="plus" onClick={() => shell.open({ type: "add_manual" })}>
                  {t("codes.emptyManual")}
                </Button>
              </>
            }>
            {t("codes.emptyBody")}
          </EmptyState>
        </Card>
      ) : (
        <>
          <div className="flex flex-wrap items-end gap-2">
            <Input
              id={searchId}
              icon="search"
              keys="/"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={onSearchKey}
              placeholder={t("codes.search")}
              aria-label={t("codes.search")}
              className="min-w-[14rem] flex-1"
              disabled={reordering}
              data-testid="codes-search"
            />
            {groups.length > 0 && (
              <Select
                aria-label={t("entry.group")}
                value={activeGroup}
                onChange={setGroup}
                disabled={reordering}
                options={[
                  { value: ALL_GROUPS, label: t("codes.allGroups") },
                  ...groups.map((g) => ({ value: g, label: g })),
                ]}
                data-testid="codes-group"
              />
            )}
            <Select
              aria-label={t("codes.sortLabel")}
              value={settings.sort}
              onChange={setSort}
              options={SORT_ORDERS.map((value) => ({ value, label: t(`codes.sort.${value}`) }))}
              data-testid="codes-sort"
            />
            {groups.length > 0 && (
              <IconButton
                icon="folder"
                label={t("codes.groupView")}
                size={28}
                bordered
                pressed={settings.group_codes}
                className="mb-0.5"
                onClick={() =>
                  void dispatch({
                    command: "settings_set",
                    settings: { ...settings, group_codes: !settings.group_codes },
                  })
                }
                data-testid="codes-group-view"
              />
            )}
            {grouped && (
              <>
                <Button
                  variant="outline"
                  icon="chevronUp"
                  disabled={allSections.every((key) => folded.has(key))}
                  onClick={() => fold(allSections)}
                  data-testid="codes-collapse-all">
                  {t("codes.collapseAll")}
                </Button>
                <Button
                  variant="outline"
                  icon="chevronDown"
                  disabled={!allSections.some((key) => folded.has(key))}
                  onClick={() => fold([])}
                  data-testid="codes-expand-all">
                  {t("codes.expandAll")}
                </Button>
              </>
            )}
            {!selecting && !reordering && (
              <>
                <Button
                  variant="outline"
                  icon="check"
                  onClick={() => startSelection()}
                  data-testid="codes-select">
                  {t("codes.select")}
                </Button>
                <Button
                  variant="outline"
                  icon="drag"
                  onClick={startReorder}
                  data-testid="codes-reorder">
                  {t("codes.reorder.open")}
                </Button>
              </>
            )}
          </div>
          {reordering && (
            <div
              role="toolbar"
              aria-label={t("codes.reorder.open")}
              onKeyDown={(event) => {
                if (event.key !== "Escape") return;
                event.preventDefault();
                setReordering(false);
              }}
              className="flex flex-wrap items-center gap-2 rounded-10 bg-surface px-3 py-2 hairline"
              data-testid="codes-reordering">
              <p className="min-w-0 flex-1 text-[13px] text-fg-muted">{t("codes.reorder.hint")}</p>
              <Button
                variant="primary"
                onClick={() => setReordering(false)}
                data-testid="codes-reorder-done">
                {t("codes.reorder.done")}
              </Button>
            </div>
          )}
          {selecting && (
            <div
              role="toolbar"
              aria-label={t("codes.select")}
              onKeyDown={(event) => {
                if (event.key !== "Escape") return;
                event.preventDefault();
                stopSelection();
              }}
              className="flex flex-wrap items-center gap-2 rounded-10 bg-surface px-3 py-2 hairline"
              data-testid="codes-selection">
              <span className="text-[13px] text-fg" aria-live="polite">
                {t("codes.selectedCount", { n: chosen.length })}
              </span>
              <Button
                variant="text"
                size="sm"
                onClick={() =>
                  tick(
                    visible.map((e) => e.id),
                    true,
                  )
                }>
                {t("codes.selectAll")}
              </Button>
              <Button variant="text-muted" size="sm" onClick={() => setTicked(new Set())}>
                {t("codes.selectNone")}
              </Button>
              <span className="flex-1" />
              <Button
                variant="primary"
                icon="folder"
                disabled={chosen.length === 0}
                onClick={() => setMoving(true)}
                data-testid="codes-move">
                {t("codes.moveToGroup")}
              </Button>
              <Button variant="outline" onClick={stopSelection}>
                {t("common.done")}
              </Button>
            </div>
          )}
          {reordering ? (
            <Card padding="none" className="p-1.5">
              <ReorderList
                entries={entries}
                sort={settings.sort}
                entryOrder={entryOrder}
                groupOrder={groupOrder}
                grouped={grouped}
                folded={folded}
                onToggleFold={toggleFold}
                onOrderEntries={(ids) => void dispatch({ command: "view_order_entries", ids })}
                onOrderGroups={(order) =>
                  void dispatch({ command: "view_order_groups", groups: order })
                }
                noGroupLabel={t("codes.groupNone")}
              />
            </Card>
          ) : visible.length === 0 ? (
            <Card>
              <EmptyState
                compact
                icon="search"
                title={t("codes.noMatch", { query: query.trim() })}
              />
            </Card>
          ) : (
            <Card padding="none" className="p-1.5">
              <div ref={list} onKeyDown={onListKey} data-testid="codes-list">
                {sections.map((section) => {
                  const name = section.key === NO_GROUP ? t("codes.groupNone") : section.key;
                  // A search shows what it finds in every section, folded or not.
                  const open = !grouped || searching || !folded.has(section.key);
                  return (
                    <section
                      key={section.key === NO_GROUP ? "\u0000" : section.key}
                      aria-label={grouped ? name : undefined}
                      data-testid={grouped ? "codes-section" : undefined}>
                      {grouped && (
                        <div className="flex items-center gap-1">
                          {selecting && (
                            <SectionTick
                              label={t("codes.selectGroup", { group: name })}
                              ids={section.entries.map((e) => e.id)}
                              ticked={ticked}
                              onChange={tick}
                            />
                          )}
                          <button
                            type="button"
                            aria-expanded={open}
                            disabled={searching}
                            onClick={() => toggleFold(section.key)}
                            data-testid="codes-group-toggle"
                            className="flex h-8 min-w-0 flex-1 items-center gap-1.5 rounded-6 px-2 text-left text-[12px] font-medium text-fg-muted outline-none transition-colors hover:bg-inset focus-visible:bg-inset disabled:hover:bg-transparent">
                            <Icon name={open ? "chevronDown" : "chevronRight"} size={14} />
                            <span
                              className="min-w-0 truncate"
                              {...(section.key === NO_GROUP ? {} : { "data-user-text": "" })}>
                              {name}
                            </span>
                            <span className="ml-auto mono text-[11px] text-fg-subtle">
                              {section.entries.length}
                            </span>
                          </button>
                        </div>
                      )}
                      {open && (
                        <div role="list" aria-label={grouped ? name : t("codes.title")}>
                          {section.entries.map((entry) => (
                            <div role="listitem" key={entry.id}>
                              <EntryRow
                                entry={entry}
                                code={codes.get(entry.id)}
                                nowMs={now}
                                masked={settings.hide_codes}
                                still={motionReduced(settings)}
                                onCopy={() => copy(entry)}
                                onNext={
                                  entry.kind.type === "hotp"
                                    ? () =>
                                        void dispatch({ command: "entry_hotp_next", id: entry.id })
                                    : undefined
                                }
                                onFavorite={() => actions.run(entry, "favorite")}
                                onEdit={() => actions.run(entry, "edit")}
                                onContextMenu={(event) => onRowContextMenu(event, entry)}
                                menu={<RowMenu entry={entry} onSelect={startSelection} />}
                                selection={
                                  selecting
                                    ? {
                                        checked: ticked.has(entry.id),
                                        onToggle: () => tick([entry.id], !ticked.has(entry.id)),
                                      }
                                    : undefined
                                }
                              />
                            </div>
                          ))}
                        </div>
                      )}
                    </section>
                  );
                })}
              </div>
              <ContextMenu
                at={context?.at ?? null}
                label={t("codes.actions")}
                sections={context === null ? [] : actions.sections(context.entry)}
                onSelect={(id) => {
                  if (context !== null) actions.run(context.entry, id);
                }}
                onClose={() => setContext(null)}
                data-testid="row-context"
              />
            </Card>
          )}
          {moving && (
            <MoveGroupDialog
              ids={chosen}
              onClose={() => setMoving(false)}
              onMoved={stopSelection}
            />
          )}
        </>
      )}
    </div>
  );
}

/** Unlocked · N accounts · last backup, and the add menu. */
function StatusCard() {
  const t = useT();
  const { entries, backup } = useUiState();
  const now = useNow();
  const accounts = t("common.accounts", { n: entries.length });
  return (
    <Card
      padding="none"
      className="flex min-h-[52px] flex-wrap items-center gap-3 px-3.5 py-2"
      data-testid="codes-status">
      <Lamp tone="ok" size={10} />
      <div className="flex min-w-[10rem] flex-1 items-baseline gap-2">
        <span className="shrink-0 text-[15px] font-medium whitespace-nowrap text-fg">
          {t("shell.status.unlocked")}
        </span>
        <span className="truncate text-[13px] text-fg-muted">
          {backup.last_backup_ms === null
            ? t("codes.statusNoBackup", { accounts })
            : t("codes.status", { accounts, backup: relativeTime(t, backup.last_backup_ms, now) })}
        </span>
      </div>
      <AddMenu />
    </Card>
  );
}

export function AddMenu() {
  const t = useT();
  const shell = useShell();
  const dispatch = useDispatch();
  const guarded = useGuarded();
  const { backend } = useBackend();
  const sections: MenuSection[] = [
    {
      items: [
        { kind: "action", id: "image", label: t("codes.add.image"), icon: "image" },
        { kind: "action", id: "clipboard", label: t("codes.add.clipboard"), icon: "clipboard" },
      ],
    },
    {
      items: [
        { kind: "action", id: "manual", label: t("codes.add.manual"), icon: "edit" },
        { kind: "action", id: "uri", label: t("codes.add.uri"), icon: "link" },
      ],
    },
  ];
  const onSelect = (id: string) => {
    if (id === "image") void guarded(() => backend.pickImportFiles("images"));
    else if (id === "clipboard") void dispatch({ command: "import_clipboard" });
    else if (id === "manual") shell.open({ type: "add_manual" });
    else shell.open({ type: "add_uri" });
  };
  return (
    <Menu
      label={t("codes.add.label")}
      align="end"
      sections={sections}
      onSelect={onSelect}
      data-testid="add-menu"
      triggerClassName="inline-flex h-8 items-center gap-2 rounded-6 bg-primary px-3 text-[13px] font-medium whitespace-nowrap text-primary-fg transition-colors hover:opacity-90"
      trigger={
        <>
          <Icon name="plus" size={14} />
          {t("codes.add.label")}
          <Icon name="chevronDown" size={12} />
        </>
      }
    />
  );
}

function RowMenu({ entry, onSelect }: { entry: EntryView; onSelect: (entry: EntryView) => void }) {
  const t = useT();
  const actions = useRowActions(onSelect);
  return (
    <Menu
      label={t("codes.more")}
      triggerLabel={`${t("codes.more")} · ${entryLabel(entry.issuer, entry.account)}`}
      title={t("codes.more")}
      align="end"
      sections={actions.sections(entry)}
      onSelect={(id) => actions.run(entry, id)}
      data-testid="row-menu"
      triggerClassName="inline-flex h-7 w-7 items-center justify-center rounded-6 text-fg-muted transition-colors hover:bg-inset2 hover:text-fg"
      trigger={<Icon name="more" size={16} />}
    />
  );
}

/** A section's box while selecting: ticks or clears all its accounts, and shows "some" between. */
function SectionTick({
  label,
  ids,
  ticked,
  onChange,
}: {
  label: string;
  ids: readonly string[];
  ticked: ReadonlySet<string>;
  onChange: (ids: readonly string[], on: boolean) => void;
}) {
  const count = ids.filter((id) => ticked.has(id)).length;
  const all = ids.length > 0 && count === ids.length;
  return (
    <input
      type="checkbox"
      aria-label={label}
      checked={all}
      ref={(box) => {
        if (box) box.indeterminate = count > 0 && !all;
      }}
      onChange={() => onChange(ids, !all)}
      className="ml-3 size-4 shrink-0 accent-accent"
    />
  );
}
