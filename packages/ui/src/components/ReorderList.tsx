// The code list while it is reordered, on the desktop and the phone alike: the groups (their
// sections, "no group" staying last) and, inside each, the accounts move by their handles. A move
// is handed on as the whole new order: the accounts' ids as the list shows them (every section,
// folded or not, in turn), or the groups' names. Pinned accounts still come first in a section.
import {
  type EntryView,
  NO_GROUP,
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
  size = "md",
}: ReorderListProps) {
  const sorted = sortEntries(entries, sort, entryOrder);
  const ids = sorted.map((e) => e.id);
  const sections = grouped
    ? groupSections(sorted, groupOrder)
    : [{ key: NO_GROUP, entries: sorted }];
  const groups = sections.map((s) => s.key).filter((key) => key !== NO_GROUP);
  const moveEntry = (id: string, over: string) => onOrderEntries(moveItem(ids, id, over));
  const section = (
    key: string,
    items: readonly EntryView[],
    handle?: Parameters<typeof DragHandle>[0]["handle"],
  ) => {
    const name = key === NO_GROUP ? noGroupLabel : key;
    const open = !grouped || !folded.has(key);
    return (
      <section aria-label={grouped ? name : undefined} data-testid="reorder-section">
        {grouped && (
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
  const last = sections.find((s) => s.key === NO_GROUP);
  return (
    <div data-testid="reorder-list">
      {grouped ? (
        <>
          <SortableList
            ids={groups.map((g) => GROUP + g)}
            onMove={(id, over) =>
              onOrderGroups(moveItem(groups, id.slice(GROUP.length), over.slice(GROUP.length)))
            }>
            {sections
              .filter((s) => s.key !== NO_GROUP)
              .map((s) => (
                <SortableItem key={s.key} id={GROUP + s.key} label={s.key}>
                  {(handle) => section(s.key, s.entries, handle)}
                </SortableItem>
              ))}
          </SortableList>
          {last && section(last.key, last.entries)}
        </>
      ) : (
        last && section(last.key, last.entries)
      )}
    </div>
  );
}
