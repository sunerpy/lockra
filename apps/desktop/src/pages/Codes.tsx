// The home page (board A1): its header with the search and the add menu, the account used last in a
// card of its own beside the vault's state, the filters (全部, 收藏, each group, 未分组) with the list's
// tools, and every account with its current code, as a list or as cards (验证码视图, a setting of its
// own whatever the theme). Click or Enter copies; ↑ ↓ move between rows, and ← → too between cards;
// "/" or Ctrl F searches. Favourites come first, then the chosen order. With groups, the accounts
// are in sections that fold; a right click (or the context-menu key) opens a row's menu. "Select"
// (in the list's ⋯ menu) ticks several accounts (one by one, a section, or all that the search
// shows) to move them to a group at once. "Reorder" shows the whole list with handles: the groups
// and the accounts inside them move by dragging or with ↑ ↓ on a handle, and the order is this
// device's. Selecting and reordering happen in the list, whichever view is chosen.
import {
  ALL_GROUPS,
  type CodesView,
  type EntryView,
  FAVORITES,
  NO_GROUP,
  SORT_ORDERS,
  type SortOrder,
  type SyncDeviceView,
  UNGROUPED,
  entryGroups,
  entryLabel,
  filterEntries,
  groupSections,
  recentEntry,
  relativeTime,
  sortEntries,
  syncSummary,
} from "@lockra/shared";
import {
  Button,
  Card,
  ContextMenu,
  EmptyState,
  EntryRow,
  EntryTile,
  type FilterChip,
  FilterChips,
  Icon,
  IconButton,
  Input,
  Lamp,
  Menu,
  initial,
  type MenuPoint,
  type MenuSection,
  PageHeader,
  RecentCard,
  ReorderList,
  Segmented,
  Select,
  cx,
  tileStep,
  useBackend,
  useClock,
  useCodes,
  useFlash,
  useT,
  useUiState,
} from "@lockra/ui";
import {
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
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
  // The row or card just copied says so for a moment.
  const [copiedId, flash] = useFlash();
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
  // The filters (board A1): everything, the pinned accounts, each group, those in no group.
  const filters = useMemo((): FilterChip[] => {
    const counts = new Map<string, number>();
    for (const e of entries) counts.set(e.group ?? "", (counts.get(e.group ?? "") ?? 0) + 1);
    const pinned = entries.filter((e) => e.favorite).length;
    const loose = counts.get("") ?? 0;
    return [
      { value: ALL_GROUPS, label: t("codes.chips.all"), count: entries.length },
      ...(pinned > 0
        ? [
            {
              value: FAVORITES,
              label: t("codes.chips.favorites"),
              count: pinned,
              icon: "star" as const,
            },
          ]
        : []),
      ...groups.map((g) => ({ value: g, label: g, count: counts.get(g) ?? 0, userText: true })),
      ...(groups.length > 0 && loose > 0
        ? [{ value: UNGROUPED, label: t("codes.groupNone"), count: loose }]
        : []),
    ];
  }, [entries, groups, t]);
  // A filter that no longer exists (its last account moved or unpinned) shows everything again.
  const activeGroup = filters.some((f) => f.value === group) ? group : ALL_GROUPS;
  const sorted = useMemo(
    () => sortEntries(entries, settings.sort, entryOrder),
    [entries, settings.sort, entryOrder],
  );
  const visible = useMemo(
    () => filterEntries(sorted, query, activeGroup),
    [sorted, query, activeGroup],
  );
  // What a move takes: the ticked accounts the search and the group above still show.
  const chosen = useMemo(
    () => visible.filter((e) => ticked.has(e.id)).map((e) => e.id),
    [visible, ticked],
  );
  const grouped = settings.group_codes && groups.length > 0;
  // Ticking several accounts happens in the list.
  const cards = settings.codes_view === "grid" && !selecting;
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
    Array.from(
      list.current?.querySelectorAll<HTMLElement>(
        '[data-testid="entry-row"], [data-testid="entry-tile"]',
      ) ?? [],
    );
  const copy = (entry: EntryView) =>
    void dispatch({ command: "entry_copy", id: entry.id }).then((done) => {
      // `null` once copied; a failure has become a toast and `undefined`.
      if (done !== undefined) flash(entry.id);
    });
  const copiedNote =
    settings.clipboard_clear_seconds > 0
      ? t("notice.copied", { s: settings.clipboard_clear_seconds })
      : t("notice.copiedKept");

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
    const next = cards
      ? tileStep(
          all.map((tile) => {
            const box = tile.getBoundingClientRect();
            return { left: box.left, top: box.top, width: box.width };
          }),
          at,
          event.key,
        )
      : event.key === "ArrowDown"
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
  const setView = (codes_view: CodesView) =>
    void dispatch({ command: "settings_set", settings: { ...settings, codes_view } });

  const toolSections: MenuSection[] = [
    { items: [{ kind: "action", id: "select", label: t("codes.select"), icon: "check" }] },
    ...(groups.length > 0
      ? [
          {
            items: [
              {
                kind: "check" as const,
                id: "group-view",
                label: t("codes.groupView"),
                checked: settings.group_codes,
              },
              ...(grouped && allSections.some((key) => !folded.has(key))
                ? [
                    {
                      kind: "action" as const,
                      id: "collapse-all",
                      label: t("codes.collapseAll"),
                      icon: "chevronUp" as const,
                    },
                  ]
                : []),
              ...(grouped && allSections.some((key) => folded.has(key))
                ? [
                    {
                      kind: "action" as const,
                      id: "expand-all",
                      label: t("codes.expandAll"),
                      icon: "chevronDown" as const,
                    },
                  ]
                : []),
            ],
          },
        ]
      : []),
  ];
  const onTool = (id: string) => {
    if (id === "select") startSelection();
    else if (id === "group-view")
      void dispatch({
        command: "settings_set",
        settings: { ...settings, group_codes: !settings.group_codes },
      });
    else if (id === "collapse-all") fold(allSections);
    else fold([]);
  };
  // The card of the account used last, else the first of the list (board A1); not while searching,
  // selecting or reordering, which are about the list.
  const recent = recentEntry(entries) ?? sorted[0];
  const hero = recent !== undefined && !searching && !selecting && !reordering;

  return (
    <div className="flex flex-col gap-4" data-testid="page-codes">
      <PageHeader
        title={t("codes.title")}
        subtitle={<CodesFacts groups={groups.length} />}
        actions={
          <>
            {entries.length > 0 && (
              <Input
                id={searchId}
                size="xl"
                pill
                icon="search"
                keys="/"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={onSearchKey}
                placeholder={t("codes.search")}
                aria-label={t("codes.search")}
                className="w-[min(26rem,36vw)]"
                disabled={reordering}
                data-testid="codes-search"
              />
            )}
            <AddMenu />
          </>
        }
      />
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
          {hero && (
            <div
              className="grid grid-cols-[minmax(0,5fr)_minmax(0,3fr)] gap-4"
              data-testid="codes-hero">
              <RecentCard
                entry={recent}
                code={codes.get(recent.id)}
                nowMs={now}
                masked={settings.hide_codes}
                still={motionReduced(settings)}
                onCopy={() => copy(recent)}
                onNext={
                  recent.kind.type === "hotp"
                    ? () => void dispatch({ command: "entry_hotp_next", id: recent.id })
                    : undefined
                }
              />
              <VaultCard />
            </div>
          )}
          <div className="flex flex-wrap items-center gap-2" data-testid="codes-tools">
            {filters.length > 1 && (
              <FilterChips
                label={t("codes.chips.label")}
                options={filters}
                value={activeGroup}
                onChange={setGroup}
                disabled={reordering}
                data-testid="codes-chips"
              />
            )}
            <span className="flex-1" />
            {!selecting && !reordering && entries.length > 1 && (
              <IconButton
                icon="drag"
                label={t("codes.reorder.open")}
                size={28}
                bordered
                onClick={startReorder}
                data-testid="codes-reorder"
              />
            )}
            <Segmented
              label={t("codes.view.label")}
              iconOnly
              value={settings.codes_view}
              onChange={setView}
              options={(["list", "grid"] as const).map((value) => ({
                value,
                label: t(`codes.view.${value}`),
                icon: value,
                // Selecting and reordering stay in the list.
                disabled: selecting || reordering,
              }))}
              data-testid="codes-view"
            />
            <Select
              aria-label={t("codes.sortLabel")}
              value={settings.sort}
              onChange={setSort}
              options={SORT_ORDERS.map((value) => ({ value, label: t(`codes.sort.${value}`) }))}
              data-testid="codes-sort"
            />
            {!selecting && !reordering && (
              <Menu
                label={t("codes.viewMenu")}
                title={t("codes.viewMenu")}
                align="end"
                sections={toolSections}
                onSelect={onTool}
                data-testid="codes-more"
                triggerClassName="inline-flex size-[28px] items-center justify-center rounded-6 bg-surface text-fg-muted hairline transition-colors hover:bg-inset hover:text-fg"
                trigger={<Icon name="more" size={16} />}
              />
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
            <>
              {cards ? (
                <div
                  ref={list}
                  onKeyDown={onListKey}
                  className="flex flex-col gap-3"
                  data-testid="codes-grid">
                  {sections.map((section) => {
                    const name = section.key === NO_GROUP ? t("codes.groupNone") : section.key;
                    const open = !grouped || searching || !folded.has(section.key);
                    return (
                      <section
                        key={section.key === NO_GROUP ? "\u0000" : section.key}
                        aria-label={grouped ? name : undefined}
                        data-testid={grouped ? "codes-section" : undefined}>
                        {grouped && (
                          <SectionHeading
                            name={name}
                            userText={section.key !== NO_GROUP}
                            count={section.entries.length}
                            open={open}
                            searching={searching}
                            onToggle={() => toggleFold(section.key)}
                          />
                        )}
                        {open && (
                          <div
                            role="list"
                            aria-label={grouped ? name : t("codes.title")}
                            className={cx(
                              "grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-4",
                              grouped && "mt-1",
                            )}>
                            {section.entries.map((entry) => (
                              <div role="listitem" key={entry.id} className="grid min-w-0">
                                <EntryTile
                                  entry={entry}
                                  code={codes.get(entry.id)}
                                  nowMs={now}
                                  masked={settings.hide_codes}
                                  still={motionReduced(settings)}
                                  copied={copiedId === entry.id ? copiedNote : undefined}
                                  onCopy={() => copy(entry)}
                                  onNext={
                                    entry.kind.type === "hotp"
                                      ? () =>
                                          void dispatch({
                                            command: "entry_hotp_next",
                                            id: entry.id,
                                          })
                                      : undefined
                                  }
                                  onFavorite={() => actions.run(entry, "favorite")}
                                  onContextMenu={(event) => onRowContextMenu(event, entry)}
                                  menu={<RowMenu entry={entry} onSelect={startSelection} />}
                                />
                              </div>
                            ))}
                          </div>
                        )}
                      </section>
                    );
                  })}
                </div>
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
                            <SectionHeading
                              name={name}
                              userText={section.key !== NO_GROUP}
                              count={section.entries.length}
                              open={open}
                              searching={searching}
                              onToggle={() => toggleFold(section.key)}
                              tick={
                                selecting && (
                                  <SectionTick
                                    label={t("codes.selectGroup", { group: name })}
                                    ids={section.entries.map((e) => e.id)}
                                    ticked={ticked}
                                    onChange={tick}
                                  />
                                )
                              }
                            />
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
                                            void dispatch({
                                              command: "entry_hotp_next",
                                              id: entry.id,
                                            })
                                        : undefined
                                    }
                                    copied={
                                      copiedId === entry.id && !selecting
                                        ? t("notice.copiedKept")
                                        : undefined
                                    }
                                    onFavorite={() => actions.run(entry, "favorite")}
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
                </Card>
              )}
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
            </>
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

/** The header's line of facts: the accounts, the groups and, in a sync space, how it stands. */
function CodesFacts({ groups }: { groups: number }) {
  const t = useT();
  const { entries, sync } = useUiState();
  const facts = [t("common.accounts", { n: entries.length })];
  if (groups > 0) facts.push(t("codes.groupsCount", { n: groups }));
  const state = sync.space?.status.state;
  if (state === "synced") facts.push(t("codes.allSynced"));
  else if (state === "syncing") facts.push(t("codes.hero.syncing"));
  else if (state === "failed") facts.push(t("codes.hero.syncFailed"));
  return facts.join(" · ");
}

/** 「保险库状态」 (board A1): unlocked and when it locks by itself, the last backup, the sync and the
 *  devices of its space. */
function VaultCard() {
  const t = useT();
  const { backup, sync, auto_lock_at_ms: autoLockAt } = useUiState();
  // The second clock, as the sidebar's: the minutes until the lock must not run a minute ahead.
  const now = useClock();
  const unlocked = t("shell.status.unlocked");
  const lock =
    autoLockAt === null
      ? unlocked
      : `${unlocked} · ${t("codes.autoLockIn", {
          time: t("common.minutes", { n: Math.max(1, Math.ceil((autoLockAt - now) / 60_000)) }),
        })}`;
  const devices = sync.space?.devices ?? [];
  return (
    <section
      aria-label={t("codes.hero.vault")}
      className="flex min-w-0 flex-col gap-3 rounded-14 bg-surface p-6 hairline panel"
      data-testid="codes-vault">
      <span className="text-[12px] font-medium text-fg-muted">{t("codes.hero.vault")}</span>
      <span className="flex min-w-0 items-center gap-2.5 text-[14px] font-medium text-fg">
        <Lamp tone="ok" size={8} />
        <span className="truncate">{lock}</span>
      </span>
      <span className="flex min-w-0 items-center gap-2.5 text-[13px] text-fg">
        <Icon name="archive" size={16} className="shrink-0 text-fg-muted" />
        <span className="truncate">
          {backup.last_backup_ms === null
            ? t("codes.hero.noBackup")
            : t("backup.manual.last", { when: relativeTime(t, backup.last_backup_ms, now) })}
        </span>
      </span>
      <span className="flex min-w-0 items-center gap-2.5 text-[13px] text-fg">
        <Icon name="cloud" size={16} className="shrink-0 text-fg-muted" />
        <span className="truncate">{syncSummary(t, sync.space)}</span>
      </span>
      {devices.length > 0 && <Devices devices={devices} />}
    </section>
  );
}

/** The colours of the devices' initials, in turn. */
const DEVICE_COLORS = ["blue", "green", "orange", "purple", "teal"] as const;

/** The space's devices: their initials side by side, then their names (this one first). */
function Devices({ devices }: { devices: readonly SyncDeviceView[] }) {
  const t = useT();
  const named = [
    ...devices.filter((d) => d.this_device),
    ...devices.filter((d) => !d.this_device),
  ].map((d) => ({ ...d, label: d.this_device ? t("codes.hero.thisDevice") : d.name || d.tag }));
  return (
    <div className="flex min-w-0 items-center gap-3" data-testid="codes-devices">
      <span aria-hidden className="flex shrink-0">
        {named.slice(0, 3).map((d, i) => (
          <span
            key={d.tag}
            data-tag={DEVICE_COLORS[i % DEVICE_COLORS.length]}
            className={cx(
              "inline-flex size-7 items-center justify-center rounded-full bg-tag-bg text-[12px] font-semibold text-tag-fg ring-2 ring-[var(--canvas)]",
              i > 0 && "-ml-2",
            )}>
            {initial(d.label)}
          </span>
        ))}
      </span>
      <span className="truncate text-[12px] text-fg-muted">
        {named.map((d) => d.label).join(" · ")}
      </span>
    </div>
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
      title={`${t("codes.add.label")} · Ctrl N`}
      align="end"
      sections={sections}
      onSelect={onSelect}
      data-testid="add-menu"
      // The page's main action (board A1): a pill in the theme's primary colour, with its glow.
      triggerClassName="inline-flex h-13.5 items-center gap-2 rounded-pill bg-primary px-5 text-[14px] font-medium whitespace-nowrap text-primary-fg shadow-primary transition-opacity hover:opacity-90"
      trigger={
        <>
          <Icon name="plus" size={18} />
          {t("codes.add.label")}
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

/** A section's heading: its name and count, folding it open or shut (not while searching, which
 *  shows what it finds everywhere); while selecting, its box first. */
function SectionHeading({
  name,
  userText,
  count,
  open,
  searching,
  onToggle,
  tick,
}: {
  name: string;
  /** The group's own name, as the user typed it (not 未分组). */
  userText: boolean;
  count: number;
  open: boolean;
  searching: boolean;
  onToggle: () => void;
  tick?: ReactNode;
}) {
  return (
    <div className="flex items-center gap-1">
      {tick}
      <button
        type="button"
        aria-expanded={open}
        disabled={searching}
        onClick={onToggle}
        data-testid="codes-group-toggle"
        className="flex h-8 min-w-0 flex-1 items-center gap-1.5 rounded-6 px-2 text-left text-[12px] font-medium text-fg-muted outline-none transition-colors hover:bg-inset focus-visible:bg-inset disabled:hover:bg-transparent">
        <Icon name={open ? "chevronDown" : "chevronRight"} size={14} />
        <span className="min-w-0 truncate" {...(userText ? { "data-user-text": "" } : {})}>
          {name}
        </span>
        <span className="ml-auto mono text-[11px] text-fg-subtle">{count}</span>
      </button>
    </div>
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
