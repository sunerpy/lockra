// The code list while it is reordered, on the desktop and the phone alike: the groups (their
// sections, "no group" among them) and, inside each, the accounts move by their handles; the
// section pinned at the top stays there. A move is handed on as the whole new order: the accounts'
// ids as the list shows them (every section, folded or not, in turn), or the groups' names ("" for
// no group), the pinned group first. Pinned accounts still come first in a section, or gather in
// their own section when 收藏 is the pinned one.
import {
  type EntryView,
  FAVORITES,
  NO_GROUP,
  type PinnedGroup,
  type SortOrder,
  groupSections,
  moveItem,
  sortEntries,
} from "@lockra/shared";
import { cx } from "../cx";
import { EntryAvatar } from "./EntryAvatar";
import { Icon } from "./Icon";
import { DragHandle, SortableItem, SortableList } from "./Sortable";

/** A group's id among the sortable sections; accounts' ids are UUIDs, so the two never meet. */
const GROUP = "group:";

export interface ReorderListProps {
  entries: readonly EntryView[];
  sort: SortOrder;
  entryOrder: readonly string[];
  groupOrder: readonly string[];
  /** Sections by group. */
  grouped: boolean;
  folded: ReadonlySet<string>;
  onToggleFold: (key: string) => void;
  onOrderEntries: (ids: string[]) => void;
  onOrderGroups: (groups: string[]) => void;
  /** The section of the accounts in no group. */
  noGroupLabel: string;
  /** The section of the pinned accounts, when 收藏 is the pinned group. */
  favoritesLabel: string;
  /** The group pinned at the top: its section stays first. */
  pinned?: PinnedGroup | null;
  /** `lg` is the phone's: 44 px handles. */
  size?: "md" | "lg";
}

export function ReorderList({
  entries,
  sort,
  entryOrder,
  groupOrder,
  grouped,
  folded,
  onToggleFold,
  onOrderEntries,
  onOrderGroups,
  noGroupLabel,
  favoritesLabel,
  pinned = null,
  size = "md",
}: ReorderListProps) {
  const sorted = sortEntries(entries, sort, entryOrder);
  const ids = sorted.map((e) => e.id);
  const sections = grouped
    ? groupSections(sorted, groupOrder, pinned)
    : [{ key: NO_GROUP, entries: sorted }];
  // The pinned section stays first; the others move, no group among them.
  const fixed =
    grouped &&
    sections[0] !== undefined &&
    (sections[0].key === FAVORITES ||
      (pinned?.type === "group" && sections[0].key === pinned.name) ||
      (pinned?.type === "ungrouped" && sections[0].key === NO_GROUP))
      ? sections[0]
      : undefined;
  const movable = fixed ? sections.slice(1) : sections;
  const groups = movable.map((s) => s.key);
  // A pinned group keeps the first place in the order too: unpinned later, it stays where it was.
  const orderGroups = (order: string[]) =>
    onOrderGroups(fixed && fixed.key !== FAVORITES ? [fixed.key, ...order] : order);
  const moveEntry = (id: string, over: string) => onOrderEntries(moveItem(ids, id, over));
  const section = (
    key: string,
    items: readonly EntryView[],
    handle?: Parameters<typeof DragHandle>[0]["handle"],
  ) => {
    const favorites = key === FAVORITES;
    const name = favorites ? favoritesLabel : key === NO_GROUP ? noGroupLabel : key;
    // The pinned accounts' section does not fold: it is what the pin keeps in sight.
    const open = !grouped || favorites || !folded.has(key);
    return (
      <section aria-label={grouped ? name : undefined} data-testid="reorder-section">
        {grouped && favorites && (
          <div
            className={cx(
              "flex items-center gap-1.5 px-2 font-medium text-fg-muted",
              size === "lg" ? "h-11 text-[13px]" : "h-8 text-[12px]",
            )}>
            <Icon name="star" size={size === "lg" ? 16 : 14} className="text-warning" />
            <span className="min-w-0 truncate">{name}</span>
            <span className="ml-auto mono text-[11px] text-fg-subtle">{items.length}</span>
          </div>
        )}
        {grouped && !favorites && (
          <div className={cx("flex items-center gap-1", size === "lg" ? "h-11" : "h-8")}>
            <button
              type="button"
              aria-expanded={open}
              onClick={() => onToggleFold(key)}
              className={cx(
                "flex min-w-0 flex-1 items-center gap-1.5 rounded-6 px-2 text-left font-medium text-fg-muted outline-none transition-colors hover:bg-inset focus-visible:bg-inset",
                size === "lg" ? "h-11 text-[13px]" : "h-8 text-[12px]",
              )}>
              <Icon name={open ? "chevronDown" : "chevronRight"} size={size === "lg" ? 16 : 14} />
              <span
                className="min-w-0 truncate"
                {...(key === NO_GROUP ? {} : { "data-user-text": "" })}>
                {name}
              </span>
              <span className="ml-auto mono text-[11px] text-fg-subtle">{items.length}</span>
            </button>
            {handle && <DragHandle handle={handle} size={size === "lg" ? 44 : 28} />}
          </div>
        )}
        {open && (
          <SortableList ids={items.map((e) => e.id)} onMove={moveEntry}>
            <div role="list" aria-label={name}>
              {items.map((entry) => (
                <SortableItem key={entry.id} id={entry.id} label={entry.issuer || entry.account}>
                  {(row) => (
                    <div
                      role="listitem"
                      data-testid="reorder-row"
                      className={cx(
                        "grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 rounded-10 bg-surface px-3",
                        size === "lg" ? "min-h-14 py-1.5" : "h-12",
                        row.dragging && "hairline",
                      )}>
                      <EntryAvatar
                        issuer={entry.issuer}
                        account={entry.account}
                        color={entry.color}
                        mark={entry.mark}
                      />
                      <div className="min-w-0">
                        <div className="flex min-w-0 items-center gap-1.5">
                          {entry.favorite && (
                            <Icon name="star" size={12} className="shrink-0 text-accent-text" />
                          )}
                          <span
                            className="truncate text-[14px] font-medium text-fg"
                            data-user-text="">
                            {entry.issuer || entry.account}
                          </span>
                        </div>
                        {entry.issuer !== "" && (
                          <div className="truncate text-[12px] text-fg-muted" data-user-text="">
                            {entry.account}
                          </div>
                        )}
                      </div>
                      <DragHandle handle={row} size={size === "lg" ? 44 : 28} />
                    </div>
                  )}
                </SortableItem>
              ))}
            </div>
          </SortableList>
        )}
      </section>
    );
  };
  const only = sections[0];
  return (
    <div data-testid="reorder-list">
      {grouped ? (
        <>
          {fixed && section(fixed.key, fixed.entries)}
          <SortableList
            ids={groups.map((g) => GROUP + g)}
            onMove={(id, over) =>
              orderGroups(moveItem(groups, id.slice(GROUP.length), over.slice(GROUP.length)))
            }>
            {movable.map((s) => (
              <SortableItem
                key={s.key === NO_GROUP ? "\u0000" : s.key}
                id={GROUP + s.key}
                label={s.key === NO_GROUP ? noGroupLabel : s.key}>
                {(handle) => section(s.key, s.entries, handle)}
              </SortableItem>
            ))}
          </SortableList>
        </>
      ) : (
        only && section(only.key, only.entries)
      )}
    </div>
  );
}
