// The logins (boards 21, 22): a large title with the vault's state, the generator and the lock; the
// search; the filters (全部, 收藏, each group, 未分组) and the tags; then every login, each a card of
// its own, in sections that fold (this phone keeps which, as the desktop does its own). A tap opens
// the login, its key button copies the password, a long press or ⋯ opens its actions. The dock's
// raised button adds a login. The logins come from `items_list` (`useItems`), asked for again
// whenever the state's `items_rev` moves.
import {
  ALL_GROUPS,
  type ItemView,
  NO_GROUP,
  errorText,
  filterItems,
  itemAccount,
  itemGroups,
  itemSections,
  itemTags,
  sortItems,
} from "@lockra/shared";
import {
  Banner,
  Button,
  EmptyState,
  FilterChips,
  Icon,
  IconButton,
  Input,
  Lamp,
  PasswordRow,
  SkeletonRows,
  TagFilter,
  noteUserLock,
  useDispatch,
  useFlash,
  useItemFilterChips,
  useItems,
  useItemsFailure,
  useT,
  useUiState,
} from "@lockra/ui";
import { useMemo, useRef, useState } from "react";
import { useNav } from "../app/nav";
import { useItemActions } from "../app/verify";
import { Dock } from "../components/Dock";
import { DeleteLoginDialog } from "../components/LoginParts";
import { LoginSheet } from "../components/LoginSheet";
import { RoundButton } from "../components/RoundButton";
import { GeneratorSheet } from "../components/GeneratorSheet";

export function Logins() {
  const t = useT();
  const nav = useNav();
  const dispatch = useDispatch();
  const actions = useItemActions();
  const scroller = useRef<HTMLElement>(null);
  const { settings, item_collapsed_groups: collapsed } = useUiState();
  const loaded = useItems();
  const failure = useItemsFailure();
  const items = useMemo(() => loaded ?? [], [loaded]);
  const [query, setQuery] = useState("");
  const [group, setGroup] = useState(ALL_GROUPS);
  const [tags, setTags] = useState<string[]>([]);
  const [sheet, setSheet] = useState<ItemView | null>(null);
  const [deleting, setDeleting] = useState<ItemView | null>(null);
  const [generating, setGenerating] = useState(false);
  const [copiedId, flash] = useFlash();
  const filters = useItemFilterChips(items);
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
  const grouped = activeGroup === ALL_GROUPS && groups.length > 0;
  const sections = useMemo(
    () => (grouped ? itemSections(visible) : [{ key: NO_GROUP, items: visible }]),
    [grouped, visible],
  );
  const folded = useMemo(() => new Set(collapsed), [collapsed]);
  const searching = query.trim() !== "";
  const toggleFold = (key: string) =>
    void dispatch({
      command: "view_collapse_items",
      groups: folded.has(key) ? [...folded].filter((k) => k !== key) : [...folded, key],
    });
  const copyPassword = (item: ItemView) =>
    void actions.copy(item, { type: "password" }).then((done) => {
      if (done) flash(item.id);
    });
  const lock = () => {
    // The user's own lock: the fingerprint waits until they leave and come back.
    noteUserLock();
    void dispatch({ command: "vault_lock" });
  };
  const loading = loaded === undefined;
  return (
    <div className="flex h-full flex-col pt-[env(safe-area-inset-top)]" data-testid="page-logins">
      <main
        ref={scroller}
        className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-[calc(env(safe-area-inset-bottom)+7rem)]">
        <header className="mb-4 flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <h1 className="text-[30px] leading-tight font-bold text-fg">{t("passwords.title")}</h1>
            <p className="mt-1 flex items-center gap-2 text-[13px] text-fg-muted">
              <Lamp tone="ok" size={8} />
              <span className="truncate" data-testid="logins-subtitle">
                {`${t("shell.status.unlocked")} · ${items.length === 0 ? t("passwords.none") : t("passwords.count", { n: items.length })}`}
              </span>
            </p>
          </div>
          <RoundButton
            icon="dice"
            label={t("generator.title")}
            onClick={() => setGenerating(true)}
            testId="logins-generate"
          />
          <RoundButton
            icon="lock"
            label={t("shell.nav.lock")}
            onClick={lock}
            testId="logins-lock"
          />
        </header>
        {failure.error !== null && (
          <Banner
            tone="danger"
            marker="bar"
            title={t("passwords.loadFailed")}
            className="mb-4"
            actions={
              <Button size="sm" variant="outline" onClick={failure.retry}>
                {t("passwords.retry")}
              </Button>
            }>
            {errorText(t, failure.error)}
          </Banner>
        )}
        {loading ? (
          <SkeletonRows label={t("passwords.loading")} />
        ) : items.length === 0 ? (
          <EmptyState
            icon="lock"
            title={t("passwords.emptyTitle")}
            actions={
              <>
                <Button
                  variant="primary"
                  size="lg"
                  icon="download"
                  onClick={() => nav.open({ name: "add" })}>
                  {t("passwords.emptyImport")}
                </Button>
                <Button size="lg" icon="plus" onClick={() => nav.open({ name: "loginEdit" })}>
                  {t("mobile.dock.newLogin")}
                </Button>
              </>
            }>
            {t("mobile.logins.emptyBody")}
          </EmptyState>
        ) : (
          <div className="flex flex-col gap-4">
            <Input
              size="xl"
              pill
              icon="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("passwords.search")}
              aria-label={t("passwords.search")}
              enterKeyHint="search"
              data-testid="logins-search"
            />
            {(filters.length > 1 || tagCounts.length > 0) && (
              <div className="flex min-w-0 items-center gap-2">
                {filters.length > 1 && (
                  <FilterChips
                    label={t("codes.chips.label")}
                    options={filters}
                    value={activeGroup}
                    onChange={setGroup}
                    scroll
                    size="lg"
                    className="min-w-0 flex-1"
                    data-testid="logins-chips"
                  />
                )}
                {tagCounts.length > 0 && (
                  <TagFilter
                    tags={tagCounts}
                    value={activeTags}
                    onChange={setTags}
                    data-testid="logins-tags"
                  />
                )}
              </div>
            )}
            {visible.length === 0 ? (
              <EmptyState
                compact
                icon="search"
                title={t("passwords.noMatch", { query: query.trim() })}
                actions={
                  searching ? (
                    <Button
                      size="lg"
                      icon="plus"
                      onClick={() => nav.open({ name: "loginEdit", title: query.trim() })}
                      data-testid="logins-create-from-search">
                      {t("passwords.noMatchCreate", { query: query.trim() })}
                    </Button>
                  ) : undefined
                }>
                {t("passwords.noMatchBody")}
              </EmptyState>
            ) : (
              <div className="flex flex-col gap-2" data-testid="logins-list">
                {sections.map((section) => {
                  const name = section.key === NO_GROUP ? t("codes.groupNone") : section.key;
                  const open = !grouped || searching || !folded.has(section.key);
                  return (
                    <section
                      key={section.key === NO_GROUP ? "\u0000" : section.key}
                      aria-label={grouped ? name : undefined}
                      data-testid={grouped ? "logins-section" : undefined}>
                      {grouped && (
                        <button
                          type="button"
                          aria-expanded={open}
                          disabled={searching}
                          onClick={() => toggleFold(section.key)}
                          data-testid="logins-group-toggle"
                          className="flex h-[38.5px] w-full items-center gap-2 rounded-6 px-2 text-left text-[13px] font-medium text-fg-muted">
                          <Icon name={open ? "chevronDown" : "chevronRight"} size={16} />
                          <span
                            className="min-w-0 truncate"
                            {...(section.key === NO_GROUP ? {} : { "data-user-text": "" })}>
                            {name}
                          </span>
                          <span className="ml-auto mono text-[12px] text-fg-subtle">
                            {section.items.length}
                          </span>
                        </button>
                      )}
                      {open && (
                        <div
                          role="list"
                          aria-label={grouped ? name : t("passwords.list")}
                          className="flex flex-col gap-[8.75px]">
                          {section.items.map((item) => (
                            <div role="listitem" key={item.id}>
                              <PasswordRow
                                card
                                item={item}
                                onSelect={() => nav.open({ name: "login", id: item.id })}
                                onCopyPassword={() => copyPassword(item)}
                                onCopyUsername={() =>
                                  void actions.copy(item, {
                                    type:
                                      itemAccount(item) === item.username ? "username" : "email",
                                  })
                                }
                                onContextMenu={(event) => {
                                  // A long press: the webview hears it as a context menu.
                                  event.preventDefault();
                                  setSheet(item);
                                }}
                                copied={
                                  copiedId === item.id
                                    ? t("notice.itemCopiedKept", {
                                        part: t("notice.copiedPart.password"),
                                      })
                                    : undefined
                                }
                                menu={
                                  <IconButton
                                    icon="more"
                                    label={t("passwords.actions")}
                                    size={40}
                                    onClick={() => setSheet(item)}
                                    data-testid="login-more"
                                  />
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
            )}
          </div>
        )}
      </main>
      <Dock
        page="logins"
        onCodes={nav.home}
        onLogins={() => scroller.current?.scrollTo({ top: 0, behavior: "smooth" })}
        onRaised={() => nav.open({ name: "loginEdit" })}
        onSettings={() => nav.open({ name: "settings" })}
      />
      {sheet !== null && (
        <LoginSheet
          item={sheet}
          onClose={() => setSheet(null)}
          onDelete={() => setDeleting(sheet)}
          onCopied={(part) => {
            if (part.type === "password") flash(sheet.id);
          }}
        />
      )}
      {deleting !== null && (
        <DeleteLoginDialog
          item={deleting}
          onClose={() => setDeleting(null)}
          onDeleted={() => setDeleting(null)}
        />
      )}
      {generating && <GeneratorSheet onClose={() => setGenerating(false)} />}
    </div>
  );
}
