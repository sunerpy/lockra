// The accounts and their codes (board A2): a large title with the vault's state, add and lock; the
// search; the account copied last in a card of its own; the filters (全部, 收藏, each group, 未分组)
// and the reorder button; then every account, each row a card of its own, or cards two to a row
// (验证码视图, in Settings › 外观). A tap copies the code, a long press or ⋯ opens the account's
// actions; in 全部, with groups, the accounts are in sections that fold (the group pinned in
// Settings first, 收藏 gathering the pinned accounts), and a search shows what it finds in every
// section, folded or not; any other filter is one flat list, and the screen opens on the pinned
// one. The reorder button shows the list with handles: the groups (未分组 among them) and the
// accounts inside them move by dragging the handles, an order this phone keeps. The dock at the
// bottom scans a QR code and opens the settings.
import {
  ALL_GROUPS,
  FAVORITES,
  NO_GROUP,
  entryGroups,
  filterEntries,
  groupSections,
  pinnedFilter,
  recentEntry,
  sortEntries,
} from "@lockra/shared";
import {
  Button,
  Card,
  EmptyState,
  EntryRow,
  EntryTile,
  FilterChips,
  Icon,
  IconButton,
  Input,
  Lamp,
  RecentCard,
  ReorderList,
  motionReduced,
  noteUserLock,
  useClock,
  useCodes,
  useDispatch,
  useFilterChips,
  useFlash,
  useT,
  useUiState,
} from "@lockra/ui";
import { type ReactNode, useMemo, useRef, useState } from "react";
import { useNav } from "../app/nav";
import { usePhoneImport } from "../app/phone-import";
import { Dock } from "../components/Dock";
import { RoundButton } from "../components/RoundButton";

export function Codes() {
  const t = useT();
  const nav = useNav();
  const dispatch = useDispatch();
  const codes = useCodes();
  const now = useClock();
  const { scan } = usePhoneImport();
  const scroller = useRef<HTMLElement>(null);
  const {
    settings,
    entries,
    collapsed_groups: collapsedGroups,
    entry_order: entryOrder,
    group_order: groupOrder,
    pinned_group: pinned,
    features,
  } = useUiState();
  const [query, setQuery] = useState("");
  const pinnedValue = pinnedFilter(pinned);
  // The screen opens on the pinned group, and turns to a group as it is pinned.
  const [group, setGroup] = useState(pinnedValue);
  const [pinSeen, setPinSeen] = useState(pinnedValue);
  if (pinSeen !== pinnedValue) {
    setPinSeen(pinnedValue);
    if (pinnedValue !== ALL_GROUPS) setGroup(pinnedValue);
  }
  const [reordering, setReordering] = useState(false);
  // The row or card just copied says so for a moment.
  const [copiedId, flash] = useFlash();
  const cards = settings.codes_view === "grid";
  const copy = (id: string) =>
    void dispatch({ command: "entry_copy", id }).then((done) => {
      if (done !== undefined) flash(id);
    });
  const groups = useMemo(() => entryGroups(entries), [entries]);
  // The filters, as on the desktop: everything, the pinned accounts, each group, those in none;
  // the pinned group first.
  const filters = useFilterChips(entries, pinned);
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
  // Sections in 全部 only: a filter shows its accounts as one flat list.
  const sectioned = settings.group_codes && activeGroup === ALL_GROUPS;
  const searching = query.trim() !== "";
  const folded = new Set(collapsedGroups);
  const sections = sectioned
    ? groupSections(visible, groupOrder, pinned)
    : [{ key: NO_GROUP, entries: visible }];
  // Headings when there is something to tell apart: groups, or the pinned accounts' own section.
  const grouped = sectioned && (groups.length > 0 || sections.some((s) => s.key !== NO_GROUP));
  // The card of the account copied last, else the first of the list; not while searching.
  const recent = recentEntry(entries) ?? sorted[0];
  // A long press: the webview hears it as a context menu.
  const actions = (id: string) => (event: { preventDefault: () => void }) => {
    event.preventDefault();
    nav.open({ name: "account", id });
  };
  const more = (id: string) => (
    <IconButton
      icon="more"
      label={t("codes.actions")}
      size={40}
      onClick={() => nav.open({ name: "account", id })}
      data-testid="row-more"
    />
  );
  const next = (id: string) => () => void dispatch({ command: "entry_hotp_next", id });
  const toggleFold = (key: string) =>
    void dispatch({
      command: "view_collapse_groups",
      groups: folded.has(key) ? [...folded].filter((k) => k !== key) : [...folded, key],
    });
  const lock = () => {
    // The user's own lock: the fingerprint waits until they leave and come back.
    noteUserLock();
    void dispatch({ command: "vault_lock" });
  };
  return (
    <div className="flex h-full flex-col pt-[env(safe-area-inset-top)]" data-testid="page-codes">
      <main
        ref={scroller}
        className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-[calc(env(safe-area-inset-bottom)+7rem)]">
        <header className="mb-4 flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <h1 className="text-[30px] leading-tight font-bold text-fg">{t("codes.title")}</h1>
            <p className="mt-1 flex items-center gap-2 text-[13px] text-fg-muted">
              <Lamp tone="ok" size={8} />
              <span className="truncate" data-testid="codes-subtitle">
                {`${t("shell.status.unlocked")} · ${t("common.accounts", { n: entries.length })}`}
              </span>
            </p>
          </div>
          <RoundButton
            icon="plus"
            label={t("codes.add.label")}
            onClick={() => nav.open({ name: "add" })}
            testId="codes-add"
          />
          <RoundButton icon="lock" label={t("shell.nav.lock")} onClick={lock} testId="codes-lock" />
        </header>
        {entries.length === 0 ? (
          <EmptyState
            icon="key"
            title={t("codes.emptyTitle")}
            actions={
              <Button
                variant="primary"
                size="lg"
                icon="plus"
                onClick={() => nav.open({ name: "add" })}>
                {t("mobile.add.title")}
              </Button>
            }>
            {t("mobile.emptyBody")}
          </EmptyState>
        ) : reordering ? (
          <div className="flex flex-col gap-3" data-testid="codes-reordering">
            <div className="flex items-center gap-3">
              <p className="min-w-0 flex-1 text-[13px] text-fg-muted">{t("mobile.reorderHint")}</p>
              <Button
                variant="primary"
                size="lg"
                onClick={() => setReordering(false)}
                data-testid="codes-reorder-done">
                {t("codes.reorder.done")}
              </Button>
            </div>
            <Card padding="none" className="p-1.5">
              <ReorderList
                size="lg"
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
                favoritesLabel={t("codes.chips.favorites")}
                pinned={pinned}
              />
            </Card>
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <Input
              size="xl"
              pill
              icon="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("codes.search")}
              aria-label={t("codes.search")}
              enterKeyHint="search"
              data-testid="codes-search"
            />
            {recent !== undefined && !searching && (
              <RecentCard
                size="sm"
                entry={recent}
                code={codes.get(recent.id)}
                nowMs={now}
                masked={settings.hide_codes}
                still={motionReduced(settings)}
                onCopy={() => copy(recent.id)}
                onNext={recent.kind.type === "hotp" ? next(recent.id) : undefined}
              />
            )}
            {(filters.length > 1 || entries.length > 1) && (
              <div className="flex min-w-0 items-center gap-2">
                {filters.length > 1 ? (
                  <FilterChips
                    label={t("codes.chips.label")}
                    options={filters}
                    value={activeGroup}
                    onChange={setGroup}
                    scroll
                    size="lg"
                    className="min-w-0 flex-1"
                    data-testid="codes-chips"
                  />
                ) : (
                  <span className="flex-1" />
                )}
                {entries.length > 1 && (
                  <RoundButton
                    icon="drag"
                    label={t("codes.reorder.open")}
                    onClick={() => {
                      setQuery("");
                      setGroup(ALL_GROUPS);
                      setReordering(true);
                    }}
                    testId="codes-reorder"
                  />
                )}
              </div>
            )}
            {visible.length === 0 ? (
              <EmptyState
                compact
                icon="search"
                title={t("codes.noMatch", { query: query.trim() })}
              />
            ) : (
              <Sections cards={cards}>
                {sections.map((section) => {
                  // The pinned accounts' section, at the top while 收藏 is pinned, does not fold.
                  const favorites = section.key === FAVORITES;
                  const name = favorites
                    ? t("codes.chips.favorites")
                    : section.key === NO_GROUP
                      ? t("codes.groupNone")
                      : section.key;
                  const open = !grouped || searching || favorites || !folded.has(section.key);
                  return (
                    <section
                      key={section.key === NO_GROUP ? "\u0000" : section.key}
                      aria-label={grouped ? name : undefined}>
                      {grouped && favorites && (
                        <div
                          data-testid="codes-pinned-heading"
                          className="flex h-11 w-full items-center gap-2 px-2 text-[13px] font-medium text-fg-muted">
                          <Icon name="star" size={16} className="text-warning" />
                          <span className="min-w-0 truncate">{name}</span>
                          <span className="ml-auto mono text-[12px] text-fg-subtle">
                            {section.entries.length}
                          </span>
                        </div>
                      )}
                      {grouped && !favorites && (
                        <button
                          type="button"
                          aria-expanded={open}
                          disabled={searching}
                          onClick={() => toggleFold(section.key)}
                          data-testid="codes-group-toggle"
                          className="flex h-11 w-full items-center gap-2 rounded-6 px-2 text-left text-[13px] font-medium text-fg-muted">
                          <Icon name={open ? "chevronDown" : "chevronRight"} size={16} />
                          <span className="min-w-0 truncate">{name}</span>
                          <span className="ml-auto mono text-[12px] text-fg-subtle">
                            {section.entries.length}
                          </span>
                        </button>
                      )}
                      {open && (
                        <div
                          role="list"
                          aria-label={grouped ? name : t("codes.title")}
                          className={cards ? "grid grid-cols-2 gap-2.5" : "flex flex-col gap-2.5"}>
                          {section.entries.map((entry) => (
                            <div
                              role="listitem"
                              key={entry.id}
                              className={cards ? "grid min-w-0" : undefined}>
                              {cards ? (
                                <EntryTile
                                  size="sm"
                                  entry={entry}
                                  code={codes.get(entry.id)}
                                  nowMs={now}
                                  masked={settings.hide_codes}
                                  still={motionReduced(settings)}
                                  copied={
                                    copiedId === entry.id ? t("notice.copiedKept") : undefined
                                  }
                                  onCopy={() => copy(entry.id)}
                                  onNext={entry.kind.type === "hotp" ? next(entry.id) : undefined}
                                  onContextMenu={actions(entry.id)}
                                  menu={more(entry.id)}
                                />
                              ) : (
                                <EntryRow
                                  card
                                  entry={entry}
                                  code={codes.get(entry.id)}
                                  nowMs={now}
                                  masked={settings.hide_codes}
                                  still={motionReduced(settings)}
                                  copied={
                                    copiedId === entry.id ? t("notice.copiedKept") : undefined
                                  }
                                  onCopy={() => copy(entry.id)}
                                  onNext={entry.kind.type === "hotp" ? next(entry.id) : undefined}
                                  onContextMenu={actions(entry.id)}
                                  menu={more(entry.id)}
                                />
                              )}
                            </div>
                          ))}
                        </div>
                      )}
                    </section>
                  );
                })}
              </Sections>
            )}
          </div>
        )}
      </main>
      <Dock
        page="codes"
        onCodes={() => scroller.current?.scrollTo({ top: 0, behavior: "smooth" })}
        onLogins={features.passwords ? () => nav.open({ name: "logins" }) : undefined}
        onRaised={() =>
          void (async () => {
            if (await scan()) nav.open({ name: "preview" });
          })()
        }
        onSettings={() => nav.open({ name: "settings" })}
      />
    </div>
  );
}

/** Rows and cards sit on the canvas, each a card of its own (board A2). */
function Sections({ cards, children }: { cards: boolean; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2" data-testid={cards ? "codes-grid" : "codes-list"}>
      {children}
    </div>
  );
}
