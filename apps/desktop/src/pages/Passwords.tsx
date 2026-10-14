// The passwords page (boards 01–05 of Penpot 「Lockra 密码管理」): its header with the search and the
// add menu, the filters (全部, 收藏, each group, 未分组) and the tags beside them, the sort and the
// list's ⋯ menu, then the list of logins and, beside it at 5 : 3, the details of the one selected. A
// click or ↑ ↓ selects a login and the detail pane shows it; Enter copies its password; a right
// click (or the context-menu key) opens its menu. The logins come from `items_list` (`useItems`),
// asked for again whenever the state's `items_rev` moves; nothing of them is in the state itself.
import {
  ALL_GROUPS,
  type CopyPart,
  ITEM_SORTS,
  type ItemSort,
  type ItemView,
  NO_GROUP,
  errorText,
  filterItems,
  itemAccount,
  itemGroups,
  itemName,
  itemSections,
  itemTags,
  sortItems,
} from "@lockra/shared";
import {
  Banner,
  Button,
  Card,
  ContextMenu,
  EmptyState,
  FilterChips,
  Icon,
  Input,
  Menu,
  type MenuPoint,
  type MenuSection,
  PageHeader,
  PasswordRow,
  Select,
  SkeletonRows,
  TagFilter,
  cx,
  useFlash,
  useItemFilterChips,
  useItems,
  useItemsFailure,
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
import { useDispatch } from "../app/dispatch";
import { useShell } from "../app/shell-state";
import { ItemDetail } from "../features/items/ItemDetail";
import { useItemActions } from "../features/items/actions";

/** A login's menu: what it offers, shared by the "⋯" button and the right click. */
function useRowMenu() {
  const t = useT();
  const shell = useShell();
  const dispatch = useDispatch();
  const actions = useItemActions();
  const sections = (item: ItemView): MenuSection[] => [
    {
      items: [
        ...(itemAccount(item) !== ""
          ? [
              {
                kind: "action" as const,
                id: "copy-username",
                label: t("passwords.menu.copyUsername"),
                icon: "user" as const,
              },
            ]
          : []),
        ...(item.has_password
          ? [
              {
                kind: "action" as const,
                id: "copy-password",
                label: t("passwords.menu.copyPassword"),
                icon: "field" as const,
              },
            ]
          : []),
        ...(item.totp_entry !== null
          ? [
              {
                kind: "action" as const,
                id: "copy-code",
                label: t("passwords.menu.copyCode"),
                icon: "key" as const,
              },
            ]
          : []),
        ...(item.has_password
          ? [
              {
                kind: "action" as const,
                id: "reveal",
                label: t("passwords.menu.reveal"),
                icon: "eye" as const,
              },
            ]
          : []),
      ],
    },
    {
      items: [
        ...(item.read_only
          ? []
          : [
              {
                kind: "action" as const,
                id: "edit",
                label: t("passwords.menu.edit"),
                icon: "edit" as const,
              },
            ]),
        ...(item.read_only
          ? []
          : [
              {
                kind: "action" as const,
                id: "favorite",
                label: item.favorite
                  ? t("passwords.menu.unfavorite")
                  : t("passwords.menu.favorite"),
                icon: "star" as const,
              },
            ]),
      ],
    },
    { items: [{ kind: "action", id: "delete", label: t("passwords.menu.delete"), icon: "trash" }] },
  ];
  const run = (item: ItemView, id: string, copied: (part: CopyPart) => void) => {
    const copy = (part: CopyPart) =>
      void actions.copy(item, part).then((done) => {
        if (done) copied(part);
      });
    if (id === "copy-username")
      copy({ type: itemAccount(item) === item.username ? "username" : "email" });
    else if (id === "copy-password") copy({ type: "password" });
    else if (id === "copy-code") copy({ type: "totp" });
    else if (id === "reveal") shell.open({ type: "item_reveal", id: item.id });
    else if (id === "edit") shell.open({ type: "item_edit", id: item.id });
    else if (id === "favorite")
      void dispatch({ command: "item_update", id: item.id, patch: { favorite: !item.favorite } });
    else shell.open({ type: "item_delete", id: item.id });
  };
  return { sections, run };
}

export function Passwords() {
  const t = useT();
  const state = useUiState();
  const shell = useShell();
  const dispatch = useDispatch();
  const searchId = useId();
  const list = useRef<HTMLDivElement>(null);
  const loaded = useItems();
  const failure = useItemsFailure();
  const actions = useItemActions();
  const menu = useRowMenu();
  const [query, setQuery] = useState("");
  const [group, setGroup] = useState(ALL_GROUPS);
  const [tags, setTags] = useState<string[]>([]);
  const [context, setContext] = useState<{ item: ItemView; at: MenuPoint } | null>(null);
  // The row just copied says so for a moment: "<id>:<part>".
  const [copiedKey, flash] = useFlash();
  const items = useMemo(() => loaded ?? [], [loaded]);
  const { settings } = state;
  const filters = useItemFilterChips(items);
  // A filter or a tag that no longer exists (its last login moved) shows everything again.
  const activeGroup = filters.some((f) => f.value === group) ? group : ALL_GROUPS;
  const tagCounts = useMemo(() => itemTags(items), [items]);
  const activeTags = useMemo(
    () => tags.filter((tag) => tagCounts.some((c) => c.tag === tag)),
    [tags, tagCounts],
  );
  const sorted = useMemo(() => sortItems(items, settings.item_sort), [items, settings.item_sort]);
  const visible = useMemo(
    () => filterItems(sorted, query, activeGroup, activeTags),
    [sorted, query, activeGroup, activeTags],
  );
  const groups = useMemo(() => itemGroups(items), [items]);
  // Sections in 全部 only, as on the codes page: any other filter is one flat list.
  const grouped = activeGroup === ALL_GROUPS && groups.length > 0;
  const sections = useMemo(
    () => (grouped ? itemSections(visible) : [{ key: NO_GROUP, items: visible }]),
    [grouped, visible],
  );
  const folded = useMemo(() => new Set(state.item_collapsed_groups), [state.item_collapsed_groups]);
  const searching = query.trim() !== "";
  const { selectedItem: selectedId, selectItem: setSelectedId } = shell;
  const selected = items.find((i) => i.id === selectedId);

  useEffect(() => {
    if (shell.searchFocus > 0) document.getElementById(searchId)?.focus();
  }, [shell.searchFocus, searchId]);

  const copiedLabel = (part: CopyPart["type"]) =>
    t("notice.itemCopiedKept", { part: t(`notice.copiedPart.${part}`) });
  const COPIED: readonly CopyPart["type"][] = ["username", "email", "password", "totp", "field"];
  const copy = (item: ItemView, part: CopyPart) =>
    void actions.copy(item, part).then((done) => {
      if (done) flash(`${item.id}:${part.type}`);
    });
  const rowCopied = (item: ItemView) => {
    if (copiedKey === undefined || !copiedKey.startsWith(`${item.id}:`)) return undefined;
    const part = COPIED.find((p) => p === copiedKey.slice(item.id.length + 1));
    return part === undefined ? undefined : copiedLabel(part);
  };

  const fold = (keys: readonly string[]) =>
    void dispatch({ command: "view_collapse_items", groups: [...keys] });
  const toggleFold = (key: string) =>
    fold(folded.has(key) ? [...folded].filter((k) => k !== key) : [...folded, key]);
  const allKeys = useMemo(() => itemSections(items).map((s) => s.key), [items]);

  const rows = () =>
    Array.from(list.current?.querySelectorAll<HTMLElement>('[data-testid="password-row"]') ?? []);
  const openContext = (row: HTMLElement, item: ItemView, pointer?: MenuPoint) => {
    row.focus();
    const box = row.getBoundingClientRect();
    setContext({ item, at: pointer ?? { x: box.right - 48, y: box.top + box.height / 2 } });
  };
  const onRowContextMenu = (event: MouseEvent<HTMLDivElement>, item: ItemView) => {
    event.preventDefault();
    const keyboard = event.clientX === 0 && event.clientY === 0;
    openContext(
      event.currentTarget,
      item,
      keyboard ? undefined : { x: event.clientX, y: event.clientY },
    );
  };
  const onListKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const all = rows();
    const at = all.findIndex((row) => row === event.target);
    if (at < 0) return;
    const row = all[at];
    if (row && (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10"))) {
      event.preventDefault();
      const item = items.find((i) => i.id === row.dataset.item);
      if (item) openContext(row, item);
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
    if (next < 0) {
      document.getElementById(searchId)?.focus();
      return;
    }
    const target = all[Math.min(next, all.length - 1)];
    target?.focus();
    if (target?.dataset.item) setSelectedId(target.dataset.item);
  };
  const onSearchKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      const first = rows()[0];
      first?.focus();
      if (first?.dataset.item) setSelectedId(first.dataset.item);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const first = visible[0];
      if (first) setSelectedId(first.id);
    } else if (event.key === "Escape" && query !== "") {
      event.preventDefault();
      setQuery("");
    }
  };
  const setSort = (item_sort: ItemSort) =>
    void dispatch({ command: "settings_set", settings: { ...settings, item_sort } });
  const toolSections: MenuSection[] = grouped
    ? [
        {
          items: [
            ...(allKeys.some((k) => !folded.has(k))
              ? [
                  {
                    kind: "action" as const,
                    id: "collapse-all",
                    label: t("passwords.collapseAll"),
                    icon: "chevronUp" as const,
                  },
                ]
              : []),
            ...(allKeys.some((k) => folded.has(k))
              ? [
                  {
                    kind: "action" as const,
                    id: "expand-all",
                    label: t("passwords.expandAll"),
                    icon: "chevronDown" as const,
                  },
                ]
              : []),
          ],
        },
      ]
    : [];
  const sectionName = (key: string) => (key === NO_GROUP ? t("codes.groupNone") : key);
  const loading = loaded === undefined;

  return (
    <div className="flex flex-col gap-4" data-testid="page-passwords">
      <PageHeader
        title={t("passwords.title")}
        subtitle={<PasswordsFacts count={items.length} groups={groups.length} />}
        actions={
          <>
            {items.length > 0 && (
              <Input
                id={searchId}
                size="xl"
                pill
                icon="search"
                keys="/"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={onSearchKey}
                placeholder={t("passwords.search")}
                aria-label={t("passwords.search")}
                className="w-[min(26rem,36vw)]"
                data-testid="passwords-search"
              />
            )}
            <AddItemMenu />
          </>
        }
      />
      {failure.error !== null && (
        <Banner
          tone="danger"
          marker="bar"
          title={t("passwords.loadFailed")}
          actions={
            <Button size="sm" variant="outline" onClick={failure.retry}>
              {t("passwords.retry")}
            </Button>
          }>
          {errorText(t, failure.error)}
        </Banner>
      )}
      {!loading && items.length === 0 ? (
        <Card>
          <EmptyState
            icon="lock"
            title={t("passwords.emptyTitle")}
            actions={
              <>
                <Button variant="primary" icon="download" onClick={() => shell.navigate("import")}>
                  {t("passwords.emptyImport")}
                </Button>
                <Button icon="plus" onClick={() => shell.open({ type: "item_new" })}>
                  {t("passwords.emptyAdd")}
                </Button>
              </>
            }>
            {t("passwords.emptyBody")}
          </EmptyState>
        </Card>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-2" data-testid="passwords-tools">
            {filters.length > 1 && (
              <FilterChips
                label={t("codes.chips.label")}
                options={filters}
                value={activeGroup}
                onChange={setGroup}
                data-testid="passwords-chips"
              />
            )}
            {tagCounts.length > 0 && (
              <TagFilter
                tags={tagCounts}
                value={activeTags}
                onChange={setTags}
                data-testid="passwords-tags"
              />
            )}
            <span className="flex-1" />
            <Select
              aria-label={t("passwords.sort.label")}
              value={settings.item_sort}
              onChange={setSort}
              options={ITEM_SORTS.map((value) => ({ value, label: t(`passwords.sort.${value}`) }))}
              data-testid="passwords-sort"
            />
            {toolSections.some((s) => s.items.length > 0) && (
              <Menu
                label={t("passwords.more")}
                title={t("passwords.more")}
                align="end"
                sections={toolSections}
                onSelect={(id) => fold(id === "collapse-all" ? allKeys : [])}
                data-testid="passwords-more"
                triggerClassName="inline-flex size-[28px] items-center justify-center rounded-6 bg-surface text-fg-muted hairline transition-colors hover:bg-inset hover:text-fg"
                trigger={<Icon name="more" size={16} />}
              />
            )}
          </div>
          <div
            className="grid grid-cols-[minmax(0,5fr)_minmax(0,3fr)] items-start gap-4"
            data-testid="passwords-split">
            <Card padding="none" className="@container min-w-0 p-1.5">
              {loading ? (
                <SkeletonRows label={t("passwords.loading")} />
              ) : visible.length === 0 ? (
                <EmptyState
                  compact
                  icon="search"
                  title={t("passwords.noMatch", { query: query.trim() })}
                  actions={
                    searching ? (
                      <Button
                        icon="plus"
                        onClick={() => shell.open({ type: "item_new", title: query.trim() })}
                        data-testid="passwords-create-from-search">
                        {t("passwords.noMatchCreate", { query: query.trim() })}
                      </Button>
                    ) : undefined
                  }>
                  {t("passwords.noMatchBody")}
                </EmptyState>
              ) : (
                <div ref={list} onKeyDown={onListKey} data-testid="passwords-list">
                  {sections.map((section) => {
                    const name = sectionName(section.key);
                    const open = !grouped || searching || !folded.has(section.key);
                    return (
                      <section
                        key={section.key === NO_GROUP ? "\u0000" : section.key}
                        aria-label={grouped ? name : undefined}
                        data-testid={grouped ? "passwords-section" : undefined}>
                        {grouped && (
                          <button
                            type="button"
                            aria-expanded={open}
                            disabled={searching}
                            onClick={() => toggleFold(section.key)}
                            data-testid="passwords-group-toggle"
                            className="flex h-7 w-full min-w-0 items-center gap-1.5 rounded-6 px-2 text-left text-[12px] font-medium text-fg-muted outline-none transition-colors hover:bg-inset focus-visible:bg-inset disabled:hover:bg-transparent">
                            <Icon name={open ? "chevronDown" : "chevronRight"} size={14} />
                            <span
                              className="min-w-0 truncate"
                              {...(section.key === NO_GROUP ? {} : { "data-user-text": "" })}>
                              {name}
                            </span>
                            <span className="ml-auto mono text-[11px] text-fg-subtle">
                              {section.items.length}
                            </span>
                          </button>
                        )}
                        {open && (
                          <div role="listbox" aria-label={grouped ? name : t("passwords.list")}>
                            {section.items.map((item) => (
                              <PasswordRow
                                key={item.id}
                                item={item}
                                selected={item.id === selectedId}
                                onSelect={() => setSelectedId(item.id)}
                                onCopyPassword={() => copy(item, { type: "password" })}
                                onCopyUsername={() =>
                                  copy(item, {
                                    type:
                                      itemAccount(item) === item.username ? "username" : "email",
                                  })
                                }
                                onContextMenu={(event) => onRowContextMenu(event, item)}
                                copied={rowCopied(item)}
                                menu={
                                  <Menu
                                    label={t("passwords.actions")}
                                    triggerLabel={`${t("passwords.actions")} · ${itemName(item)}`}
                                    title={t("passwords.actions")}
                                    align="end"
                                    sections={menu.sections(item)}
                                    onSelect={(id) =>
                                      menu.run(item, id, (part) => flash(`${item.id}:${part.type}`))
                                    }
                                    data-testid="password-row-menu"
                                    triggerClassName="inline-flex h-7 w-7 items-center justify-center rounded-6 text-fg-muted transition-colors hover:bg-inset2 hover:text-fg"
                                    trigger={<Icon name="more" size={16} />}
                                  />
                                }
                              />
                            ))}
                          </div>
                        )}
                      </section>
                    );
                  })}
                </div>
              )}
            </Card>
            {/* The details stay in view while the list scrolls: their own scroll, under the title bar. */}
            <div className={cx("sticky top-[14px] min-w-0")}>
              <ItemDetail item={selected} />
            </div>
          </div>
          <ContextMenu
            at={context?.at ?? null}
            label={t("passwords.actions")}
            sections={context === null ? [] : menu.sections(context.item)}
            onSelect={(id) => {
              if (context !== null)
                menu.run(context.item, id, (part) => flash(`${context.item.id}:${part.type}`));
            }}
            onClose={() => setContext(null)}
            data-testid="password-context"
          />
        </>
      )}
    </div>
  );
}

/** The header's facts: the logins, the groups and, in a sync space, how it stands. */
function PasswordsFacts({ count, groups }: { count: number; groups: number }) {
  const t = useT();
  const { sync } = useUiState();
  const facts = [count === 0 ? t("passwords.none") : t("passwords.count", { n: count })];
  if (groups > 0) facts.push(t("codes.groupsCount", { n: groups }));
  const state = sync.space?.status.state;
  if (state === "synced") facts.push(t("codes.allSynced"));
  else if (state === "syncing") facts.push(t("codes.hero.syncing"));
  else if (state === "failed") facts.push(t("codes.hero.syncFailed"));
  return facts.join(" · ");
}

/** The page's main action (board 04-f): a new login, a generated password, an import. */
export function AddItemMenu() {
  const t = useT();
  const shell = useShell();
  const sections: MenuSection[] = [
    {
      items: [
        { kind: "action", id: "new", label: `${t("passwords.add.new")} · Ctrl N`, icon: "plus" },
        {
          kind: "action",
          id: "generate",
          label: `${t("passwords.add.generate")} · Ctrl G`,
          icon: "dice",
        },
      ],
    },
    {
      items: [{ kind: "action", id: "import", label: t("passwords.add.import"), icon: "download" }],
    },
  ];
  const onSelect = (id: string) => {
    if (id === "new") shell.open({ type: "item_new" });
    else if (id === "generate") shell.open({ type: "generator" });
    else shell.navigate("import");
  };
  return (
    <Menu
      label={t("passwords.add.button")}
      title={`${t("passwords.add.button")} · Ctrl N`}
      align="end"
      sections={sections}
      onSelect={onSelect}
      data-testid="add-item-menu"
      triggerClassName="inline-flex h-13.5 items-center gap-2 rounded-pill bg-primary px-5 text-[14px] font-medium whitespace-nowrap text-primary-fg shadow-primary transition-opacity hover:opacity-90"
      trigger={
        <>
          <Icon name="plus" size={18} />
          {t("passwords.add.button")}
        </>
      }
    />
  );
}
