// The accounts and their codes: a tap copies the code; with groups, the accounts are in sections
// that fold; a search shows what it finds in every section, folded or not.
import {
  ALL_GROUPS,
  NO_GROUP,
  entryGroups,
  filterEntries,
  groupSections,
  sortEntries,
} from "@lockra/shared";
import {
  Card,
  EmptyState,
  EntryRow,
  Icon,
  IconButton,
  Input,
  motionReduced,
  useClock,
  useCodes,
  useDispatch,
  useT,
  useUiState,
} from "@lockra/ui";
import { useMemo, useState } from "react";

export function Codes() {
  const t = useT();
  const dispatch = useDispatch();
  const codes = useCodes();
  const now = useClock();
  const { settings, entries, collapsed_groups: collapsedGroups } = useUiState();
  const [query, setQuery] = useState("");
  const visible = useMemo(
    () => filterEntries(sortEntries(entries, settings.sort), query, ALL_GROUPS),
    [entries, settings.sort, query],
  );
  const grouped = settings.group_codes && entryGroups(entries).length > 0;
  const searching = query.trim() !== "";
  const folded = new Set(collapsedGroups);
  const sections = grouped ? groupSections(visible) : [{ key: NO_GROUP, entries: visible }];
  const toggleFold = (key: string) =>
    void dispatch({
      command: "view_collapse_groups",
      groups: folded.has(key) ? [...folded].filter((k) => k !== key) : [...folded, key],
    });
  return (
    <div className="flex h-full flex-col" data-testid="page-codes">
      <header className="flex items-center gap-2 border-b border-border bg-surface px-4 pt-[max(env(safe-area-inset-top),0.5rem)] pb-1">
        <h1 className="flex-1 text-[18px] font-semibold text-fg">{t("codes.title")}</h1>
        <IconButton
          icon="lock"
          label={t("shell.nav.lock")}
          size={40}
          onClick={() => void dispatch({ command: "vault_lock" })}
          data-testid="codes-lock"
        />
      </header>
      <main className="min-h-0 flex-1 overflow-y-auto px-3 pt-3 pb-[max(env(safe-area-inset-bottom),0.75rem)]">
        {entries.length === 0 ? (
          <EmptyState icon="key" title={t("codes.emptyTitle")}>
            {t("mobile.emptyBody")}
          </EmptyState>
        ) : (
          <div className="flex flex-col gap-3">
            <Input
              size="lg"
              icon="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("codes.search")}
              aria-label={t("codes.search")}
              enterKeyHint="search"
              data-testid="codes-search"
            />
            {visible.length === 0 ? (
              <EmptyState
                compact
                icon="search"
                title={t("codes.noMatch", { query: query.trim() })}
              />
            ) : (
              <Card padding="none" className="p-1.5" data-testid="codes-list">
                {sections.map((section) => {
                  const name = section.key === NO_GROUP ? t("codes.groupNone") : section.key;
                  const open = !grouped || searching || !folded.has(section.key);
                  return (
                    <section
                      key={section.key === NO_GROUP ? "\u0000" : section.key}
                      aria-label={grouped ? name : undefined}>
                      {grouped && (
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
                        <div role="list" aria-label={grouped ? name : t("codes.title")}>
                          {section.entries.map((entry) => (
                            <div role="listitem" key={entry.id}>
                              <EntryRow
                                entry={entry}
                                code={codes.get(entry.id)}
                                nowMs={now}
                                masked={settings.hide_codes}
                                still={motionReduced(settings)}
                                onCopy={() =>
                                  void dispatch({ command: "entry_copy", id: entry.id })
                                }
                                onNext={
                                  entry.kind.type === "hotp"
                                    ? () =>
                                        void dispatch({ command: "entry_hotp_next", id: entry.id })
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
              </Card>
            )}
          </div>
        )}
      </main>
    </div>
  );
}
